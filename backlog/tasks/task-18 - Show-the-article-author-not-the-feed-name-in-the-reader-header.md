---
id: TASK-18
title: Show the article author (not the feed name) in the reader header
status: To Do
assignee:
  - '@ross'
created_date: '2026-06-30 20:21'
labels:
  - rust
  - ui
dependencies: []
documentation:
  - docs/architecture.md
  - docs/data-model.md
priority: medium
ordinal: 4014
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The reader header repeats the feed/blog name, which is redundant — the feed is already the highlighted selection in the Sources column. Replace it with the entry's author when available.

Feedbin's entries.json includes a nullable "author" field that roses does not currently deserialize. Add author: Option<String> to feedbin::Entry (src/feedbin.rs) and render it in the reader header (reader_text in src/tui.rs) in place of the feed name. Note: this task and "Format the published date in the reader view" both edit the reader header, so whichever lands second should rebase.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 feedbin::Entry deserializes the Feedbin "author" field as Option<String>
- [ ] #2 The reader header shows the author name when the entry has one, in place of the feed/blog name
- [ ] #3 When the entry has no author, the line is omitted entirely (no empty label, and it does not fall back to showing the feed name) — the feed/blog name no longer appears in the reader header in either case
- [ ] #4 Tests cover both paths: author present (shown) and author absent (line omitted)
- [ ] #5 docs/data-model.md (new Entry.author field) and docs/architecture.md (reader header) are updated in the same commit
<!-- AC:END -->
