//! Assets pass: bundle and minify CSS, copy static files.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use lightningcss::bundler::{Bundler, FileProvider};
use lightningcss::stylesheet::{MinifyOptions, ParserOptions, PrinterOptions};
use lightningcss::targets::{Browsers, Targets};
use walkdir::WalkDir;

/// Oldest browsers we write CSS for. Lightning CSS rewrites newer syntax
/// (nesting, `light-dark()`, …) so these still get a working page.
fn targets() -> Targets {
    let version = |major: u32| Some(major << 16);
    Targets::from(Browsers {
        chrome: version(111),
        edge: version(111),
        firefox: version(113),
        safari: version(16),
        ios_saf: version(16),
        ..Browsers::default()
    })
}

/// Bundle `styles/main.css` and everything it imports into `out/style.css`.
pub fn build_css(styles: &Path, out: &Path) -> anyhow::Result<()> {
    let entry = styles.join("main.css");
    let provider = FileProvider::new();
    let mut bundler = Bundler::new(&provider, None, ParserOptions::default());
    let mut sheet = bundler
        .bundle(&entry)
        .map_err(|e| anyhow::anyhow!("{}: {e}", entry.display()))?;

    let targets = targets();
    sheet.minify(MinifyOptions {
        targets,
        ..MinifyOptions::default()
    })?;
    let css = sheet.to_css(PrinterOptions {
        minify: true,
        targets,
        ..PrinterOptions::default()
    })?;
    fs::write(out.join("style.css"), css.code)?;
    Ok(())
}

/// Copy the images notes link to into `out/media/`, keeping their folder structure.
pub fn copy_media(media: &BTreeSet<(PathBuf, String)>, out: &Path) -> anyhow::Result<()> {
    for (file, public) in media {
        let target = out.join("media").join(public);
        if let Some(dir) = target.parent() {
            fs::create_dir_all(dir)?;
        }
        fs::copy(file, &target).with_context(|| format!("copying image {public}"))?;
    }
    Ok(())
}

/// File types allowed in `static/`. Anything else fails the build, so stray files
/// (notes, exports, `.DS_Store`) can't slip onto the public site.
const STATIC_TYPES: &[&str] = &[
    "woff2", "txt", "svg", "png", "jpg", "jpeg", "webp", "avif", "gif", "ico", "pdf",
];

/// Copy `static/` into `out/` as-is. Symlinks are not followed.
/// Image metadata is checked at commit time by `scripts/guard.sh`.
pub fn copy_static(dir: &Path, out: &Path) -> anyhow::Result<()> {
    if !dir.exists() {
        return Ok(());
    }
    for entry in WalkDir::new(dir).follow_links(false) {
        let entry = entry?;
        let rel = entry.path().strip_prefix(dir)?;
        let name = entry.file_name().to_string_lossy();
        if name.starts_with('.') {
            continue;
        }
        if entry.file_type().is_dir() {
            fs::create_dir_all(out.join(rel))?;
            continue;
        }
        if !entry.file_type().is_file() {
            continue;
        }
        let ext = entry
            .path()
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        if !STATIC_TYPES.contains(&ext.as_str()) {
            bail!(
                "static/{}: file type not allowed on the public site",
                rel.display()
            );
        }
        fs::copy(entry.path(), out.join(rel))
            .with_context(|| format!("copying static/{}", rel.display()))?;
    }
    Ok(())
}
