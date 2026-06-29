---
id: TASK-13
title: Wrap article titles in the articles pane and space items apart
status: To Do
assignee: []
created_date: '2026-06-29 20:00'
labels:
  - rust
  - ui
dependencies:
  - TASK-11
documentation:
  - docs/tui_research.md
priority: medium
ordinal: 13
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
In the articles pane each item is a single line, so long titles are truncated. Render each article's title wrapped across as many lines as needed so the full title is visible, and add a small gap (e.g. a blank line) below each item to separate them. Selection and navigation must still operate per-article, not per-line: up/down moves between whole articles and the highlight covers the entire wrapped item. Wrap to the pane's current inner width and recompute on resize.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Long article titles wrap onto multiple lines so the full title is visible (no truncation) in the articles pane.
- [ ] #2 Each article item is separated from the next by spacing (e.g. a blank line).
- [ ] #3 Up/down navigation moves between whole articles, and the selection highlight covers the entire wrapped item.
<!-- AC:END -->
