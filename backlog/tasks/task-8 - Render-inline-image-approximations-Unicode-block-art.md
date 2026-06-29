---
id: TASK-8
title: Render inline image approximations (Unicode block-art)
status: To Do
assignee: []
created_date: '2026-06-29 00:56'
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
