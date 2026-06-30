---
id: TASK-25
title: Fix reader image art wrapping into half-height rows after a resize
status: Done
assignee:
  - '@ross'
created_date: '2026-06-30 21:56'
updated_date: '2026-06-30 21:56'
labels:
  - rust
  - ui
dependencies: []
documentation:
  - docs/architecture.md
priority: medium
ordinal: 11014
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Half-block image art is rendered once at the reader's content width when it is fetched, then cached by URL. If the terminal later narrows (a resize, or an initial size that settles after launch), the stale art is wider than the reader's current Wrap width, so ratatui wraps each art line into a full row + a short fragment row — the image appears with 'half-height' rows interleaved.

Root cause confirmed by dumping the reader buffer: with matched widths the art is solid; with art wider than the wrap width every line wraps (full row of N cells, then a fragment of the overflow). Not caused by the TASK-17/18 date/author work (that only shifts the image vertically); surfaced while reviewing that branch.

Fix: reader_text clips each Ready art line to the reader's current inner width (clip_line_to_width) so it can never exceed the wrap width — a no-op when the art fits, a graceful right-crop after a shrink, until a reload re-renders at the new width. Text lines are not clipped (they still wrap).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Image art never wraps into fragment/half-height rows in the reader, even when the cached art is wider than the reader's current content width (post-resize)
- [x] #2 A regression test asserts no reader line can exceed the wrap width when art is over-wide (and the art row survives, clipped, not dropped)
- [x] #3 Art that already fits is unaffected — no cropping or change in normal use; body text still wraps
- [x] #4 docs/architecture.md (reader pipeline) documents the clipping
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Reproduce via a TestBackend buffer dump: confirm matched art/wrap width is solid; confirm art wider than wrap width wraps into full+fragment rows. 2. Clip each Ready art line to the reader's current inner width in reader_text (new max_width param, passed from draw_reader) via clip_line_to_width. 3. Regression test asserting no line exceeds the wrap width when art is over-wide. 4. Re-dump to confirm solid rows. 5. Doc the clip.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Confirmed root cause empirically with a buffer dump: art at 80 cells wrapped at reader width 44 produced alternating 44-wide full rows and 36-wide fragment rows (the reported 'half-height' look). After clipping art to max_width, the same scenario renders solid full-width rows. Not a TASK-17/18 regression — the date/author change only moves the image's vertical position; verified the image rendering code (images::render + the Segment::Image arm) is byte-identical to main. clip_line_to_width truncates a Line's spans to N display columns using unicode-width (correct for any cell width; ▀ is width 1). reader_text gained a max_width param; draw_reader computes inner_width before building the text and passes it. Verified: fmt+clippy clean, 75 tests (incl. the new regression), stable 10/10. Also fixed an orphaned reader_text doc comment left by the TASK-17 edit.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Fixed reader images showing interleaved 'half-height' fragment rows. Root cause (confirmed by a TestBackend buffer dump, not the date/author change): half-block art is rendered+cached at the reader width when fetched, so after the terminal narrows the stale-wide art exceeds the reader's Wrap width and each line wraps into a full row + a short fragment. Fix: reader_text now clips each Ready art line to the reader's current inner width (clip_line_to_width; max_width passed from draw_reader) so art can never overflow and wrap — a no-op when it fits, a graceful right-crop after a shrink (until reload re-renders at the new width); body text still wraps. Regression test asserts no reader line exceeds the wrap width with over-wide art. fmt+clippy clean, 75 tests, stable 10/10; docs/architecture.md updated.
<!-- SECTION:FINAL_SUMMARY:END -->
