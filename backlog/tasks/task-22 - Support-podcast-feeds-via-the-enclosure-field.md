---
id: TASK-22
title: Support podcast feeds via the enclosure field
status: To Do
assignee:
  - '@ross'
created_date: '2026-06-30 21:19'
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
- [ ] #1 The entries request uses mode=extended and feedbin::Entry deserializes the enclosure object (Option, tolerant of absence)
- [ ] #2 Entries with an enclosure show a clear indicator in the reader — at least the media type and itunes_duration (e.g. an audio glyph + 47:30)
- [ ] #3 A keypress opens/plays the enclosure_url via the existing browser/opener launch path
- [ ] #4 Entries without an enclosure are unaffected; tests cover enclosure parsing and the indicator/open behavior; docs/architecture.md + docs/data-model.md (Entry) updated
<!-- AC:END -->
