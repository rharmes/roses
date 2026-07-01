---
id: TASK-19
title: Show a braille loading spinner + count in the lower-right corner
status: Done
assignee:
  - '@ross'
created_date: '2026-06-30 20:21'
updated_date: '2026-07-01 00:41'
labels:
  - rust
  - ui
dependencies: []
documentation:
  - docs/architecture.md
  - docs/data-model.md
priority: low
ordinal: 5014
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
When background work is in flight (today: image pre-fetches), show an animated braille spinner in the lower-right corner with a short status like "Loading 4 of 19 images". The UI loop already tracks an in-flight image counter and an image_queue (see the concurrency model in docs/architecture.md), so surface that as a footer-right indicator that animates while work remains and disappears when idle.

The run_loop redraws every ~100 ms (TICK) regardless of input, so the spinner can advance off a per-iteration frame counter. Per the project's no-flaky-tests rule, the frame must be freezable for tests.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 While images are still loading, the lower-right corner shows an animated braille spinner (frames cycling through ⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏) plus text like "Loading N of M images", where M is the total images for the current load and N is how many have finished
- [x] #2 The spinner animates over time (advances each UI tick, ~100 ms) without requiring keypresses
- [x] #3 When no background work remains, the indicator is hidden and the corner is clear
- [x] #4 The indicator is right-aligned and does not overlap or corrupt the existing footer help text; it degrades gracefully on a narrow terminal
- [x] #5 The indicator text/frame is produced by a pure function so a TestBackend test can assert it deterministically (frozen frame, no timing flakiness)
- [x] #6 docs/architecture.md (the loading indicator + in-flight counter) and docs/data-model.md (any new App field) are updated in the same commit
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. App gains spinner_tick: usize (animation frame) and image_urls: Vec<String> (the current load's distinct image URLs, in order). 2. refill_image_queue builds image_urls (deduped) alongside queueing. 3. image_progress()->Option<(done,total)>: done = image_urls whose cache state is Ready/Failed; None when total==0 or done>=total (all finished/idle). 4. Pure loading_indicator(done,total,tick)->String using SPINNER_FRAMES (⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏); testable with a frozen tick. 5. draw() footer: when image_progress is Some, split the footer with Layout::horizontal([Min(0), Length(w)]) — help flexes left, indicator right-aligned right (disjoint areas, never overlap; degrades on narrow width). 6. run_loop increments spinner_tick each iteration (loop ticks ~every 100ms, so it animates without input). 7. Tests: loading_indicator (frozen frames), image_progress (counts + None cases), TestBackend footer (frozen tick shows '⠋ Loading N of M images', help intact). 8. Update architecture.md (footer indicator) + data-model.md (new App fields).
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
App gained image_urls (Vec<String>: current load's distinct image URLs, built in refill_image_queue) and spinner_tick (usize, advanced once per run_loop iteration). image_progress()->Option<(done,total)> counts Ready/Failed vs total, None when no images or all resolved. Pure loading_indicator(done,total,tick) formats the braille frame + text. draw() splits the footer with Layout::horizontal([Min(0), Length(w)]) so the right-aligned indicator never overlaps the help (help flexes/truncates on narrow widths; disjoint areas). Verified render: ' ↑↓ move · … · q quit           ⠋ Loading 0 of 1 images'. fmt+clippy clean, 78 tests (3 new: pure indicator, progress counts+hide, TestBackend footer with frozen tick), stable 10/10.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Added an animated braille loading indicator in the footer's lower-right: 'SPINNER Loading N of M images' while background image fetches are in flight, hidden when idle. App tracks image_urls (the current load's distinct image URLs) and spinner_tick (advanced each ~100ms run_loop iteration, so it animates without input). image_progress() derives (done,total) from the image cache states; loading_indicator() is a pure function (frame passed in) so tests freeze it deterministically. The footer is split via Layout so the right-aligned indicator never overlaps or corrupts the help text and degrades gracefully on narrow terminals. Verified: fmt+clippy clean, 78 tests (incl. pure-function, progress-count/hide, and a frozen-frame TestBackend footer test), stable 10/10. docs/architecture.md (image pre-fetch + indicator) and docs/data-model.md (App fields) updated.
<!-- SECTION:FINAL_SUMMARY:END -->
