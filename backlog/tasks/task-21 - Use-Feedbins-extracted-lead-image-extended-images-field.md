---
id: TASK-21
title: Use Feedbin's extracted lead image (extended images field)
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
priority: medium
ordinal: 7014
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
roses currently scrapes the article's image by pulling img src out of the body HTML. Feedbin's extended mode returns an images object per entry with a curated lead image and known dimensions: { original_url, size_1: { cdn_url, width, height } }. Prefer that as the article's primary image — it is more reliable (handles feeds whose image lives in metadata, not the body) and the width/height let us size the half-block art without a decode round-trip.

This requires switching the entries request to mode=extended (add the query param to the entries() client call if not already present) and adding an images field to feedbin::Entry. Inline body images keep working as today; this is about the representative lead image and using its dimensions.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The entries request uses mode=extended and feedbin::Entry deserializes the images object (original_url + size_1.cdn_url/width/height), tolerating its absence (Option)
- [ ] #2 When an entry has an extended lead image, the reader uses size_1.cdn_url for it, and the provided width/height size the half-block art (no pre-decode needed for dimensions)
- [ ] #3 Entries without an extended image fall back to the existing body-img behavior; image fetches stay on the separate unauthenticated client
- [ ] #4 Tests cover parsing the images object and that the lead image url/dimensions are used; docs/architecture.md (image pre-fetch + network layer) and docs/data-model.md (Entry) updated
<!-- AC:END -->
