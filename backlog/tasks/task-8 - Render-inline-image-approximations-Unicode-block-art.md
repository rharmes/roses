---
id: TASK-8
title: Render inline image approximations (Unicode block-art)
status: Done
assignee:
  - '@claude'
created_date: '2026-06-29 00:56'
updated_date: '2026-06-29 19:47'
labels:
  - rust
  - feature
dependencies:
  - TASK-6
documentation:
  - docs/tui_research.md
priority: low
ordinal: 8
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Show low-fidelity image approximations for entry images using Unicode/ANSI block-art that works on any terminal (no Sixel/Kitty). Prefer the ratatui-image halfblocks widget. See docs/tui_research.md sections 3.1 and 4.2.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Images in entry content render as Unicode block-art inline in the reader (no graphics protocol required)
- [x] #2 Rendering degrades gracefully when an image cannot be fetched or decoded
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Approach: render images as in-process Unicode half-block art (image crate: decode+resize; each cell = '▀' with truecolor fg=top pixel / bg=bottom pixel) producing ratatui Lines spliced INTO the reader text. Chosen over the ratatui-image widget because images must flow inline within the single scrollable reader Paragraph (a widget-in-Rect doesn't compose with scrolling text) — a flagged deviation from the task's 'prefer ratatui-image'. 1. content: extract <img src> URLs in document order from entry HTML (alongside the text). 2. async: fetch+decode+render on a spawn_blocking task (reqwest blocking client), cache by URL in App as Loading/Ready(Vec<Line>)/Failed; Msg::Image{url,result}. 3. reader: splice cached art at each image's position, with a '[image loading...]' placeholder and '[image unavailable]' fallback on fetch/decode failure (AC#2); render at the reader width captured at fetch time. deps: image. 4. tests: half-block render of a tiny synthetic image, img-URL extraction, graceful failure. fmt/clippy/test.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented (half-block-into-text approach, as approved — a flagged deviation from the task's 'prefer ratatui-image', because images must flow inline in the single scrolling reader Paragraph). src/images.rs: render(image, max_cols) -> Vec<Line> half-block art ('▀' with fg=top pixel / bg=bottom pixel, run-length-coalesced; aspect-corrected; width<=80, height<=40 cells); fetch_and_render() fetches over a PLAIN no-auth client (never replay Feedbin creds off-site), 10s timeout + 16MB cap, then decodes (image crate: png/jpeg/gif/webp) and renders. tui.rs: content_blocks() splits entry HTML into ordered Text/Image segments (extract_img_src ignores srcset); reader_text splices cached art at each image, with '[image loading...]' placeholder and '[image unavailable]' on failure (AC#2); a per-URL cache (Loading/Ready/Failed) in App; run_loop kicks off a spawn_blocking fetch (Msg::Image) for each uncached image of the on-screen article, sized to the reader width. dep: image (png,jpeg,gif,webp). 50 tests (render colours + caps, content segmentation, srcset-safe extraction, placeholder/failure rendering), stable 10x. AC#2 test-backed and checked. AC#1 (real images rendering inline) needs a live look at an article with images.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Inline image approximations as Unicode half-block art (no graphics protocol). src/images.rs: render() maps a decoded image to half-block Lines (fg=top pixel / bg=bottom pixel, run-length-coalesced, aspect-corrected, size-capped); fetch_and_render() fetches over a plain NO-auth client (never replays Feedbin creds off-site) with a timeout + 16MB cap, then decodes (image crate: png/jpeg/gif/webp). tui.rs: content_blocks() splits entry HTML into ordered text/image segments (srcset-safe src extraction); the reader splices cached art at each image with '[image loading...]' / '[image unavailable]' states (AC#2). Images pre-fetch proactively on a bounded (<=6 concurrent) background queue in on-screen top-to-bottom order, with a focused-article bump for still-unfetched images (hybrid). Approach: half-block-into-text rather than the ratatui-image widget, so images flow inline in the single scrolling reader Paragraph (approved deviation, flagged). Verified: fmt, clippy -D warnings, 54 tests; user live-verified inline image rendering and pre-fetch order. All ACs met.
<!-- SECTION:FINAL_SUMMARY:END -->
