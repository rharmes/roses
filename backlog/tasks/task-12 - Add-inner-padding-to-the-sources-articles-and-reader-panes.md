---
id: TASK-12
title: 'Add inner padding to the sources, articles, and reader panes'
status: In Progress
assignee:
  - '@ross'
created_date: '2026-06-29 19:59'
updated_date: '2026-06-29 20:42'
labels:
  - rust
  - ui
dependencies:
  - TASK-11
documentation:
  - docs/tui_research.md
priority: medium
ordinal: 12
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The three TUI panes render text flush against their borders. Add a small inner margin between each pane's border and its content for breathing room - roughly one cell of horizontal padding (~6px), with a modest top/bottom inset. Keep the existing line height/spacing of body text unchanged. In ratatui this is the bordered Block's padding (Padding) or an inset inner Rect, applied consistently to all three panes (sources, articles, reader).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 The sources, articles, and reader panes inset their content from the borders with horizontal padding of about one cell, giving visible breathing room.
- [x] #2 Body-text line height/spacing is unchanged.
- [x] #3 Padding is applied consistently across all three panes.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Add a consistent inner Padding to all three panes via the shared column_block() Block (Padding::uniform(1): ~1 cell horizontal + modest 1-row top/bottom inset). This covers AC#1 and AC#3 in one place; the empty-state Paragraphs reuse the same block so they inset too.
2. Fix draw_reader scroll-clamp math: derive the true content rect from block.inner(area) (accounts for border AND padding) instead of the hardcoded saturating_sub(2), so reader_scroll stays correct with padding.
3. Body text keeps single-line spacing — padding only insets the content rect, no inter-line gaps (AC#2).
4. Tests: add/adjust a TestBackend assertion that content is inset from the border; run fmt + clippy -D warnings + test, 10x for stability; watch CI green.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented via a single Padding::uniform(1) on the shared column_block() — 1 cell horizontal + a 1-row top/bottom inset, applied to all three panes (AC#1, AC#3). Body text keeps single-line spacing; padding only insets the content rect (AC#2). Fixed draw_reader's scroll-clamp + draw()'s reader_width to derive geometry from block.inner(area) (border + padding) instead of hardcoded area-2, so scroll bounds and pre-fetched image width stay correct under padding. Added regression test panes_inset_content_from_their_borders (verifies the inset at the exact border/padding/content columns of all three panes). fmt + clippy -D warnings clean; 55 tests pass, stable 10/10. Updated docs/architecture.md (layout + reader-scroll notes).
<!-- SECTION:NOTES:END -->
