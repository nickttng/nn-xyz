# nickng.xyz

A personal digital garden: professional + personal writing, authored as Markdown with YAML front matter, compiled to a static site by a small custom Rust binary, and hosted on GitHub Pages from a **public** repo.

## Who you're working with
The owner is a designer and lightweight developer. Explain Rust decisions in plain terms, prefer readable code over clever code, and keep templates and CSS editable without touching Rust.

## Shape of the repo
- `content/` — published Markdown notes only, plus `content/images/`. Private writing lives outside the repo.
- `templates/` — MiniJinja templates: `base`; `note` and the layouts that extend it (`essay`, `log`, `project`); `home`, `tags`, `loci`; and partials prefixed with `_`.
- `styles/` — hand-written CSS, bundled by Lightning CSS.
- `static/` — fonts and site files, copied as-is (allow-listed types only).
- `src/` — the Rust build engine.
- `dist/` — build output. Never edit, never commit.
- `.github/workflows/` — CI checks and the Pages deploy.

## Setup (once per clone)
```sh
brew install gitleaks exiftool webp     # pre-commit hook + scripts/images.sh
curl https://sh.rustup.rs -sSf | sh     # Rust
scripts/setup.sh                        # turns on .githooks/ (works with GitHub Desktop too)
```

## Commands
- `cargo run -- serve` — dev server on http://127.0.0.1:4000 with live reload (restart it after Rust changes)
- `cargo run -- build` — build `content/` into `dist/`
- `cargo run -- check` — report front matter problems and broken links without building
- `cargo run -- --strict check` — same, but broken links fail
- `KWAJ_CONTENT_DIR=~/path/to/vault cargo run -- build` — build from a private vault locally
- `scripts/guard.sh --all` — the privacy guard CI runs (only covers tracked files; new files are checked at commit by the hook)
- `scripts/images.sh` — audit images for size, format, colour profile, and metadata (`optimize FILE…` to fix)

## Publishing
- Merging to `main` deploys via `.github/workflows/deploy.yml` (Pages source: GitHub Actions). Other branches and PRs run the same checks but don't deploy.
- The custom domain lives in Settings → Pages; there is no `CNAME` file.
- Actions in the workflow are pinned to commit SHAs. Keep them pinned when updating.

## Hard rules
- **The repo is public, history included.** Never commit a note that isn't explicitly published (`publish: true` or `draft: false`), personal data, secrets, or images with any metadata (EXIF, XMP, IPTC).
- Links to unpublished notes must not leak their title, slug, or path into any output.
- A build must never silently drop content: broken wikilinks and bad front matter are reported with file + line.
- No client-side JS framework, no third-party scripts. JS is optional enhancement only.
- The owner's prose is theirs. Never change the body of a note without an explicit yes; propose edits instead. Front matter changes are fine when asked for, but say so.
- Changes to `content/`, `src/`, `templates/`, workflows, or dependencies get a pass from the `security-engineer` role.
- Changes to `templates/`, `styles/`, the renderer, or any new layout or component get a pass from the `accessibility-architect` role. WCAG 2.2 AA is the bar in light and dark mode.
- Commit messages and PRs describe the change only, with no tool or assistant attribution.

## Before you finish (every change)
1. `cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test` all pass.
2. `cargo run -- --strict check` has no new errors (CI fails on broken links); new warnings are explained to the owner.
3. Dry-run the pre-commit hook on the change: `git add -A && .githooks/pre-commit; git reset -q` (only when nothing is already staged).
4. If you touched a note's body, prove it: compare checksums before and after.
5. Update the skill or role whose guidance your change made stale. Stale guidance is worse than none.
6. Ask the relevant role for a pass (table below), and report what it found, including what couldn't be checked.
7. If you changed Rust and the dev server is running, restart it.
8. Hand over a suggested commit message (no attribution lines).

## Skills (`.agents/skills/`)
- `rust-engine` — architecture and crate choices for the build tool.
- `content-model` — the front matter schema, layouts, images, and the Loci ontology.
- `garden-links` — wikilinks, backlinks, slugs, the link graph.
- `css-system` — tokens, typography, and the no-framework CSS approach.
- `web-images` — image budgets, compression, and keeping pages with photos fast.

## Roles (`.agents/roles/`)
| Role | Reviews | Can block? |
|---|---|---|
| `security-engineer` | Privacy and security: content, engine, templates, workflows, dependencies, and identifying details about people in prose | Yes |
| `accessibility-architect` | WCAG 2.2 AA in light and dark mode; tracks the WCAG 3 draft | Yes |
| `editor` | Voice, typos, clarity, and alt text in the owner's writing. Suggests; never edits prose without a yes | No |
| `cataloguer` | The loci vocabulary: names, relations, guardrails, consistency across notes | No |

Only the two blocking roles can stop a change. Advisory roles propose; the owner decides.
