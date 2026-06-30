---
id: TASK-19
title: Show a braille loading spinner + count in the lower-right corner
status: To Do
assignee:
  - '@ross'
created_date: '2026-06-30 20:21'
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
- [ ] #1 While images are still loading, the lower-right corner shows an animated braille spinner (frames cycling through ⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏) plus text like "Loading N of M images", where M is the total images for the current load and N is how many have finished
- [ ] #2 The spinner animates over time (advances each UI tick, ~100 ms) without requiring keypresses
- [ ] #3 When no background work remains, the indicator is hidden and the corner is clear
- [ ] #4 The indicator is right-aligned and does not overlap or corrupt the existing footer help text; it degrades gracefully on a narrow terminal
- [ ] #5 The indicator text/frame is produced by a pure function so a TestBackend test can assert it deterministically (frozen frame, no timing flakiness)
- [ ] #6 docs/architecture.md (the loading indicator + in-flight counter) and docs/data-model.md (any new App field) are updated in the same commit
<!-- AC:END -->
