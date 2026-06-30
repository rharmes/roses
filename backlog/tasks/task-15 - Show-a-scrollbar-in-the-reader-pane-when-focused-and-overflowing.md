---
id: TASK-15
title: Show a scrollbar in the reader pane when focused and overflowing
status: Done
assignee:
  - '@ross'
created_date: '2026-06-29 23:31'
updated_date: '2026-06-30 00:17'
labels:
  - rust
  - ui
dependencies:
  - TASK-11
ordinal: 1014
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The reader pane scrolls but gives no visual indication of position or that there is more content below/above. Add a vertical scrollbar on the reader's right edge, shown only when the reader is the active (focused) pane and its wrapped content is taller than the viewport. This reuses the wrapped-height and reader_scroll values draw_reader already computes for scroll clamping.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 When the reader pane is focused and its wrapped content exceeds the visible height, a vertical scrollbar is drawn along the reader pane's right edge
- [x] #2 The scrollbar thumb reflects the current scroll position and content-to-viewport ratio, updating as the reader scrolls (top/middle/bottom are distinguishable)
- [x] #3 No scrollbar is shown when the reader pane is not focused, or when the content fits within the viewport (no overflow)
- [x] #4 Reader text layout/wrapping is unaffected by the scrollbar (no content clipped by the scrollbar track)
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Use ratatui's Scrollbar widget on the reader's right edge.
1. In draw_reader, reuse the already-computed wrapped height + inner_height + reader_scroll (the scroll clamp values). Overflow = wrapped > inner_height.
2. After rendering the reader Paragraph+block, if focused (Focus::Reader) AND overflow, render Scrollbar(VerticalRight) into area.inner(Margin{vertical:1, horizontal:0}) so the track sits on the right border between the corners. ScrollbarState::new(wrapped).viewport_content_length(inner_height).position(reader_scroll) — ratatui maps position=content_length-viewport to thumb-at-bottom, matching max_scroll.
3. Hidden when reader unfocused (focus Articles still shows the article, no bar) or when content fits (wrapped <= inner_height).
4. Clean style: thumb block, track matching the border, no begin/end arrows.
Tests: TestBackend — scrollbar cells present on right edge when reader focused + long content; absent when focused + short content; absent when content long but focus=Articles. fmt+clippy+test 10x.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented in draw_reader: after rendering the reader Paragraph+block, if focus==Reader && wrapped > inner_height, render Scrollbar(VerticalRight) into area.inner(Margin{vertical:1}) so the track rides the right border between the corners. State = ScrollbarState::new(wrapped).viewport_content_length(inner_height).position(reader_scroll) — reuses the existing scroll-clamp values; ratatui maps position=content_length-viewport to a bottom thumb (== max_scroll). Thumb █ over a │ track (matches the border); no begin/end arrows. Scrollbar is on the border, not over the padded text, so nothing is clipped (AC#4). Hidden when content fits or the reader isn't focused (the reader still shows the article under Articles focus, just no bar). 3 new tests (shows-when-focused+overflow; hidden-when-fits; hidden-when-unfocused) via TestBackend, scanning the right-edge column for the █ thumb. fmt+clippy clean, 64 tests, stable 10/10. docs/architecture.md updated.

Rebased onto main after TASK-14 merged (resolved the shared use ratatui::layout import line + the test-module-tail splice; 69 tests green pre-fix). Bug fix (user report: thumb stopped ~halfway at max scroll): ScrollbarState content_length must be the count of scroll POSITIONS (max_scroll+1), not the total line count — ratatui's thumb only reaches the track bottom when position==content_length-1 (its max_viewport_position = content_length-1 + viewport). Changed new(wrapped) → new(max_scroll+1); thumb now reaches the bottom row at full scroll (verified by dump: thumb at top row when scroll=0, last track row at max). Added regression test reader_scrollbar_thumb_reaches_the_bottom_when_fully_scrolled. 70 tests, fmt+clippy clean, stable 10/10. docs/architecture.md scrollbar note corrected.

UX tweak (user: held ↓ scrolled too slowly): reader ↑/↓ now scroll READER_SCROLL_STEP=3 lines per keypress instead of 1, so holding the arrow moves at a useful pace (held-scroll rate was capped at the OS key-repeat rate × 1 line). PgUp/PgDn still page by READER_PAGE=10. Updated the two tests asserting the old 1-line step (now READER_SCROLL_STEP) + the keybindings doc. 70 tests, stable 10/10.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Added a vertical scrollbar to the reader pane, shown only when the reader is the focused pane AND its wrapped content overflows the viewport (wrapped > inner_height). In draw_reader, after the Paragraph, render Scrollbar(VerticalRight) into area.inner(Margin{vertical:1}) so the track rides the right border between the corners (thumb █ over a │ track, no arrows); it reuses the existing wrapped/inner_height/reader_scroll scroll-clamp values. content_length is the count of scroll POSITIONS (max_scroll + 1), not the total line count — ratatui's thumb only reaches the bottom of the track when position == content_length-1, so the thumb now lands on the last track row at full scroll (a bug fix after initial review). Hidden when content fits or the reader isn't focused (a focused Articles pane still shows the article, just no bar). Also sped up reader scrolling: ↑/↓ now move READER_SCROLL_STEP=3 lines per press (was 1) so holding the arrow scrolls at a useful pace; PgUp/PgDn still page by READER_PAGE=10. Verified: cargo fmt --check, clippy --all-targets -D warnings, 70 tests (shows-when-focused+overflow, hidden-when-fits, hidden-when-unfocused, thumb-reaches-bottom-at-max-scroll; plus the two updated reader-scroll-step tests), stable 10/10, CI green. Branch rebased onto main after TASK-14 merged. docs/architecture.md updated (scrollbar mechanism + keybindings).
<!-- SECTION:FINAL_SUMMARY:END -->
