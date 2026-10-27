//! Markdown settings and the custom HTML output for wikilinks.

use std::fmt::Write;

use comrak::html::{ChildRendering, dangerous_url, format_node_default};
use comrak::nodes::{AstNode, NodeValue};
use comrak::{Arena, create_formatter};

/// A wikilink's URL is rewritten to one of these before rendering when it doesn't
/// point at a published note. Every wikilink is rewritten, so these can't clash
/// with anything an author typed.
pub const PRIVATE: &str = "kwaj:private";
pub const MISSING: &str = "kwaj:missing";
/// Prefix for an image URL that couldn't be resolved; the original URL follows it.
pub const MISSING_IMAGE: &str = "kwaj:missing-image:";

/// Alt text that editors leave as a placeholder. Read aloud, it's noise, so it's dropped.
const PLACEHOLDER_ALTS: &[&str] = &[
    "alt text",
    "alt",
    "description",
    "image",
    "photo",
    "picture",
    "img",
];

pub fn is_placeholder_alt(alt: &str) -> bool {
    PLACEHOLDER_ALTS.contains(&alt.trim().to_lowercase().as_str())
}

#[derive(PartialEq)]
enum Block {
    /// A paragraph that's only images: shown as a grid.
    Gallery,
    /// One image followed by an italic line: a figure with a caption.
    Figure,
    Text,
}

/// Decide how a paragraph renders from what's in it, ignoring line breaks and spaces.
fn block_kind<'a>(paragraph: &'a AstNode<'a>) -> Block {
    let parts: Vec<&AstNode> = paragraph
        .children()
        .filter(|n| match &n.data().value {
            NodeValue::SoftBreak | NodeValue::LineBreak => false,
            NodeValue::Text(t) => !t.trim().is_empty(),
            _ => true,
        })
        .collect();
    let is_image = |n: &&AstNode| matches!(n.data().value, NodeValue::Image(_));
    let is_italic = |n: &&AstNode| matches!(n.data().value, NodeValue::Emph);
    match parts.as_slice() {
        [img, caption] if is_image(img) && is_italic(caption) => Block::Figure,
        many if many.len() >= 2 && many.iter().all(is_image) => Block::Gallery,
        _ => Block::Text,
    }
}

fn is_external(url: &str) -> bool {
    url.starts_with("http://") || url.starts_with("https://")
}

/// Markdown settings shared by the index and render passes.
/// Raw HTML stays off: comrak replaces it with a comment.
pub fn options() -> comrak::Options<'static> {
    let mut options = comrak::Options::default();
    options.extension.table = true;
    options.extension.strikethrough = true;
    options.extension.autolink = true;
    options.extension.tasklist = true;
    options.extension.footnotes = true;
    options.extension.wikilinks_title_after_pipe = true;
    // Gives headings ids, so [[note#Heading]] has somewhere to land.
    options.extension.header_id_prefix = Some(String::new());
    options
}

pub fn parse<'a>(
    arena: &'a Arena<'a>,
    body: &str,
    options: &comrak::Options,
) -> &'a comrak::nodes::AstNode<'a> {
    comrak::parse_document(arena, body, options)
}

create_formatter!(GardenHtml, {
    NodeValue::Paragraph => |context, node, entering| {
        match block_kind(node) {
            Block::Gallery => context.write_str(if entering { "<div class=\"gallery\">\n" } else { "</div>\n" })?,
            Block::Figure => context.write_str(if entering { "<figure>\n" } else { "</figure>\n" })?,
            Block::Text => return format_node_default(context, node, entering),
        }
    },
    NodeValue::Emph => |context, node, entering| {
        if node.parent().is_some_and(|p| block_kind(p) == Block::Figure) {
            context.write_str(if entering { "<figcaption>" } else { "</figcaption>" })?;
        } else {
            return format_node_default(context, node, entering);
        }
    },
    NodeValue::Image(ref link) => |context, node, entering| {
        if entering {
            if let Some(original) = link.url.strip_prefix(MISSING_IMAGE) {
                context.write_str(r#"<span class="image-missing">Image not found: "#)?;
                context.escape(original)?;
                context.write_str("</span>")?;
            } else {
                let alt = node.collect_text();
                let alt = if is_placeholder_alt(&alt) { String::new() } else { alt };
                context.write_str(r#"<img src=""#)?;
                if !dangerous_url(&link.url) {
                    context.escape_href(&link.url)?;
                }
                context.write_str(r#"" alt=""#)?;
                context.escape(&alt)?;
                context.write_str(r#"" loading="lazy" decoding="async">"#)?;
            }
        }
        return Ok(ChildRendering::Skip);
    },
    // Links that leave the site don't get to see or control this page.
    NodeValue::Link(ref link) => |context, node, entering| {
        if !is_external(&link.url) {
            return format_node_default(context, node, entering);
        }
        if entering {
            context.write_str(r#"<a href=""#)?;
            if !dangerous_url(&link.url) {
                context.escape_href(&link.url)?;
            }
            context.write_str("\"")?;
            if !link.title.is_empty() {
                context.write_str(r#" title=""#)?;
                context.escape(&link.title)?;
                context.write_str("\"")?;
            }
            context.write_str(r#" rel="noopener noreferrer">"#)?;
        } else {
            context.write_str("</a>")?;
        }
    },
    // Wide tables scroll inside a labelled, focusable region instead of the page.
    NodeValue::Table(..) => |context, entering| {
        context.write_str(if entering {
            r#"<div class="table-scroll" role="region" aria-label="Table" tabindex="0"><table>"#
        } else {
            "</table></div>\n"
        })?;
    },
    // Code blocks scroll sideways, so keyboard users need to be able to focus them.
    NodeValue::CodeBlock(ref block) => |context, entering| {
        if entering {
            context.write_str(r#"<pre tabindex="0"><code"#)?;
            if let Some(lang) = block.info.split_whitespace().next() {
                context.write_str(r#" class="language-"#)?;
                context.escape(lang)?;
                context.write_str("\"")?;
            }
            context.write_str(">")?;
            context.escape(&block.literal)?;
            context.write_str("</code></pre>\n")?;
        }
        return Ok(ChildRendering::Skip);
    },
    NodeValue::WikiLink(ref link) => |context, entering| {
        match link.url.as_str() {
            // Links to unpublished notes: label only, no element, no trace of the target.
            PRIVATE => {}
            MISSING => context.write_str(if entering {
                r#"<span class="link-missing" title="This note doesn't exist yet">"#
            } else {
                "</span>"
            })?,
            href => {
                if entering {
                    context.write_str(r#"<a class="wikilink" href=""#)?;
                    context.escape_href(href)?;
                    context.write_str(r#"">"#)?;
                } else {
                    context.write_str("</a>")?;
                }
            }
        }
    },
});

/// Render a parsed document with the garden's wikilink output.
pub fn to_html<'a>(root: &'a comrak::nodes::AstNode<'a>, options: &comrak::Options) -> String {
    let mut html = String::new();
    // Writing to a String can't fail.
    let _ = GardenHtml::format_document(root, options, &mut html);
    html
}
