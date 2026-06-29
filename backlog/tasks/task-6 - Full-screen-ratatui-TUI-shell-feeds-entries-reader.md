---
id: TASK-6
title: Full-screen ratatui TUI shell (feeds / entries / reader)
status: To Do
assignee: []
created_date: '2026-06-29 00:56'
labels:
  - rust
  - ui
dependencies:
  - TASK-4
documentation:
  - docs/tui_research.md
priority: medium
ordinal: 6
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Replace the stdout proof-of-concept with the real terminal UI: a full-screen ratatui app (crossterm backend) with a feeds/entries list and a reader pane, driven by async tokio fetches so the UI stays responsive. See docs/tui_research.md section 3.1.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Full-screen ratatui app renders a list/detail layout (entries list + reader pane) on the crossterm backend
- [ ] #2 Entries load from Feedbin asynchronously without blocking input
- [ ] #3 Keyboard navigation moves selection and scrolls the reader; quitting restores the terminal cleanly
<!-- AC:END -->
