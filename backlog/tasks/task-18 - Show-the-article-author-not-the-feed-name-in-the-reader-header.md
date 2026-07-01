---
id: TASK-18
title: Show the article author (not the feed name) in the reader header
status: Done
assignee:
  - '@ross'
created_date: '2026-06-30 20:21'
updated_date: '2026-06-30 21:02'
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
- [x] #1 feedbin::Entry deserializes the Feedbin "author" field as Option<String>
- [x] #2 The reader header shows the author name when the entry has one, in place of the feed/blog name
- [x] #3 When the entry has no author, the line is omitted entirely (no empty label, and it does not fall back to showing the feed name) — the feed/blog name no longer appears in the reader header in either case
- [x] #4 Tests cover both paths: author present (shown) and author absent (line omitted)
- [x] #5 docs/data-model.md (new Entry.author field) and docs/architecture.md (reader header) are updated in the same commit
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Add author: Option<String> to feedbin::Entry (from Feedbin entries.json); update the struct doc comment (author no longer 'ignored'). 2. Reader meta line shows author when present, in place of the feed name; reader_text drops its feed param. 3. No author -> line omitted; feed/blog name never appears in the reader header. 4. Tests: author present (shown) + absent (omitted); feed name absent from the reader render; feedbin JSON test asserts author parses. 5. Update data-model.md (Entry.author) + architecture.md (reader header).
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Added author: Option<String> to feedbin::Entry (serde deserializes Feedbin's nullable 'author'); updated the struct doc comment (author no longer 'ignored'). reader_text dropped its feed param — the meta line now joins [author?, formatted-date?] with ' · ', omitting the feed/blog name entirely (it's the highlighted source on the left). No author -> the author segment is skipped; empty/whitespace authors are trimmed and treated as absent. Tests cover author present/absent and the feed name not leaking into the reader; feedbin JSON test asserts author parses (and stays None when null). fmt+clippy clean, 74 tests, stable 10/10.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
The reader header no longer repeats the feed/blog name (redundant with the highlighted source column); it shows the entry's author instead when Feedbin provides one. Added Entry.author (deserialized from entries.json); reader_text builds the meta line from author and the formatted date, joined by ' · ', dropping any absent part — and never falls back to the feed name. Verified with new tests (author shown; author-absent shows date only; meta line omitted when nothing to show; JSON parse asserts author) plus the feed-name-absent guard. docs/data-model.md (new Entry.author row) and docs/architecture.md (reader header) updated. fmt+clippy clean, 74 tests, stable 10/10.
<!-- SECTION:FINAL_SUMMARY:END -->
