//! Render pass: Markdown -> HTML, wrapped in the template for each note's layout.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::Context;
use minijinja::value::Serde;
use minijinja::{Environment, context};
use serde::Serialize;

use comrak::nodes::NodeValue;
use comrak::{Anchorizer, Arena};

use crate::content::{Area, Garden, Layout, Note, Stage, slugify};
use crate::links::{Index, Target};
use crate::loci::{Kind, Ontology, Relation, Thing};
use crate::markdown;

/// What a template is allowed to see. Fields not listed here never reach the HTML,
/// which is how file paths and unused front matter stay out of the output.
#[derive(Serialize)]
struct PageView<'a> {
    slug: &'a str,
    url: String,
    title: &'a str,
    summary: Option<&'a str>,
    stage: Option<&'static str>,
    layout: &'static str,
    area: Option<&'static str>,
    role: Option<&'a str>,
    tags: Vec<TagView<'a>>,
    planted: Option<String>,
    tended: Option<String>,
    reading_minutes: usize,
    locus: Option<&'static str>,
    locus_url: Option<String>,
    era: Option<&'a str>,
    region: Option<&'a str>,
    /// Task checkboxes in the note, e.g. a project's definition of done.
    tasks: Option<Tasks>,
    toc: Vec<TocEntry>,
}

#[derive(Serialize, Clone)]
struct TagView<'a> {
    name: &'a str,
    url: String,
}

#[derive(Serialize, Default, Clone, Copy)]
struct Tasks {
    done: usize,
    total: usize,
}

#[derive(Serialize)]
struct TocEntry {
    level: u8,
    id: String,
    text: String,
}

/// Facts about a note's body that templates show: length, tasks, headings.
struct Outline {
    words: usize,
    tasks: Tasks,
    toc: Vec<TocEntry>,
}

/// Reads at about 230 words a minute; never shows "0 min".
const WORDS_PER_MINUTE: usize = 230;

fn outline(body: &str) -> Outline {
    let options = markdown::options();
    let arena = Arena::new();
    let root = markdown::parse(&arena, body, &options);
    // Same algorithm and order comrak uses for heading ids, so links match.
    let mut anchorizer = Anchorizer::new();
    let mut outline = Outline {
        words: 0,
        tasks: Tasks::default(),
        toc: Vec::new(),
    };
    for node in root.descendants() {
        match &node.data().value {
            NodeValue::Text(t) => outline.words += t.split_whitespace().count(),
            NodeValue::TaskItem(task) => {
                outline.tasks.total += 1;
                if task.symbol.is_some() {
                    outline.tasks.done += 1;
                }
            }
            NodeValue::Heading(h) => {
                let text = node.collect_text();
                let id = anchorizer.anchorize(&text);
                if (2..=3).contains(&h.level) {
                    outline.toc.push(TocEntry {
                        level: h.level,
                        id,
                        text,
                    });
                }
            }
            _ => {}
        }
    }
    outline
}

fn tag_url(tag: &str) -> String {
    format!("/tags/{}/", slugify(tag))
}

/// A link to another page, as shown in "Linked from" and "Links to".
#[derive(Serialize)]
struct LinkView<'a> {
    url: String,
    title: &'a str,
    snippet: Option<&'a str>,
}

impl<'a> From<&'a Note> for PageView<'a> {
    fn from(note: &'a Note) -> Self {
        let outline = outline(&note.body);
        PageView {
            slug: &note.slug,
            url: note_url(note),
            title: &note.title,
            summary: note.meta.summary.as_deref(),
            stage: note.meta.stage.map(|s| match s {
                Stage::Seedling => "seedling",
                Stage::Budding => "budding",
                Stage::Evergreen => "evergreen",
            }),
            layout: match note.meta.layout {
                Layout::Note => "note",
                Layout::Essay => "essay",
                Layout::Project => "project",
                Layout::Log => "log",
                Layout::Index => "index",
                Layout::Home => "home",
            },
            area: note.meta.area.map(|a| match a {
                Area::Professional => "professional",
                Area::Personal => "personal",
            }),
            role: note.meta.role.as_deref(),
            tags: note
                .meta
                .tags
                .iter()
                .map(|t| TagView {
                    name: t,
                    url: tag_url(t),
                })
                .collect(),
            planted: note.meta.planted.map(|d| d.to_string()),
            tended: note.meta.tended.map(|d| d.to_string()),
            locus: note.meta.locus.map(Kind::name),
            locus_url: note.meta.locus.map(|k| format!("/loci/#{}", k.plural())),
            era: note.meta.era.as_deref(),
            region: note.meta.region.as_deref(),
            reading_minutes: outline.words.div_ceil(WORDS_PER_MINUTE).max(1),
            tasks: (outline.tasks.total > 0).then_some(outline.tasks),
            toc: outline.toc,
        }
    }
}

fn template_name(layout: Layout) -> &'static str {
    match layout {
        Layout::Note => "note.html",
        Layout::Essay => "essay.html",
        Layout::Project => "project.html",
        Layout::Log => "log.html",
        Layout::Index => "index.html",
        Layout::Home => "home.html",
    }
}

/// Where a note lives on the site. The home note is the site root.
fn note_url(note: &Note) -> String {
    match note.meta.layout {
        Layout::Home => "/".to_string(),
        _ => format!("/{}/", note.slug),
    }
}

/// A `## Heading` block of the home note, shown as a column.
#[derive(Serialize)]
struct SectionView {
    heading: String,
    html: String,
}

/// Split Markdown into the part before the first `## ` heading and one chunk per
/// `## ` heading. Headings inside fenced code blocks don't count.
fn split_sections(body: &str) -> (String, Vec<(String, String)>) {
    let mut intro = String::new();
    let mut sections: Vec<(String, String)> = Vec::new();
    let mut in_fence = false;
    for line in body.lines() {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
        }
        if !in_fence && let Some(heading) = line.strip_prefix("## ") {
            sections.push((heading.trim().to_string(), String::new()));
            continue;
        }
        let target = match sections.last_mut() {
            Some((_, text)) => text,
            None => &mut intro,
        };
        target.push_str(line);
        target.push('\n');
    }
    (intro, sections)
}

/// Load every `*.html` file in `templates/`. Names ending in `.html` are autoescaped.
fn load_templates(dir: &Path) -> anyhow::Result<Environment<'static>> {
    let mut env = Environment::new();
    for entry in fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
        let path = entry?.path();
        if path.extension().is_some_and(|ext| ext == "html") {
            let name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            let source = fs::read_to_string(&path)?;
            env.add_template_owned(name, source)
                .with_context(|| format!("template {}", path.display()))?;
        }
    }
    Ok(env)
}

/// Refuse to wipe a folder that holds notes, templates, or the project itself.
fn ensure_safe_to_clear(out: &Path, protected: &[&Path]) -> anyhow::Result<()> {
    if !out.exists() {
        return Ok(());
    }
    let out = out.canonicalize()?;
    let cwd = std::env::current_dir()?;
    for path in protected.iter().copied().chain([cwd.as_path()]) {
        let Ok(path) = path.canonicalize() else {
            continue;
        };
        if path.starts_with(&out) {
            anyhow::bail!(
                "refusing to clear {}: it contains {}",
                out.display(),
                path.display()
            );
        }
    }
    Ok(())
}

/// Markdown -> HTML for `body` (all or part of note `i`), with every wikilink
/// rewritten to its resolved target.
fn note_html(
    garden: &Garden,
    index: &Index,
    i: usize,
    body: &str,
    options: &comrak::Options,
) -> String {
    let arena = Arena::new();
    let root = markdown::parse(&arena, body, options);
    for node in root.descendants() {
        let image = match &node.data().value {
            NodeValue::Image(link) => Some(link.url.clone()),
            _ => None,
        };
        if let Some(written) = image {
            let url = match index.images.get(&(i, written.clone())) {
                Some(Some(public)) => public.clone(),
                Some(None) => format!("{}{written}", markdown::MISSING_IMAGE),
                None => written,
            };
            if let NodeValue::Image(ref mut link) = node.data_mut().value {
                link.url = url;
            }
            continue;
        }
        let written = match &node.data().value {
            NodeValue::WikiLink(link) => link.url.clone(),
            _ => continue,
        };
        let (url, title) = match index.resolve(&written, i) {
            Target::Note { index: to, anchor } => {
                let target = &garden.notes[to];
                let url = match anchor {
                    Some(a) if to == i => format!("#{a}"),
                    Some(a) => format!("{}#{a}", note_url(target)),
                    None => note_url(target),
                };
                (url, Some(&target.title))
            }
            Target::Private => (markdown::PRIVATE.to_string(), None),
            Target::Missing => (markdown::MISSING.to_string(), None),
        };
        if let NodeValue::WikiLink(ref mut link) = node.data_mut().value {
            link.url = url;
        }

        // [[file name]] with no custom label shows the note's title instead.
        if let (Some(title), Some(child)) = (title, node.first_child())
            && child.next_sibling().is_none()
            && !written.contains('#')
            && let NodeValue::Text(ref mut text) = child.data_mut().value
            && **text == *written
        {
            *text = title.clone().into();
        }
    }
    markdown::to_html(root, options)
}

/// Everything the render pass reads.
pub struct Site<'a> {
    pub garden: &'a Garden,
    pub index: &'a Index,
    pub ontology: &'a Ontology,
}

/// A link to a note or a generated locus page.
#[derive(Serialize)]
struct RefView {
    title: String,
    url: String,
}

#[derive(Serialize)]
struct RelationItem {
    thing: RefView,
    /// For `displaces`: what was displaced.
    old: Option<RefView>,
}

/// One verb and the things it connects, e.g. "read alongside: A, B".
#[derive(Serialize)]
struct RelationGroup {
    label: &'static str,
    items: Vec<RelationItem>,
}

fn locus_url(kind: Kind, slug: &str) -> String {
    format!("/loci/{}/{slug}/", kind.plural())
}

fn thing_ref(site: &Site, thing: &Thing) -> RefView {
    match thing {
        Thing::Note(i) => RefView {
            title: site.garden.notes[*i].title.clone(),
            url: note_url(&site.garden.notes[*i]),
        },
        Thing::Locus(kind, slug) => RefView {
            title: site.ontology.loci[&(*kind, slug.clone())].name.clone(),
            url: locus_url(*kind, slug),
        },
    }
}

/// Typed relations for note `i`: the ones it declares, then the ones pointing at it.
fn relation_groups(site: &Site, i: usize) -> Vec<RelationGroup> {
    let mut groups = Vec::new();
    for relation in Relation::ALL {
        let items: Vec<RelationItem> = site.ontology.outgoing[i]
            .iter()
            .filter(|e| e.relation == relation)
            .map(|e| RelationItem {
                thing: thing_ref(site, &e.target),
                old: e.old.as_ref().map(|o| thing_ref(site, o)),
            })
            .collect();
        if !items.is_empty() {
            groups.push(RelationGroup {
                label: relation.label(),
                items,
            });
        }
    }
    for relation in Relation::ALL {
        let items: Vec<RelationItem> = site.ontology.incoming[i]
            .iter()
            .filter(|(_, r)| *r == relation)
            .map(|(from, _)| RelationItem {
                thing: thing_ref(site, &Thing::Note(*from)),
                old: None,
            })
            .collect();
        if !items.is_empty() {
            groups.push(RelationGroup {
                label: relation.inverse(),
                items,
            });
        }
    }
    groups
}

/// An entry on the `/loci/` index.
#[derive(Serialize)]
struct LocusEntry {
    title: String,
    url: String,
    /// How many notes name it.
    count: usize,
}

#[derive(Serialize)]
struct KindSection {
    name: &'static str,
    plural: &'static str,
    entries: Vec<LocusEntry>,
}

/// "In <note>, <this> displaces <other>" seen from either side.
#[derive(Serialize)]
struct DisplacementView {
    verb: &'static str,
    other: RefView,
    note: RefView,
}

/// `/loci/` lists every locus by kind; `/loci/<kind>/<slug>/` shows one.
fn write_loci_pages(site: &Site, env: &Environment, out: &Path) -> anyhow::Result<()> {
    let template = env.get_template("loci.html")?;

    for locus in site.ontology.loci.values() {
        let here = Thing::Locus(locus.kind, locus.slug.clone());
        let mut groups = Vec::new();
        for relation in Relation::ALL {
            let items: Vec<RelationItem> = locus
                .mentions
                .iter()
                .filter(|(_, r)| *r == relation)
                .map(|(n, _)| RelationItem {
                    thing: thing_ref(site, &Thing::Note(*n)),
                    old: None,
                })
                .collect();
            if !items.is_empty() {
                groups.push(RelationGroup {
                    label: relation.inverse(),
                    items,
                });
            }
        }
        let displacements: Vec<DisplacementView> = site
            .ontology
            .displacements
            .iter()
            .filter_map(|d| {
                let (verb, other) = if d.new == here {
                    ("displaces", &d.old)
                } else if d.old == here {
                    ("displaced by", &d.new)
                } else {
                    return None;
                };
                Some(DisplacementView {
                    verb,
                    other: thing_ref(site, other),
                    note: thing_ref(site, &Thing::Note(d.note)),
                })
            })
            .collect();

        let dir = out.join("loci").join(locus.kind.plural()).join(&locus.slug);
        fs::create_dir_all(&dir)?;
        let html = template.render(context! {
            locus => context! { name => &locus.name, kind => locus.kind.name(), plural => locus.kind.plural() },
            relations => Serde(&groups),
            displacements => Serde(&displacements),
        })?;
        fs::write(dir.join("index.html"), html)?;
    }

    let sections: Vec<KindSection> = Kind::ALL
        .into_iter()
        .map(|kind| {
            let mut entries: Vec<LocusEntry> = site
                .garden
                .notes
                .iter()
                .enumerate()
                .filter(|(_, n)| n.meta.locus == Some(kind))
                .map(|(i, n)| LocusEntry {
                    title: n.title.clone(),
                    url: note_url(n),
                    count: site.ontology.incoming[i].len(),
                })
                .chain(
                    site.ontology
                        .loci
                        .values()
                        .filter(|l| l.kind == kind)
                        .map(|l| LocusEntry {
                            title: l.name.clone(),
                            url: locus_url(kind, &l.slug),
                            count: l.mentions.len(),
                        }),
                )
                .collect();
            entries.sort_by_key(|e| e.title.to_lowercase());
            KindSection {
                name: kind.name(),
                plural: kind.plural(),
                entries,
            }
        })
        .collect();
    let html = template.render(context! { sections => Serde(&sections) })?;
    fs::create_dir_all(out.join("loci"))?;
    fs::write(out.join("loci").join("index.html"), html)?;
    Ok(())
}

/// Write the whole site to `out`. Each note becomes `out/<slug>/index.html`.
pub fn build(
    site: &Site,
    content: &Path,
    templates: &Path,
    out: &Path,
    dev: bool,
) -> anyhow::Result<()> {
    let (garden, index) = (site.garden, site.index);
    let mut env = load_templates(templates)?;
    // `dev` adds the live-reload script; it's never set for a real build.
    env.add_global("dev", dev);
    let options = markdown::options();

    ensure_safe_to_clear(out, &[content, templates])?;
    if out.exists() {
        fs::remove_dir_all(out).with_context(|| format!("clearing {}", out.display()))?;
    }
    fs::create_dir_all(out)?;

    let mut home = None;
    for (i, note) in garden.notes.iter().enumerate() {
        if note.meta.layout == Layout::Home {
            home = Some(i);
            continue;
        }
        // Fall back to the plain note template until a layout has its own.
        let name = template_name(note.meta.layout);
        let template = env
            .get_template(name)
            .or_else(|_| env.get_template("note.html"))?;

        let backlinks: Vec<LinkView> = index.backlinks[i]
            .iter()
            .map(|b| LinkView {
                url: note_url(&garden.notes[b.source]),
                title: &garden.notes[b.source].title,
                snippet: Some(&b.snippet),
            })
            .collect();
        let outgoing: Vec<LinkView> = index.outgoing[i]
            .iter()
            .map(|&to| LinkView {
                url: note_url(&garden.notes[to]),
                title: &garden.notes[to].title,
                snippet: None,
            })
            .collect();

        let html = template.render(context! {
            page => Serde(PageView::from(note)),
            content => note_html(garden, index, i, &note.body, &options),
            backlinks => Serde(&backlinks),
            outgoing => Serde(&outgoing),
            relations => Serde(&relation_groups(site, i)),
        })?;

        let dir = out.join(&note.slug);
        fs::create_dir_all(&dir)?;
        fs::write(dir.join("index.html"), html)?;
    }

    // Most recently tended first, like a garden's "what's new".
    let mut pages: Vec<PageView> = garden
        .notes
        .iter()
        .filter(|n| n.meta.layout != Layout::Home && !n.meta.unlisted && !n.meta.pinned)
        .map(PageView::from)
        .collect();
    // Pinned notes get their own slot above the list, for newcomers.
    let pinned: Vec<PageView> = garden
        .notes
        .iter()
        .filter(|n| n.meta.pinned && !n.meta.unlisted && n.meta.layout != Layout::Home)
        .map(PageView::from)
        .collect();
    pages.sort_by(|a, b| {
        b.tended
            .cmp(&a.tended)
            .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
    });

    let (page, intro, sections) = match home {
        Some(i) => {
            let note = &garden.notes[i];
            let (intro, sections) = split_sections(&note.body);
            let sections: Vec<SectionView> = sections
                .into_iter()
                .map(|(heading, body)| SectionView {
                    heading,
                    html: note_html(garden, index, i, &body, &options),
                })
                .collect();
            (
                Some(PageView::from(note)),
                note_html(garden, index, i, &intro, &options),
                sections,
            )
        }
        None => (None, String::new(), Vec::new()),
    };

    let html = env.get_template("home.html")?.render(context! {
        page => Serde(&page),
        intro => intro,
        sections => Serde(&sections),
        pinned => Serde(&pinned),
        pages => Serde(&pages),
    })?;
    fs::write(out.join("index.html"), html)?;

    write_tag_pages(garden, &env, out)?;
    write_loci_pages(site, &env, out)?;

    Ok(())
}

/// A tag and the notes that carry it.
#[derive(Serialize)]
struct TagGroup<'a> {
    name: &'a str,
    url: String,
    pages: Vec<PageView<'a>>,
}

/// `/tags/` lists every tag; `/tags/<tag>/` lists its notes, newest first.
fn write_tag_pages(garden: &Garden, env: &Environment, out: &Path) -> anyhow::Result<()> {
    let mut tags: BTreeMap<String, TagGroup> = BTreeMap::new();
    for note in garden.notes.iter().filter(|n| !n.meta.unlisted) {
        for tag in &note.meta.tags {
            tags.entry(slugify(tag))
                .or_insert_with(|| TagGroup {
                    name: tag,
                    url: tag_url(tag),
                    pages: Vec::new(),
                })
                .pages
                .push(PageView::from(note));
        }
    }
    let template = env.get_template("tags.html")?;
    for (slug, group) in &mut tags {
        group.pages.sort_by(|a, b| b.tended.cmp(&a.tended));
        let dir = out.join("tags").join(slug);
        fs::create_dir_all(&dir)?;
        let html = template.render(context! { tag => Serde(&*group) })?;
        fs::write(dir.join("index.html"), html)?;
    }
    let all: Vec<&TagGroup> = tags.values().collect();
    let html = template.render(context! { tags => Serde(&all) })?;
    fs::create_dir_all(out.join("tags"))?;
    fs::write(out.join("tags").join("index.html"), html)?;
    Ok(())
}

#[cfg(test)]
mod outline_tests {
    use super::outline;

    #[test]
    fn counts_words_tasks_and_headings() {
        let o = outline(
            "# Title\n\nOne two three.\n\n## Done\n\n- [x] a\n- [ ] b\n\n### Sub part\n\n## Done\n",
        );
        assert_eq!((o.tasks.done, o.tasks.total), (1, 2));
        let ids: Vec<&str> = o.toc.iter().map(|t| t.id.as_str()).collect();
        // Duplicate headings get the same -1 suffix comrak gives them.
        assert_eq!(ids, ["done", "sub-part", "done-1"]);
        assert!(o.words >= 6);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::{FrontMatter, slugify};
    use crate::links;
    use std::path::PathBuf;

    fn note(stem: &str, body: &str) -> Note {
        Note {
            path: PathBuf::from(format!("{stem}.md")),
            slug: slugify(stem),
            title: stem.to_string(),
            meta: FrontMatter::default(),
            body: body.to_string(),
            body_line: 1,
        }
    }

    fn render_first(body: &str) -> String {
        let mut garden = Garden {
            notes: vec![note("Here", body), note("Other Note", "# Big Idea\n")],
            private_names: ["Secret Diary".to_string()].into(),
            ..Garden::default()
        };
        let index = links::build(&mut garden, false);
        note_html(
            &garden,
            &index,
            0,
            &garden.notes[0].body,
            &markdown::options(),
        )
    }

    #[test]
    fn resolved_links_point_at_slugs_and_headings() {
        let html = render_first("[[Other Note]] and [[other note#Big Idea|the idea]] and [[#Top]]");
        assert!(html.contains(r#"<a class="wikilink" href="/other-note/">Other Note</a>"#));
        assert!(render_first("[[other-note]]").contains(">Other Note</a>"));
        assert!(html.contains(r#"<a class="wikilink" href="/other-note/#big-idea">the idea</a>"#));
        assert!(html.contains(r##"<a class="wikilink" href="#top">"##));
    }

    #[test]
    fn tables_and_code_are_keyboard_scrollable() {
        let html =
            render_first("| a | b |\n|---|---|\n| 1 | 2 |\n\n```rust\nlet x = 1 < 2;\n```\n");
        assert!(html.contains(
            r#"<div class="table-scroll" role="region" aria-label="Table" tabindex="0"><table>"#
        ));
        assert!(
            html.contains(r#"<pre tabindex="0"><code class="language-rust">let x = 1 &lt; 2;"#)
        );
    }

    #[test]
    fn galleries_figures_and_external_links() {
        let html = render_first(
            "![a](/a.png)\n![b](/b.png)\n\n![Bowl](/c.png)\n*A caption*\n\n[x](https://example.com) and [y](javascript:alert(1)) and *plain*",
        );
        assert!(html.contains("<div class=\"gallery\">"));
        assert!(html.contains("<figure>"));
        assert!(html.contains("<figcaption>A caption</figcaption>"));
        assert!(html.contains(r#"<a href="https://example.com" rel="noopener noreferrer">x</a>"#));
        assert!(!html.contains("javascript:"));
        assert!(html.contains("<em>plain</em>"));
        assert!(html.contains(r#"loading="lazy""#));
    }

    #[test]
    fn placeholder_alt_is_dropped_and_missing_images_are_marked() {
        let html = render_first("![alt text](/a.png)\n\n![Ghost](images/nope.webp)");
        assert!(html.contains(r#"alt="""#));
        assert!(
            html.contains(
                r#"<span class="image-missing">Image not found: images/nope.webp</span>"#
            )
        );
    }

    #[test]
    fn missing_links_are_marked() {
        let html = render_first("[[Ghost]]");
        assert!(html.contains(r#"<span class="link-missing""#));
        assert!(html.contains(">Ghost</span>"));
    }

    #[test]
    fn private_links_are_plain_text_with_no_trace() {
        let html = render_first("Read [[Secret Diary|my notes]] later.");
        assert_eq!(html.trim(), "<p>Read my notes later.</p>");
    }
}

#[cfg(test)]
mod section_tests {
    use super::split_sections;

    #[test]
    fn splits_on_h2_but_not_inside_code() {
        let (intro, sections) =
            split_sections("Hi\n## One\na\n```\n## not a heading\n```\n## Two\nb\n");
        assert_eq!(intro, "Hi\n");
        assert_eq!(sections.len(), 2);
        assert_eq!(sections[0].0, "One");
        assert!(sections[0].1.contains("## not a heading"));
        assert_eq!(sections[1], ("Two".to_string(), "b\n".to_string()));
    }
}
