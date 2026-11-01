# nickng.xyz

The source for [nickng.xyz](https://nickng.xyz/), my digital garden, though I aspire to make it more of an observatory. I use observatory as a metaphor: a place to collect observations, shift perspective, and look outward through reading notes, field notes from travel, and other writing TBD.

## How it's built

- **Writing:** Markdown files in `content/`, with front matter and `[[wikilinks]]` between notes.
- **Tooling:** a small Rust program in `src/` that turns the notes into plain HTML. It uses [comrak](https://github.com/kivikakk/comrak) for Markdown, [MiniJinja](https://github.com/mitsuhiko/minijinja) for templates, and [Lightning CSS](https://lightningcss.dev/) for styles.
- **Type:** IBM Plex Mono and Inter, both open source and served from the site itself.
- **Hosting:** GitHub Pages.

To run it locally:

```sh
cargo run -- serve   # http://127.0.0.1:4000
```
