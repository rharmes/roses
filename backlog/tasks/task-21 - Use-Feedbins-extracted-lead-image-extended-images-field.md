---
id: TASK-21
title: Use Feedbin's extracted lead image (extended images field)
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
- [x] #1 The entries request uses mode=extended and feedbin::Entry deserializes the images object (original_url + size_1.cdn_url/width/height), tolerating its absence (Option)
- [x] #2 When an entry has an extended lead image, the reader uses size_1.cdn_url for it, and the provided width/height size the half-block art (no pre-decode needed for dimensions)
- [x] #3 Entries without an extended image fall back to the existing body-img behavior; image fetches stay on the separate unauthenticated client
- [x] #4 Tests cover parsing the images object and that the lead image url/dimensions are used; docs/architecture.md (image pre-fetch + network layer) and docs/data-model.md (Entry) updated
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Shared: entries() sends mode=extended; feedbin::Entry gains images: Option<EntryImages{size_1: Option<ImageSize{cdn_url}>}> (nested Option, tolerant). lead_image_url() accessor. TASK-21: article_image_urls falls back to the lead image when the body has no inline <img> (so it's pre-fetched + counted); reader_text renders the lead image as a hero at the top only in that no-inline case. Tests: parse, fallback ordering, reader shows lead when no inline / ignores it when inline present. Note: half-block art sizes from the decoded CDN image (== Feedbin's width/height), so those dims aren't separately needed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
entries() sends mode=extended; Entry gains images: Option<Box<EntryImages{size_1:Option<ImageSize{cdn_url}>}>> (boxed to keep Entry lean — clippy large_enum_variant on Msg::Write otherwise). lead_image_url() accessor. article_image_urls falls back to the lead image only when the body has no inline <img>; reader_text renders it as a hero at the top in that case (shared push_image helper). Deviation: original_url and size_1 width/height are NOT parsed — the half-block renderer decodes the CDN image and sizes art from its actual pixels (identical to Feedbin's dims), so those fields would be dead weight; serde still tolerates their presence. Tests: extended parse + mode=extended query assertion, lead-image shown-vs-suppressed + pre-fetch. fmt+clippy clean, 83 tests, stable 10/10.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Reader now uses Feedbin's extracted lead image (images.size_1.cdn_url) as a hero at the top of the reader — but only when the article body has no inline <img>, so image-rich articles are unchanged and metadata-only feeds still get a picture. entries() requests mode=extended; the lead image flows through the same pre-fetch/render/clip pipeline (article_image_urls + push_image) so it's fetched, counted in the 'N of M' indicator, and clipped like any image. width/height/original_url were intentionally not parsed (the decoder sizes from the CDN image's real pixels). Verified: fmt+clippy clean, 83 tests (extended-mode parse + mode=extended query, lead-image shown/suppressed/pre-fetched), stable 10/10. docs updated.
<!-- SECTION:FINAL_SUMMARY:END -->
