---
id: TASK-22
title: Support podcast feeds via the enclosure field
status: Done
assignee:
  - '@ross'
created_date: '2026-06-30 21:19'
updated_date: '2026-07-01 02:17'
labels:
  - rust
  - ui
dependencies: []
documentation:
  - docs/architecture.md
  - docs/data-model.md
priority: low
ordinal: 8014
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Podcast/media feeds attach an enclosure (the audio/video file). Feedbin returns it in extended mode: enclosure { enclosure_url, enclosure_type, enclosure_length, itunes_duration, itunes_image }. Make these feeds usable in roses: show that an entry is a podcast (type + duration) and let the user play it.

Requires mode=extended on the entries request (add the query param if not already present) and an enclosure field on feedbin::Entry. roses is a reader, not a media player — the pragmatic MVP is to open the enclosure_url with the user's configured browser/opener (reuse browser::resolve/run), not to play audio inline.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 The entries request uses mode=extended and feedbin::Entry deserializes the enclosure object (Option, tolerant of absence)
- [x] #2 Entries with an enclosure show a clear indicator in the reader — at least the media type and itunes_duration (e.g. an audio glyph + 47:30)
- [x] #3 A keypress opens/plays the enclosure_url via the existing browser/opener launch path
- [x] #4 Entries without an enclosure are unaffected; tests cover enclosure parsing and the indicator/open behavior; docs/architecture.md + docs/data-model.md (Entry) updated
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Entry gains enclosure: Option<Enclosure{enclosure_url, enclosure_type, itunes_duration}>. Reader header shows a 'Podcast · <duration>' line when present (format_duration: numeric seconds -> H:MM:SS/M:SS, else pass-through; no emoji, terminal-safe). Open precedence: o opens enclosure_url for podcasts (before external_url/url). Tests: parse, indicator shown, format_duration, o opens enclosure.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Entry gains enclosure: Option<Box<Enclosure{enclosure_url, enclosure_type, itunes_duration}>>. Reader header shows a dim 'kind · duration' line (podcast_indicator: Audio/Video/Media from the type + format_duration, which turns bare seconds into H:MM:SS/M:SS, else passes through). o opens the enclosure for podcasts (selected_url precedence enclosure > external_url > url) via the existing browser/opener path. No emoji (terminal-safe). Tests: parse, indicator 'Audio · 47:03', format_duration, o-opens-enclosure. fmt+clippy clean, 83 tests, stable 10/10.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Podcast/media feeds are now usable: entries carry the extended-mode enclosure, the reader header shows a 'kind · duration' indicator (e.g. 'Audio · 47:03' — media kind from the type, duration via format_duration), and 'o' opens/plays the enclosure_url through the configured browser/opener (open precedence: enclosure > external_url > url). Entries without an enclosure are unchanged. Verified: fmt+clippy clean, 83 tests (enclosure parse, indicator, format_duration seconds+passthrough, o-opens-enclosure), stable 10/10. docs updated.
<!-- SECTION:FINAL_SUMMARY:END -->
