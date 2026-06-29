---
id: TASK-12
title: 'Add inner padding to the sources, articles, and reader panes'
status: To Do
assignee: []
created_date: '2026-06-29 19:59'
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
- [ ] #1 The sources, articles, and reader panes inset their content from the borders with horizontal padding of about one cell, giving visible breathing room.
- [ ] #2 Body-text line height/spacing is unchanged.
- [ ] #3 Padding is applied consistently across all three panes.
<!-- AC:END -->
