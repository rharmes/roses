---
id: TASK-28
title: Memoize rendered reader content instead of re-parsing HTML every frame
status: To Do
assignee: []
created_date: '2026-07-01 14:37'
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
- [ ] #1 Reader content is rebuilt only when the selected article, the reader width, or a relevant image cache state changes, not on every frame
- [ ] #2 Scrolling the reader does not trigger a re-parse
- [ ] #3 No visible behavior change: header, body, images, and scroll clamp render identically to today
<!-- AC:END -->
