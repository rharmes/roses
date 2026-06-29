---
id: TASK-8
title: Render inline image approximations (Unicode block-art)
status: In Progress
assignee:
  - '@claude'
created_date: '2026-06-29 00:56'
updated_date: '2026-06-29 17:09'
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
- [ ] #1 Images in entry content render as Unicode block-art inline in the reader (no graphics protocol required)
- [ ] #2 Rendering degrades gracefully when an image cannot be fetched or decoded
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Approach: render images as in-process Unicode half-block art (image crate: decode+resize; each cell = '▀' with truecolor fg=top pixel / bg=bottom pixel) producing ratatui Lines spliced INTO the reader text. Chosen over the ratatui-image widget because images must flow inline within the single scrollable reader Paragraph (a widget-in-Rect doesn't compose with scrolling text) — a flagged deviation from the task's 'prefer ratatui-image'. 1. content: extract <img src> URLs in document order from entry HTML (alongside the text). 2. async: fetch+decode+render on a spawn_blocking task (reqwest blocking client), cache by URL in App as Loading/Ready(Vec<Line>)/Failed; Msg::Image{url,result}. 3. reader: splice cached art at each image's position, with a '[image loading...]' placeholder and '[image unavailable]' fallback on fetch/decode failure (AC#2); render at the reader width captured at fetch time. deps: image. 4. tests: half-block render of a tiny synthetic image, img-URL extraction, graceful failure. fmt/clippy/test.
<!-- SECTION:PLAN:END -->
