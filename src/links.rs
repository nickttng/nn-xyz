//! Index pass: resolve every wikilink and build the backlink graph.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Component, Path, PathBuf};

use comrak::nodes::{AstNode, NodeValue};
use comrak::{Anchorizer, Arena};

use crate::content::{Diagnostic, Garden, Level, slugify};
use crate::markdown;

/// Where a wikilink points.
#[derive(Debug, PartialEq, Eq)]
pub enum Target {
    /// A published note, by index into `Garden::notes`.
    Note {
        index: usize,
        anchor: Option<String>,
    },
    /// A note that exists but isn't published. Rendered as plain text.
    Private,
    /// Nothing by that name.
    Missing,
}

#[derive(Debug)]
pub struct Backlink {
    /// Index of the note that contains the link.
    pub source: usize,
    /// The paragraph around the link, as plain text.
    pub snippet: String,
}

#[derive(Debug, Default)]
pub struct Index {
    /// Normalised name or alias -> note index.
    by_name: BTreeMap<String, usize>,
    /// Normalised names of unpublished notes. Only used to decide "plain text, not broken".
    private: HashSet<String>,
    /// Per note: who links here. Indexed like `Garden::notes`.
    pub backlinks: Vec<Vec<Backlink>>,
    /// Per note: which notes it links to, in order of first appearance.
    pub outgoing: Vec<Vec<usize>>,
    /// (note index, image URL as written) -> public URL, or `None` if it can't be shown.
    pub images: BTreeMap<(usize, String), Option<String>>,
    /// Image files to publish: (file on disk, path under `/media/`).
    pub media: BTreeSet<(PathBuf, String)>,
}

/// Image types the site will copy and serve.
const IMAGE_TYPES: &[&str] = &["jpg", "jpeg", "png", "webp", "avif", "gif", "svg"];

/// Find the file behind `![…](url)` in a note.
/// `Ok(None)`: a root path like `/fonts/x.svg`, served from `static/` as written.
/// `Err`: why it can't be shown (external, outside the content folder, missing…).
fn resolve_image(root: &Path, note: &Path, url: &str) -> Result<Option<(PathBuf, String)>, String> {
    if url.starts_with("http://") || url.starts_with("https://") || url.starts_with("//") {
        return Err("blocked: external images break the site's security policy; save them into content/ and link relatively".into());
    }
    if url.starts_with('/') {
        return Ok(None);
    }
    // Resolve against the note's folder, never letting `..` climb out of the content root.
    let mut rel = PathBuf::new();
    for part in note
        .parent()
        .unwrap_or(Path::new(""))
        .join(url)
        .components()
    {
        match part {
            Component::Normal(p) => rel.push(p),
            Component::CurDir => {}
            Component::ParentDir if rel.pop() => {}
            _ => return Err("blocked: points outside the content folder".into()),
        }
    }
    let ext = rel
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    if !IMAGE_TYPES.contains(&ext.as_str()) {
        return Err("not an image type the site serves".into());
    }
    let file = root.join(&rel);
    match file.symlink_metadata() {
        Ok(meta) if meta.is_file() => {}
        Ok(_) => return Err("not a regular file".into()),
        Err(_) => return Err("not found".into()),
    }
    let public = rel
        .components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/");
    Ok(Some((file, public)))
}

const SNIPPET_CHARS: usize = 180;

impl Index {
    /// Resolve the URL part of `[[url]]` as written in note `from`.
    pub fn resolve(&self, url: &str, from: usize) -> Target {
        let (name, heading) = match url.split_once('#') {
            Some((name, heading)) => (name, Some(heading)),
            None => (url, None),
        };
        let anchor = heading.map(|h| Anchorizer::new().anchorize(h.trim()));

        // [[#Heading]] links within the same note.
        if name.trim().is_empty() {
            return Target::Note {
                index: from,
                anchor,
            };
        }

        let key = normalise(name);
        if let Some(&index) = self.by_name.get(&key) {
            Target::Note { index, anchor }
        } else if self.private.contains(&key) {
            Target::Private
        } else {
            Target::Missing
        }
    }
}

/// `projects/Field Notes.md` and `field-notes` both become `field-notes`.
fn normalise(name: &str) -> String {
    let name = name.rsplit('/').next().unwrap_or(name);
    let name = name.strip_suffix(".md").unwrap_or(name);
    slugify(name)
}

/// Build the link index. Broken links are warnings, or errors when `strict`.
pub fn build(garden: &mut Garden, strict: bool) -> Index {
    let mut index = Index {
        private: garden.private_names.iter().map(|n| normalise(n)).collect(),
        backlinks: garden.notes.iter().map(|_| Vec::new()).collect(),
        outgoing: garden.notes.iter().map(|_| Vec::new()).collect(),
        ..Index::default()
    };

    for (i, note) in garden.notes.iter().enumerate() {
        // The slug is already unique (checked at load); aliases might not be.
        index.by_name.insert(note.slug.clone(), i);
        let stem = note.path.file_stem().unwrap_or_default().to_string_lossy();
        index.by_name.insert(normalise(&stem), i);
    }
    for (i, note) in garden.notes.iter().enumerate() {
        for alias in &note.meta.aliases {
            let key = normalise(alias);
            match index.by_name.get(&key) {
                Some(&other) if other != i => garden.diagnostics.push(Diagnostic {
                    level: Level::Error,
                    path: note.path.clone(),
                    line: None,
                    message: format!(
                        "alias `{alias}` clashes with {}",
                        garden.notes[other].path.display()
                    ),
                }),
                Some(_) => {}
                None => {
                    index.by_name.insert(key, i);
                }
            }
        }
    }

    let options = markdown::options();
    let mut found = Vec::new();
    for (from, note) in garden.notes.iter().enumerate() {
        // Image warnings are grouped per note: a photo gallery shouldn't print 30 lines.
        let mut image_issues: BTreeMap<&str, Vec<(usize, String)>> = BTreeMap::new();
        let mut image_problems: BTreeMap<String, Vec<(usize, String)>> = BTreeMap::new();
        let arena = Arena::new();
        let root = markdown::parse(&arena, &note.body, &options);

        for node in root.descendants() {
            let image_url = match &node.data().value {
                NodeValue::Image(link) => Some(link.url.clone()),
                _ => None,
            };
            if let Some(url) = image_url {
                let line = note.body_line + node.data().sourcepos.start.line - 1;
                let alt = node.collect_text();
                if alt.trim().is_empty() {
                    image_issues
                        .entry(NO_ALT)
                        .or_default()
                        .push((line, url.clone()));
                } else if markdown::is_placeholder_alt(&alt) {
                    image_issues
                        .entry(PLACEHOLDER_ALT)
                        .or_default()
                        .push((line, url.clone()));
                }
                let public = match resolve_image(&garden.root, &note.path, &url) {
                    Ok(Some((file, public))) => {
                        index.media.insert((file, public.clone()));
                        Some(format!("/media/{public}"))
                    }
                    Ok(None) => Some(url.clone()),
                    Err(problem) => {
                        image_problems
                            .entry(problem)
                            .or_default()
                            .push((line, url.clone()));
                        None
                    }
                };
                index.images.insert((from, url), public);
                continue;
            }
            let (url, embed) = match &node.data().value {
                NodeValue::WikiLink(link) => (link.url.clone(), link.embed),
                _ => continue,
            };
            let line = note.body_line + node.data().sourcepos.start.line - 1;

            match index.resolve(&url, from) {
                Target::Note { index: to, .. } => {
                    if to == from {
                        continue;
                    }
                    if !index.outgoing[from].contains(&to) {
                        index.outgoing[from].push(to);
                    }
                    if !index.backlinks[to].iter().any(|b| b.source == from) {
                        index.backlinks[to].push(Backlink {
                            source: from,
                            snippet: snippet(node),
                        });
                    }
                }
                Target::Private => found.push(Diagnostic {
                    level: Level::Warning,
                    path: note.path.clone(),
                    line: Some(line),
                    message: format!("[[{url}]] is unpublished; its label is shown as plain text"),
                }),
                Target::Missing => found.push(Diagnostic {
                    level: if strict { Level::Error } else { Level::Warning },
                    path: note.path.clone(),
                    line: Some(line),
                    message: if embed && !url.ends_with(".md") && url.contains('.') {
                        format!("![[{url}]]: file embeds aren't supported yet")
                    } else {
                        format!("broken link [[{url}]]")
                    },
                }),
            }
        }
        let grouped = image_issues
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .chain(image_problems);
        for (problem, items) in grouped {
            found.push(image_diagnostic(&note.path, &problem, &items));
        }
    }
    garden.diagnostics.extend(found);

    for links in &mut index.backlinks {
        links.sort_by_key(|b| garden.notes[b.source].title.to_lowercase());
    }
    index
}

const NO_ALT: &str = "have no alt text; describe each as ![description](...)";
const PLACEHOLDER_ALT: &str = "have placeholder alt text (left empty so screen readers skip it); describe each as ![description](...)";

/// One warning for every image in a note that shares a problem.
fn image_diagnostic(path: &Path, problem: &str, items: &[(usize, String)]) -> Diagnostic {
    let shown: Vec<&str> = items.iter().take(3).map(|(_, url)| url.as_str()).collect();
    let more = match items.len().saturating_sub(3) {
        0 => String::new(),
        n => format!(" and {n} more"),
    };
    let (count, problem) = match items.len() {
        1 => ("1 image".to_string(), problem.replacen("have ", "has ", 1)),
        n => (format!("{n} images"), problem.to_string()),
    };
    Diagnostic {
        level: Level::Warning,
        path: path.to_path_buf(),
        line: items.first().map(|(line, _)| *line),
        message: format!("{count} {problem}: {}{more}", shown.join(", ")),
    }
}

/// Plain text of the block (paragraph, list item, heading...) containing `node`.
fn snippet<'a>(node: &'a AstNode<'a>) -> String {
    let block = node
        .ancestors()
        .find(|n| {
            matches!(
                n.data().value,
                NodeValue::Paragraph
                    | NodeValue::Heading(_)
                    | NodeValue::TableCell
                    | NodeValue::Item(_)
            )
        })
        .unwrap_or(node);

    let mut text = String::new();
    for n in block.descendants() {
        match &n.data().value {
            NodeValue::Text(t) => text.push_str(t),
            NodeValue::Code(c) => text.push_str(&c.literal),
            NodeValue::SoftBreak | NodeValue::LineBreak => text.push(' '),
            _ => {}
        }
    }
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");

    if text.chars().count() <= SNIPPET_CHARS {
        text
    } else {
        let cut: String = text.chars().take(SNIPPET_CHARS).collect();
        format!("{}…", cut.trim_end())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::{FrontMatter, Note};
    use std::path::PathBuf;

    fn note(file: &str, body: &str, aliases: &[&str]) -> Note {
        let stem = file.trim_end_matches(".md");
        Note {
            path: PathBuf::from(file),
            slug: slugify(stem),
            title: stem.to_string(),
            meta: FrontMatter {
                aliases: aliases.iter().map(|a| a.to_string()).collect(),
                ..FrontMatter::default()
            },
            body: body.to_string(),
            body_line: 5,
        }
    }

    fn garden(notes: Vec<Note>, private: &[&str]) -> Garden {
        Garden {
            notes,
            private_names: private.iter().map(|p| p.to_string()).collect(),
            ..Garden::default()
        }
    }

    #[test]
    fn resolves_by_name_ignoring_case_spaces_and_folders() {
        let mut g = garden(vec![note("Field Notes.md", "", &[])], &[]);
        let index = build(&mut g, false);
        for url in [
            "Field Notes",
            "field notes",
            "field-notes",
            "projects/Field Notes.md",
        ] {
            assert_eq!(
                index.resolve(url, 0),
                Target::Note {
                    index: 0,
                    anchor: None
                },
                "{url}"
            );
        }
    }

    #[test]
    fn resolves_aliases_and_headings() {
        let mut g = garden(vec![note("Definition of Done.md", "", &["DoD"])], &[]);
        let index = build(&mut g, false);
        assert_eq!(
            index.resolve("dod#A Template", 0),
            Target::Note {
                index: 0,
                anchor: Some("a-template".into())
            }
        );
    }

    #[test]
    fn private_and_missing_are_distinguished() {
        let mut g = garden(vec![note("a.md", "", &[])], &["Secret Diary"]);
        let index = build(&mut g, false);
        assert_eq!(index.resolve("secret diary", 0), Target::Private);
        assert_eq!(index.resolve("nope", 0), Target::Missing);
    }

    #[test]
    fn backlinks_carry_snippets_and_skip_self_links() {
        let mut g = garden(
            vec![
                note(
                    "a.md",
                    "Intro.\n\nSee [[b]] for more, and [[b]] again.\n\nSelf [[a]].",
                    &[],
                ),
                note("b.md", "Nothing here.", &[]),
            ],
            &[],
        );
        let index = build(&mut g, false);
        assert_eq!(index.outgoing[0], vec![1]);
        assert!(index.backlinks[0].is_empty());
        assert_eq!(index.backlinks[1].len(), 1);
        assert_eq!(
            index.backlinks[1][0].snippet,
            "See b for more, and b again."
        );
    }

    #[test]
    fn broken_links_report_file_line_and_respect_strict() {
        let mut g = garden(vec![note("a.md", "one\n\n[[ghost]]", &[])], &[]);
        build(&mut g, false);
        let d = &g.diagnostics[0];
        assert_eq!((d.level, d.line), (Level::Warning, Some(7)));

        let mut g = garden(vec![note("a.md", "[[ghost]]", &[])], &[]);
        build(&mut g, true);
        assert!(g.has_errors());
    }

    #[test]
    fn images_without_alt_text_warn() {
        let body = "![](/x.png) and ![A map](/y.png) and ![alt text](/z.png)";
        let mut g = garden(vec![note("a.md", body, &[])], &[]);
        build(&mut g, false);
        assert_eq!(g.diagnostics.len(), 2);
        assert!(g.diagnostics.iter().all(|d| d.message.contains("alt text")));
    }

    #[test]
    fn images_resolve_inside_the_content_root_only() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("trips/images")).unwrap();
        std::fs::write(dir.path().join("trips/images/a.webp"), b"x").unwrap();
        let root = dir.path();
        let note = Path::new("trips/note.md");

        let (_, public) = resolve_image(root, note, "images/a.webp").unwrap().unwrap();
        assert_eq!(public, "trips/images/a.webp");
        assert!(resolve_image(root, note, "../../etc/passwd.png").is_err());
        assert!(resolve_image(root, note, "images/missing.webp").is_err());
        assert!(resolve_image(root, note, "https://example.com/a.png").is_err());
        assert!(resolve_image(root, note, "images/a.txt").is_err());
        assert_eq!(resolve_image(root, note, "/fonts/x.svg").unwrap(), None);
    }

    #[test]
    fn clashing_alias_is_an_error() {
        let mut g = garden(vec![note("a.md", "", &[]), note("b.md", "", &["a"])], &[]);
        build(&mut g, false);
        assert!(g.has_errors());
    }

    #[test]
    fn long_snippets_are_trimmed() {
        let body = format!("{} [[b]]", "word ".repeat(100));
        let mut g = garden(vec![note("a.md", &body, &[]), note("b.md", "", &[])], &[]);
        let index = build(&mut g, false);
        let s = &index.backlinks[1][0].snippet;
        assert!(s.ends_with('…') && s.chars().count() <= SNIPPET_CHARS + 1);
    }
}
