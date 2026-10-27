---
title: Colophon
publish: true
unlisted: true
stage: evergreen
planted: 2026-10-25
tended: 2026-10-26
summary: How this site is made, and a few notes on reading it.
---

This site is a garden, not a feed. Notes are planted while they're still rough and tended over time. Some will change. A few will stay as they are. Where a note shows a stage, it tells you how far along it is: seedling, budding, or evergreen.

## Writing

Everything begins as a Markdown file, in my own words. I write each draft myself, then use an LLM for a final pass on voice and tone. The front matter tells the site how to set each page; the body is the writing itself. Notes link to one another with double brackets, the way they do in a Markdown editor with bi-directional linking.

## Reading

There's no tracking here. No analytics, no cookies, nothing fetched from elsewhere.

## Building

A small program, written in Rust, reads the notes and sets them into plain HTML. I want it to be quick (the whole site builds in under a second).

It stands on good work by others: comrak for Markdown, MiniJinja for templates, Lightning CSS for styles.

## Setting

Headings are set in IBM Plex Mono; the body in Inter. Both are open-source, and both are served from here. The warm paper, #FBFAF6, carries over from the previous version of this site. The page follows your device into light or dark.


