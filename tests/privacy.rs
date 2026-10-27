//! End-to-end privacy checks: build the fixture garden and inspect every output file.

use std::fs;
use std::path::Path;
use std::process::Command;

fn build_fixture() -> tempfile::TempDir {
    let out = tempfile::tempdir().unwrap();
    let status = Command::new(env!("CARGO_BIN_EXE_kwaj"))
        .args(["--content", "tests/fixtures/garden", "build", "--out"])
        .arg(out.path())
        .status()
        .unwrap();
    assert!(status.success());
    out
}

fn read(out: &tempfile::TempDir, slug: &str) -> String {
    fs::read_to_string(out.path().join(slug).join("index.html")).unwrap()
}

fn all_output(dir: &Path) -> String {
    let mut text = String::new();
    for entry in walkdir::WalkDir::new(dir) {
        let entry = entry.unwrap();
        // Every text file the site ships. Fonts and images are binary.
        let is_text = entry.path().extension().is_some_and(|e| {
            ["html", "css", "js", "json", "xml", "txt"].contains(&e.to_str().unwrap())
        });
        if entry.file_type().is_file() && is_text {
            text.push_str(&fs::read_to_string(entry.path()).unwrap());
        }
    }
    text
}

#[test]
fn builds_only_published_notes() {
    let out = build_fixture();
    assert!(
        out.path()
            .join("designing-for-slowness/index.html")
            .exists()
    );
    assert!(out.path().join("field-notes/index.html").exists());
    assert!(!out.path().join("private-diary").exists());
    assert!(!out.path().join("workspace").exists());
}

#[test]
fn private_content_never_reaches_output() {
    let out = build_fixture();
    let html = all_output(out.path());
    for marker in [
        "PRIVATE-FIXTURE",
        "private-diary",
        "no-front-matter",
        ".obsidian",
    ] {
        assert!(!html.contains(marker), "output leaked {marker:?}");
    }
}

#[test]
fn output_has_no_source_paths_or_raw_html() {
    let out = build_fixture();
    let html = all_output(out.path());
    assert!(
        !html.contains("tests/fixtures"),
        "output leaked a source path"
    );
    assert!(!html.contains(".md\""), "output leaked a .md filename");
    assert!(
        !html.contains("<script>"),
        "raw HTML from Markdown was rendered"
    );
    assert!(html.contains("Content-Security-Policy"));
}

#[test]
fn refuses_to_clear_a_folder_holding_notes() {
    // Copy the fixtures so a failure here can never delete the real ones.
    let dir = tempfile::tempdir().unwrap();
    let notes = dir.path().join("notes");
    fs::create_dir(&notes).unwrap();
    fs::copy(
        "tests/fixtures/garden/designing-for-slowness.md",
        notes.join("a.md"),
    )
    .unwrap();

    for out in [notes.clone(), dir.path().to_path_buf()] {
        let status = Command::new(env!("CARGO_BIN_EXE_kwaj"))
            .arg("--content")
            .arg(&notes)
            .args(["build", "--out"])
            .arg(&out)
            .status()
            .unwrap();
        assert!(!status.success(), "build should refuse to clear {out:?}");
        assert!(notes.join("a.md").exists(), "a note was deleted");
    }
}

#[test]
fn links_to_private_notes_render_as_plain_text() {
    let out = build_fixture();
    let html = read(&out, "designing-for-slowness");
    assert!(html.contains("I wrote about this in a diary entry and elsewhere."));
}

#[test]
fn backlinks_and_broken_links_render() {
    let out = build_fixture();
    let field = read(&out, "field-notes");
    assert!(field.contains("linked from"));
    // Templates escape `/` as `&#x2f;` inside attributes; browsers read it as `/`.
    let field_decoded = field.replace("&#x2f;", "/");
    assert!(
        field_decoded.contains(r#"<a href="/designing-for-slowness/">Designing for Slowness</a>"#)
    );
    assert!(field.contains("Some interfaces should be slow. See Field Notes."));

    let slow = read(&out, "designing-for-slowness");
    assert!(slow.contains(r#"<span class="link-missing""#));
}

#[test]
fn strict_mode_fails_on_broken_links() {
    let status = Command::new(env!("CARGO_BIN_EXE_kwaj"))
        .args(["--content", "tests/fixtures/garden", "--strict", "check"])
        .status()
        .unwrap();
    assert!(!status.success());
}

#[test]
fn real_build_ships_no_dev_script_and_self_hosts_fonts() {
    let out = build_fixture();
    let html = all_output(out.path());
    assert!(
        !html.contains("__kwaj"),
        "dev live-reload script leaked into a build"
    );
    assert!(
        !html.contains("fonts.googleapis"),
        "third-party font request"
    );
    assert!(out.path().join("style.css").exists());
    assert!(out.path().join("fonts/inter-variable.woff2").exists());
}

#[test]
fn home_note_renders_at_site_root() {
    let out = build_fixture();
    let home = fs::read_to_string(out.path().join("index.html")).unwrap();
    assert!(home.contains("A fixture front page."));
    assert!(home.contains("Fixture Column"));
    assert!(
        !out.path().join("home").exists(),
        "home note also rendered at /home/"
    );
}

#[test]
fn tag_pages_list_only_published_notes() {
    let out = build_fixture();
    let design = read(&out, "tags/design");
    assert!(design.contains("Designing for Slowness"));
    assert!(out.path().join("tags/index.html").exists());
    // The private fixture's tags must not create pages.
    let tags = all_output(out.path());
    assert!(!tags.contains("PRIVATE-FIXTURE"));
}

#[test]
fn blog_style_notes_publish_and_only_their_images_ship() {
    let out = build_fixture();
    let html = read(&out, "blog-style").replace("&#x2f;", "/");
    assert!(html.contains(r#"<img src="/media/images/public.png" alt="A single sage pixel""#));
    assert!(html.contains("<figcaption>A caption</figcaption>"));
    assert!(out.path().join("media/images/public.png").exists());
    // Linked only from a private note, so it must never be copied.
    assert!(!out.path().join("media/images/private.png").exists());
}

#[test]
fn unlisted_notes_publish_but_stay_out_of_lists() {
    let out = build_fixture();
    assert!(out.path().join("about-this-site/index.html").exists());
    let home = fs::read_to_string(out.path().join("index.html")).unwrap();
    assert!(!home.contains("About This Site"));
    assert!(!out.path().join("tags/meta").exists());
}

#[test]
fn loci_pages_build_and_never_name_private_notes() {
    let out = build_fixture();
    assert!(out.path().join("loci/index.html").exists());
    assert!(out.path().join("loci/places/dunkeld/index.html").exists());
    assert!(
        out.path()
            .join("loci/artefacts/the-organ/index.html")
            .exists()
    );
    // `read-alongside: private-diary` must leave no trace anywhere.
    assert!(!out.path().join("loci/works/private-diary").exists());
    let all = all_output(out.path());
    assert!(!all.contains("private-diary"));
    // Relations are hidden on notes for now (see templates/note.html). When they
    // come back, assert the published target shows the inverse relation again:
    // `read alongside` and `Blog Style` on designing-for-slowness.
    let slow = read(&out, "designing-for-slowness");
    assert!(!slow.contains("read alongside"));
}

#[test]
fn pinned_notes_sit_above_the_latest_list() {
    let out = build_fixture();
    let home = fs::read_to_string(out.path().join("index.html")).unwrap();
    let pinned = home.find("home-pinned").expect("pinned section");
    let latest = home.find(">recent notes<").expect("recent notes list");
    // The home note's own columns may link to it too, so look from the pinned slot on.
    let slot = &home[pinned..latest];
    assert!(
        slot.contains("Field Notes</a>"),
        "pinned note should be in the pinned slot"
    );
    assert!(
        !home[latest..].contains("Field Notes</a>"),
        "pinned note shouldn't repeat in the dated list"
    );
}
