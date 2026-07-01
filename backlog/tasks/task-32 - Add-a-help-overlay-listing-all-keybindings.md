---
id: TASK-32
title: Add a help overlay listing all keybindings
status: Done
assignee:
  - '@ross'
created_date: '2026-07-01 14:37'
updated_date: '2026-07-01 22:44'
labels:
  - feature
  - ux
dependencies: []
priority: medium
ordinal: 18014
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The footer shows only a subset of keys. Add a ? key that toggles a modal overlay listing every binding (navigation, focus, mark/undo, star, open, reload, next-unread, quit, etc.), dismissed by ?/Esc/q. Render it from a single source of truth so it stays in sync with handle_key.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Pressing ? opens a help overlay listing all active keybindings; Esc or ? closes it
- [x] #2 The overlay does not interfere with background loading or lose selection state
- [x] #3 A TestBackend test asserts the overlay renders the expected keys
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Recommended next after TASK-30: the help overlay is the proper home for the growing keybinding set. TASK-30 adds M (mark selected source read) and A (mark whole loaded window read, y/n confirm) — the 1-line footer can only hint at these ('M src · A all'); the overlay should document m/M/A read + u undo and the rest in full.

Implemented on branch task-32-help-overlay. Added a single BINDINGS table (group/keys/desc + optional compact footer form) as the source of truth for BOTH the footer hint and the ? overlay, so they can't drift. ? sets App.show_help; draw() floats draw_help_overlay() — a Clear'd rose-bordered box centered via centered_rect(Flex::Center), grouped headings from help_lines(), sized to content and clamped to area. Modal: while open, handle_key treats ANY key as dismiss (so ?/Esc/q all close; q closes help rather than quitting) and returns Action::None. Pure chrome — reads no mutable state, sets only show_help, so background loads/selection are untouched (AC #2). Per user request, moved the M/A bulk-mark hints OUT of the footer (now flagged overlay-only in BINDINGS) and added a '? help' hint; footer is now: ↑↓ move · ←→ focus · o open · m read · u undo · r reload · ? help · q quit. 5 new tests incl. a TestBackend render assert (AC #3), toggle/any-key-close, Esc/q don't quit, and load-doesn't-disturb-selection (AC #2). 121 tests pass, fmt+clippy clean, 5x stable. Docs updated in-commit (README shortcuts, architecture keybindings+footer diagram+new Help overlay subsection, data-model show_help field). Also created TASK-45 (configurable hex highlight color) per user request.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Shipped as PR #29 (merged to main as 30baa06). Added a ? help overlay listing every active keybinding grouped under headings (Navigation/Reading/Marking read/App): a Clear'd rose-bordered box centered via Flex::Center, sized to content. Modal-but-forgiving: any key closes it (?/Esc/q all dismiss; q closes help rather than quitting). Both the overlay and the 1-line footer render from one BINDINGS table (single source of truth) so they can't drift; per request the M/A bulk-mark hints moved out of the footer (overlay-only) and a '? help' hint was added. Pure chrome — reads no mutable state, sets only show_help — so background loads/image pre-fetch/selection are untouched (AC #2). 5 new tests incl. a TestBackend render assert (AC #3); 121 pass, fmt+clippy clean, 5x stable. Docs updated in-commit. Spawned follow-ups TASK-45 (hex highlight color) and TASK-46 (grey-out accent under the overlay).
<!-- SECTION:FINAL_SUMMARY:END -->
