---
id: TASK-17
title: Format the published date in the reader view
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
ordinal: 3014
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The reader header shows the entry's published timestamp as the raw Feedbin ISO-8601 string (e.g. "2023-12-02T02:30:21.000000Z"), which is hard to read. Render it as a friendly long date-time like "Sunday, June 15, 2026 at 6:00 AM".

Entry.published is an Option<String> ISO-8601 value (src/feedbin.rs) currently used only for sorting (a raw string compare in load()). Add parsing + display formatting in the reader header (reader_text / the meta line in src/tui.rs) while leaving the sort behavior untouched. There is no date/time crate in the dependency tree today, so this likely adds chrono or time.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The reader header renders the published date as a long human-readable form: weekday name, full month name, day, year, and 12-hour time with AM/PM (e.g. "Sunday, June 15, 2026 at 6:00 AM")
- [ ] #2 The displayed time is converted from the source UTC value to the user's local timezone (document the chosen behavior; if local conversion is undesirable, fall back to UTC and say so)
- [ ] #3 An entry with a missing or unparseable published value degrades gracefully — the date line is omitted, never a panic or a raw/garbage string
- [ ] #4 The existing newest-first ordering (which sorts on the raw published string) is unchanged
- [ ] #5 A unit test covers formatting a known timestamp and the missing/unparseable case
- [ ] #6 docs/architecture.md (reader pipeline) and docs/data-model.md (published is now rendered, not only sorted) are updated in the same commit
<!-- AC:END -->
