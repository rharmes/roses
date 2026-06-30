---
id: TASK-17
title: Format the published date in the reader view
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
ordinal: 3014
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The reader header shows the entry's published timestamp as the raw Feedbin ISO-8601 string (e.g. "2023-12-02T02:30:21.000000Z"), which is hard to read. Render it as a friendly long date-time like "Sunday, June 15, 2026 at 6:00 AM".

Entry.published is an Option<String> ISO-8601 value (src/feedbin.rs) currently used only for sorting (a raw string compare in load()). Add parsing + display formatting in the reader header (reader_text / the meta line in src/tui.rs) while leaving the sort behavior untouched. There is no date/time crate in the dependency tree today, so this likely adds chrono or time.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 The reader header renders the published date as a long human-readable form: weekday name, full month name, day, year, and 12-hour time with AM/PM (e.g. "Sunday, June 15, 2026 at 6:00 AM")
- [x] #2 The displayed time is converted from the source UTC value to the user's local timezone (document the chosen behavior; if local conversion is undesirable, fall back to UTC and say so)
- [x] #3 An entry with a missing or unparseable published value degrades gracefully — the date line is omitted, never a panic or a raw/garbage string
- [x] #4 The existing newest-first ordering (which sorts on the raw published string) is unchanged
- [x] #5 A unit test covers formatting a known timestamp and the missing/unparseable case
- [x] #6 docs/architecture.md (reader pipeline) and docs/data-model.md (published is now rendered, not only sorted) are updated in the same commit
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Add chrono (no-default-features, clock) for RFC3339 parse + local-tz formatting. 2. format_published(raw)->Option<String> via Local using PUBLISHED_FORMAT '%A, %B %-d, %Y at %-I:%M %p'; split a tz-agnostic core format_published_in(raw, tz) so tests are deterministic (Local is machine-dependent). 3. Reader meta line renders the formatted date; missing/unparseable -> line omitted (AC#3). 4. load() sort stays raw-string compare (AC#4). 5. Unit tests: format_published_in with Utc + a FixedOffset (tz conversion, AM/PM, day rollover); None for bad/empty input. 6. Update architecture.md + data-model.md.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Added chrono (no default features, 'clock' only — keeps it lean and avoids the time crate, whose local-offset detection errors in multithreaded programs; roses runs a tokio blocking pool). format_published(raw) parses RFC3339 via DateTime::parse_from_rfc3339 and renders in chrono::Local with PUBLISHED_FORMAT '%A, %B %-d, %Y at %-I:%M %p'. The tz-agnostic core format_published_in(raw, tz) is split out so unit tests pin Utc / a FixedOffset instead of depending on the host clock (no-flaky-tests rule). Missing/unparseable -> None, so the reader meta line is dropped. load() sort untouched (raw string compare). Verified: fmt+clippy clean, 74 tests, stable 10/10.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Reader header now renders the published date as a long, local-time, human-readable string (e.g. 'Saturday, December 2, 2023 at 2:30 AM') instead of the raw ISO-8601 value. Added chrono (clock feature only); format_published()/format_published_in() do the parse+format, with the tz-agnostic core unit-tested against fixed offsets (UTC + a -08:00 day-boundary case) so the tests don't depend on the host timezone. Missing or unparseable dates are dropped gracefully; newest-first ordering (raw published string compare in load()) is unchanged. docs/architecture.md (reader pipeline + dependency table) and docs/data-model.md (published row) updated. fmt+clippy clean, 74 tests, stable 10/10.
<!-- SECTION:FINAL_SUMMARY:END -->
