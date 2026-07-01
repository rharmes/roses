---
id: TASK-28
title: Memoize rendered reader content instead of re-parsing HTML every frame
status: Done
assignee:
  - '@claude'
created_date: '2026-07-01 14:37'
updated_date: '2026-07-01 15:05'
labels:
  - perf
  - refactor
dependencies: []
priority: medium
ordinal: 14014
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
draw_reader calls reader_text() every ~100ms tick, which re-parses the selected article HTML (content_blocks/decode_entities/sanitize), rebuilds the whole Text, then clones it again to measure wrapped height via line_count. Cache the rendered reader Text keyed by (selected entry id, reader inner width) and invalidate when the selection, width, or the entry image cache state changes, so a static article is not re-parsed ten times a second.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Reader content is rebuilt only when the selected article, the reader width, or a relevant image cache state changes, not on every frame
- [x] #2 Scrolling the reader does not trigger a re-parse
- [x] #3 No visible behavior change: header, body, images, and scroll clamp render identically to today
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Add reader_cache (key: entry_id,inner_width,image_generation; stores Text + wrapped line_count) and image_generation:u64 to App. 2. Bump image_generation on Msg::Image apply. 3. Factor draw_reader's build into reader_render() that rebuilds only on key miss and returns whether it rebuilt (observability for tests). 4. Tests: repeated render = hit (no re-parse); width/selection/image_generation change = miss. 5. fmt/clippy/test.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Added ReaderCache (key: entry_id,width,image_generation; stores built Text + wrapped height) and image_generation:u64 to App, bumped on Msg::Image. draw_reader now calls ensure_reader_cache(), which rebuilds (re-parses HTML) only on a key miss and returns whether it rebuilt; idle frames and scrolling reuse the cache. New test reader_cache_rebuilds_only_on_key_change asserts hit/miss transitions. 88 tests pass, clippy/fmt clean; existing reader render/scroll tests unchanged confirm no visible behavior change.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Memoized the reader render (ReaderCache keyed by entry id, reader width, and an image-generation counter); draw_reader re-parses article HTML only on a key miss, not every ~100ms frame or scroll. No visible behavior change; covered by a hit/miss cache test with existing render/scroll tests unchanged.
<!-- SECTION:FINAL_SUMMARY:END -->
