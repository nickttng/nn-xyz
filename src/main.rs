//! kwaj: builds a digital garden from Markdown notes.

mod assets;
mod content;
mod links;
mod loci;
mod markdown;
mod render;
mod serve;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};

#[derive(Parser)]
#[command(version, about = "Build the kwaj digital garden")]
struct Cli {
    /// Folder of Markdown notes. Can point at an Obsidian vault.
    #[arg(
        long,
        env = "KWAJ_CONTENT_DIR",
        default_value = "content",
        global = true
    )]
    content: PathBuf,

    /// Treat broken wikilinks as errors (use in CI).
    #[arg(long, global = true)]
    strict: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Args, Clone)]
struct SiteDirs {
    #[arg(long, default_value = "dist")]
    out: PathBuf,
    #[arg(long, default_value = "templates")]
    templates: PathBuf,
    #[arg(long, default_value = "styles")]
    styles: PathBuf,
    #[arg(long = "static", default_value = "static")]
    static_dir: PathBuf,
}

#[derive(Subcommand)]
enum Command {
    /// Build the site into the output folder.
    Build(SiteDirs),
    /// Build, serve on http://127.0.0.1:4000, and rebuild on every save.
    Serve(SiteDirs),
    /// Check notes for problems without building.
    Check,
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(code) => code,
        Err(err) => {
            eprintln!("error: {err:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> anyhow::Result<ExitCode> {
    match cli.command {
        Command::Check => {
            let Some(garden) = load(&cli.content, cli.strict)? else {
                return Ok(ExitCode::FAILURE);
            };
            println!("{} published notes, no errors", garden.0.notes.len());
        }
        Command::Build(dirs) => {
            if !build(&cli.content, cli.strict, &dirs, false)? {
                return Ok(ExitCode::FAILURE);
            }
        }
        Command::Serve(dirs) => {
            let paths = serve::Paths {
                out: dirs.out.clone(),
                watch: vec![
                    cli.content.clone(),
                    dirs.templates.clone(),
                    dirs.styles.clone(),
                    dirs.static_dir.clone(),
                ],
            };
            let (content, strict) = (cli.content, cli.strict);
            serve::run(paths, move || {
                // Errors are printed and the last good build keeps being served.
                if let Err(err) = build(&content, strict, &dirs, true) {
                    eprintln!("error: {err:#}");
                }
            })?;
        }
    }
    Ok(ExitCode::SUCCESS)
}

/// Load and index the garden, printing diagnostics. `None` if there were errors.
fn load(
    content: &std::path::Path,
    strict: bool,
) -> anyhow::Result<Option<(content::Garden, links::Index, loci::Ontology)>> {
    let mut garden = content::load(content)?;
    let index = links::build(&mut garden, strict);
    let ontology = loci::build(&mut garden, &index);
    for diagnostic in &garden.diagnostics {
        eprintln!("{diagnostic}");
    }
    Ok((!garden.has_errors()).then_some((garden, index, ontology)))
}

/// Full build. Returns `false` if notes had errors (nothing is written then).
fn build(
    content: &std::path::Path,
    strict: bool,
    dirs: &SiteDirs,
    dev: bool,
) -> anyhow::Result<bool> {
    let Some((garden, index, ontology)) = load(content, strict)? else {
        return Ok(false);
    };
    let site = render::Site {
        garden: &garden,
        index: &index,
        ontology: &ontology,
    };
    render::build(&site, content, &dirs.templates, &dirs.out, dev)?;
    assets::build_css(&dirs.styles, &dirs.out)?;
    assets::copy_static(&dirs.static_dir, &dirs.out)?;
    assets::copy_media(&index.media, &dirs.out)?;
    println!(
        "built {} notes into {}",
        garden.notes.len(),
        dirs.out.display()
    );
    Ok(true)
}
