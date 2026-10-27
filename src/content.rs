//! Load pass: find notes, keep only published ones, parse front matter into typed structs.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use jiff::civil::Date;
use serde::Deserialize;
use walkdir::WalkDir;

/// Front matter keys the engine understands. Used to catch typos like `publsh`.
const KNOWN_FIELDS: &[&str] = &[
    "title",
    "publish",
    "draft",
    "date",
    "layout",
    "stage",
    "area",
    "tags",
    "aliases",
    "planted",
    "tended",
    "summary",
    "slug",
    "cover",
    "role",
    "unlisted",
    "pinned",
    "locus",
    "era",
    "region",
    "read-alongside",
    "part-of",
    "traversed",
    "attends-to",
    "raises",
    "displaces",
];

#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Layout {
    #[default]
    Note,
    Essay,
    Project,
    Log,
    Index,
    /// The site's front page, rendered at `/`. At most one.
    Home,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Stage {
    Seedling,
    Budding,
    Evergreen,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Area {
    Professional,
    Personal,
}

// Some fields are part of the schema now but only used by later phases (links, layouts).
#[allow(dead_code)]
#[derive(Debug, Default, Deserialize)]
pub struct FrontMatter {
    pub title: Option<String>,
    #[serde(default)]
    pub publish: bool,
    /// Blog-style alternative to `publish`: `draft: false` publishes, `draft: true` never does.
    pub draft: Option<bool>,
    /// Blog-style date. Fills in `planted` and `tended` when they're missing.
    /// Kept loose so a value like `date: year` warns instead of failing the build.
    #[serde(default)]
    pub date: serde_norway::Value,
    #[serde(default)]
    pub layout: Layout,
    /// No badge is shown unless a stage is set.
    pub stage: Option<Stage>,
    /// Published, but left out of the home index and tag pages (e.g. a colophon).
    #[serde(default)]
    pub unlisted: bool,
    /// Shown in its own slot above the home page list, for newcomers.
    #[serde(default)]
    pub pinned: bool,

    // Loci: what this note is about, and how it relates to other things. See `loci.rs`.
    pub locus: Option<crate::loci::Kind>,
    /// For works: when they're set or were made, in words ("c. 1820s–1830s").
    pub era: Option<String>,
    /// For places: a region, never a point on a map.
    pub region: Option<String>,
    #[serde(
        default,
        rename = "read-alongside",
        deserialize_with = "crate::loci::one_or_many"
    )]
    pub read_alongside: Vec<String>,
    #[serde(
        default,
        rename = "part-of",
        deserialize_with = "crate::loci::one_or_many"
    )]
    pub part_of: Vec<String>,
    #[serde(default, deserialize_with = "crate::loci::one_or_many")]
    pub traversed: Vec<String>,
    #[serde(
        default,
        rename = "attends-to",
        deserialize_with = "crate::loci::one_or_many"
    )]
    pub attends_to: Vec<String>,
    #[serde(default, deserialize_with = "crate::loci::one_or_many")]
    pub raises: Vec<String>,
    /// Each entry reads `new thing > old thing`.
    #[serde(default, deserialize_with = "crate::loci::one_or_many")]
    pub displaces: Vec<String>,
    pub area: Option<Area>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub aliases: Vec<String>,
    pub planted: Option<Date>,
    pub tended: Option<Date>,
    pub summary: Option<String>,
    pub slug: Option<String>,
    pub cover: Option<String>,
    pub role: Option<String>,
    /// Anything else. Kept so unknown fields warn instead of failing.
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_norway::Value>,
}

#[derive(Debug)]
pub struct Note {
    /// Path relative to the content root. Never rendered into output.
    pub path: PathBuf,
    pub slug: String,
    pub title: String,
    pub meta: FrontMatter,
    pub body: String,
    /// Line in the source file where `body` starts, for error messages.
    pub body_line: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Warning,
    Error,
}

#[derive(Debug)]
pub struct Diagnostic {
    pub level: Level,
    pub path: PathBuf,
    pub line: Option<usize>,
    pub message: String,
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let level = match self.level {
            Level::Warning => "warning",
            Level::Error => "error",
        };
        write!(f, "{level}: {}", self.path.display())?;
        if let Some(line) = self.line {
            write!(f, ":{line}")?;
        }
        write!(f, ": {}", self.message)
    }
}

#[derive(Debug, Default)]
pub struct Garden {
    pub notes: Vec<Note>,
    pub diagnostics: Vec<Diagnostic>,
    /// The content folder, for finding images next to notes.
    pub root: PathBuf,
    /// File names and aliases of unpublished notes. Used only to render links to them
    /// as plain text instead of "broken". Never written to output.
    pub private_names: BTreeSet<String>,
}

impl Garden {
    pub fn has_errors(&self) -> bool {
        self.diagnostics.iter().any(|d| d.level == Level::Error)
    }
}

/// Walk the content root and load every published note.
///
/// Hidden folders (`.obsidian`, `.trash`) are skipped, and symlinks are not followed,
/// so nothing outside the root can be pulled in.
pub fn load(root: &Path) -> anyhow::Result<Garden> {
    let mut garden = Garden {
        root: root.to_path_buf(),
        ..Garden::default()
    };

    let entries = WalkDir::new(root)
        .follow_links(false)
        .sort_by_file_name()
        .into_iter()
        .filter_entry(|e| e.depth() == 0 || !e.file_name().to_string_lossy().starts_with('.'));

    for entry in entries {
        let entry = entry?;
        let is_markdown = entry.path().extension().is_some_and(|ext| ext == "md");
        if !entry.file_type().is_file() || !is_markdown {
            continue;
        }
        let rel = entry.path().strip_prefix(root)?.to_path_buf();
        let source = fs::read_to_string(entry.path())?;
        load_note(rel, &source, &mut garden);
    }

    check_duplicate_slugs(&mut garden);
    check_single_home(&mut garden);
    Ok(garden)
}

/// Minimal view used to decide whether a note is published at all,
/// before we hold it to the full schema.
#[derive(Deserialize)]
struct PublishOnly {
    #[serde(default)]
    publish: bool,
    draft: Option<bool>,
    /// Obsidian allows a single string or a list, so stay loose here.
    #[serde(default)]
    aliases: serde_norway::Value,
}

fn load_note(path: PathBuf, source: &str, garden: &mut Garden) {
    let stem = path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();

    let Some((yaml, body)) = split_front_matter(source) else {
        // No front matter means no `publish: true`, so it's private. Skip quietly.
        garden.private_names.insert(stem);
        return;
    };

    match serde_norway::from_str::<PublishOnly>(yaml) {
        // Opt-in only: `publish: true` or an explicit `draft: false`. `draft: true` always wins.
        Ok(p) if p.draft != Some(true) && (p.publish || p.draft == Some(false)) => {}
        Ok(p) => {
            garden.private_names.insert(stem);
            let aliases = match p.aliases {
                serde_norway::Value::String(a) => vec![a],
                serde_norway::Value::Sequence(list) => list
                    .into_iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect(),
                _ => vec![],
            };
            garden.private_names.extend(aliases);
            return;
        }
        Err(_) => {
            garden.private_names.insert(stem);
            garden.diagnostics.push(Diagnostic {
                level: Level::Warning,
                path,
                line: None,
                message: "skipped: front matter is not valid YAML".into(),
            });
            return;
        }
    }

    let mut meta: FrontMatter = match serde_norway::from_str(yaml) {
        Ok(meta) => meta,
        Err(err) => {
            garden.diagnostics.push(Diagnostic {
                level: Level::Error,
                // +1 for the opening `---` line.
                line: err.location().map(|l| l.line() + 1),
                path,
                message: format!("front matter: {err}"),
            });
            return;
        }
    };

    for key in meta.extra.keys() {
        if crate::loci::LOCATION_FIELDS.contains(&key.to_lowercase().as_str()) {
            garden.diagnostics.push(Diagnostic {
                level: Level::Error,
                path: path.clone(),
                line: None,
                message: format!(
                    "`{key}` isn't allowed: places stay at the level of a town or region (use `region:`)"
                ),
            });
            continue;
        }
        let typo_of = KNOWN_FIELDS
            .iter()
            .find(|known| strsim::levenshtein(key, known) <= 2);
        let (level, message) = match typo_of {
            Some(known) => (
                Level::Error,
                format!("unknown field `{key}` (did you mean `{known}`?)"),
            ),
            None => (Level::Warning, format!("unknown field `{key}` is ignored")),
        };
        garden.diagnostics.push(Diagnostic {
            level,
            path: path.clone(),
            line: None,
            message,
        });
    }

    match &meta.date {
        serde_norway::Value::Null => {}
        value => match value.as_str().and_then(|d| d.parse::<Date>().ok()) {
            Some(date) => {
                meta.planted = meta.planted.or(Some(date));
                meta.tended = meta.tended.or(Some(date));
            }
            None => garden.diagnostics.push(Diagnostic {
                level: Level::Warning,
                path: path.clone(),
                // +2: the opening `---` line, and lines are 1-based.
                line: yaml
                    .lines()
                    .position(|l| l.starts_with("date:"))
                    .map(|i| i + 2),
                message: format!(
                    "`date: {}` isn't a YYYY-MM-DD date, so it's ignored",
                    value.as_str().unwrap_or("…")
                ),
            }),
        },
    }

    let title = match &meta.title {
        Some(t) => t.clone(),
        None => {
            garden.diagnostics.push(Diagnostic {
                level: Level::Warning,
                path: path.clone(),
                line: None,
                message: "missing `title`, using the filename".into(),
            });
            stem.clone()
        }
    };
    let slug = slugify(meta.slug.as_deref().unwrap_or(&stem));
    // `body` is the tail of `source`, so everything before it is front matter.
    let body_line = source[..source.len() - body.len()].lines().count() + 1;

    garden.notes.push(Note {
        path,
        slug,
        title,
        meta,
        body: body.to_string(),
        body_line,
    });
}

fn check_single_home(garden: &mut Garden) {
    let homes: Vec<&Note> = garden
        .notes
        .iter()
        .filter(|n| n.meta.layout == Layout::Home)
        .collect();
    if let [first, rest @ ..] = homes.as_slice() {
        let extra: Vec<Diagnostic> = rest
            .iter()
            .map(|n| Diagnostic {
                level: Level::Error,
                path: n.path.clone(),
                line: None,
                message: format!(
                    "only one note can use `layout: home`; {} already does",
                    first.path.display()
                ),
            })
            .collect();
        garden.diagnostics.extend(extra);
    }
}

/// Folders the site itself uses. A note with one of these slugs would overwrite them.
const RESERVED_SLUGS: &[&str] = &["tags", "fonts", "media", "loci", "__kwaj"];

fn check_duplicate_slugs(garden: &mut Garden) {
    let reserved: Vec<Diagnostic> = garden
        .notes
        .iter()
        .filter(|n| RESERVED_SLUGS.contains(&n.slug.as_str()))
        .map(|n| Diagnostic {
            level: Level::Error,
            path: n.path.clone(),
            line: None,
            message: format!(
                "slug `{}` is reserved by the site; rename the file or set `slug:`",
                n.slug
            ),
        })
        .collect();
    garden.diagnostics.extend(reserved);

    let mut seen: BTreeMap<&str, &Path> = BTreeMap::new();
    let mut found = Vec::new();
    for note in &garden.notes {
        if let Some(first) = seen.insert(&note.slug, &note.path) {
            found.push(Diagnostic {
                level: Level::Error,
                path: note.path.clone(),
                line: None,
                message: format!("slug `{}` is also used by {}", note.slug, first.display()),
            });
        }
    }
    garden.diagnostics.extend(found);
}

/// Split `---\nyaml\n---\nbody` into `(yaml, body)`. Returns `None` if there's no front matter.
pub fn split_front_matter(source: &str) -> Option<(&str, &str)> {
    let source = source.strip_prefix('\u{feff}').unwrap_or(source);
    let rest = source
        .strip_prefix("---\n")
        .or_else(|| source.strip_prefix("---\r\n"))?;

    let mut offset = 0;
    for line in rest.split_inclusive('\n') {
        if line.trim_end() == "---" {
            return Some((&rest[..offset], &rest[offset + line.len()..]));
        }
        offset += line.len();
    }
    None
}

/// "Designing for Slowness" -> "designing-for-slowness"
pub fn slugify(name: &str) -> String {
    let mut slug = String::with_capacity(name.len());
    for c in name.trim().chars() {
        if c.is_alphanumeric() {
            slug.extend(c.to_lowercase());
        } else if !slug.ends_with('-') && !slug.is_empty() {
            slug.push('-');
        }
    }
    slug.trim_end_matches('-').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn load_one(source: &str) -> Garden {
        let mut garden = Garden::default();
        load_note(PathBuf::from("note.md"), source, &mut garden);
        garden
    }

    #[test]
    fn slugify_handles_spaces_and_punctuation() {
        assert_eq!(slugify("Designing for Slowness"), "designing-for-slowness");
        assert_eq!(slugify("  What's next?  "), "what-s-next");
        assert_eq!(slugify("吴天霆 notes"), "吴天霆-notes");
    }

    #[test]
    fn splits_front_matter() {
        let (yaml, body) = split_front_matter("---\ntitle: A\n---\nHello\n").unwrap();
        assert_eq!(yaml, "title: A\n");
        assert_eq!(body, "Hello\n");
        assert!(split_front_matter("No front matter").is_none());
    }

    #[test]
    fn unpublished_notes_are_skipped_silently() {
        assert!(
            load_one("---\ntitle: Private\n---\nsecret")
                .notes
                .is_empty()
        );
        assert!(
            load_one("---\ntitle: Private\npublish: false\n---\n")
                .notes
                .is_empty()
        );
        assert!(load_one("no front matter at all").notes.is_empty());
        assert!(
            load_one("---\ntitle: Private\n---\n")
                .diagnostics
                .is_empty()
        );
        let g = load_one("---\naliases: [Hidden]\n---\n");
        assert!(g.private_names.contains("note") && g.private_names.contains("Hidden"));
    }

    #[test]
    fn published_note_is_parsed_into_types() {
        let g = load_one(
            "---\ntitle: Hi\npublish: true\nlayout: essay\nstage: evergreen\nplanted: 2026-10-25\n---\nBody",
        );
        let note = &g.notes[0];
        assert_eq!(note.meta.layout, Layout::Essay);
        assert_eq!(note.meta.stage, Some(Stage::Evergreen));
        assert_eq!(note.meta.planted.unwrap().to_string(), "2026-10-25");
        assert_eq!(note.body, "Body");
        assert_eq!(note.body_line, 8);
    }

    #[test]
    fn draft_false_publishes_but_missing_or_true_does_not() {
        assert_eq!(
            load_one("---\ntitle: A\ndraft: false\n---\n").notes.len(),
            1
        );
        assert!(
            load_one("---\ntitle: A\ndraft: true\n---\n")
                .notes
                .is_empty()
        );
        assert!(load_one("---\ntitle: A\n---\n").notes.is_empty());
        // `draft: true` wins over `publish: true`.
        assert!(
            load_one("---\npublish: true\ndraft: true\n---\n")
                .notes
                .is_empty()
        );
    }

    #[test]
    fn date_fills_planted_and_tended_and_bad_dates_warn() {
        let g = load_one("---\ntitle: A\ndraft: false\ndate: 2024-07-19\n---\n");
        assert_eq!(g.notes[0].meta.planted.unwrap().to_string(), "2024-07-19");
        assert_eq!(g.notes[0].meta.tended.unwrap().to_string(), "2024-07-19");
        assert!(g.diagnostics.is_empty());

        let g = load_one("---\ntitle: A\ndraft: false\ndate: year\n---\n");
        assert_eq!(g.notes.len(), 1);
        assert!(!g.has_errors());
        assert_eq!(g.diagnostics[0].line, Some(4));
    }

    #[test]
    fn coordinates_are_refused() {
        let g = load_one("---\ntitle: A\ndraft: false\nlatitude: 56.56\n---\n");
        assert!(g.has_errors());
        assert!(g.diagnostics[0].message.contains("town or region"));
    }

    #[test]
    fn loci_fields_accept_one_or_many() {
        let g = load_one(
            "---\ntitle: A\ndraft: false\nlocus: work\nread-alongside: One\ntraversed: [X, Y]\n---\n",
        );
        assert!(g.diagnostics.is_empty(), "{:?}", g.diagnostics);
        let meta = &g.notes[0].meta;
        assert_eq!(meta.locus, Some(crate::loci::Kind::Work));
        assert_eq!(meta.read_alongside, vec!["One"]);
        assert_eq!(meta.traversed, vec!["X", "Y"]);
    }

    #[test]
    fn stage_is_optional() {
        let g = load_one("---\ntitle: A\ndraft: false\n---\n");
        assert_eq!(g.notes[0].meta.stage, None);
    }

    #[test]
    fn typo_of_known_field_is_an_error() {
        let g = load_one("---\ntitle: Hi\npublish: true\nsumary: x\n---\n");
        assert!(g.has_errors());
        assert!(g.diagnostics[0].message.contains("did you mean `summary`"));
    }

    #[test]
    fn unrelated_unknown_field_is_a_warning() {
        let g = load_one("---\ntitle: Hi\npublish: true\nmood: sunny\n---\n");
        assert!(!g.has_errors());
        assert_eq!(g.diagnostics.len(), 1);
    }

    #[test]
    fn reserved_slugs_are_errors() {
        let mut g = Garden::default();
        load_note(
            PathBuf::from("Tags.md"),
            "---\npublish: true\n---\n",
            &mut g,
        );
        check_duplicate_slugs(&mut g);
        assert!(g.has_errors());
    }

    #[test]
    fn bad_enum_value_reports_line() {
        let g = load_one("---\ntitle: Hi\npublish: true\nstage: sprout\n---\n");
        assert!(g.has_errors());
        assert_eq!(g.diagnostics[0].line, Some(4));
    }
}
