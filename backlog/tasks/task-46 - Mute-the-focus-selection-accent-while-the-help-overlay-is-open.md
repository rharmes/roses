---
id: TASK-46
title: Mute the focus/selection accent while the help overlay is open
status: Done
assignee:
  - '@ross'
created_date: '2026-07-01 22:42'
updated_date: '2026-07-01 23:00'
labels:
  - feature
  - ux
dependencies:
  - TASK-32
priority: low
ordinal: 31014
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
When the ? help overlay (TASK-32) is open, the three-column UI behind it still draws the rose accent on the focused column (border + title) and on the selected item (the selection highlight bar). Since the overlay is the visual focus, de-emphasize that background chrome: while show_help is set, render the focused column's border/title and the current-item highlight in grey (a neutral dim color) instead of the rose accent, and restore the rose accent once the overlay closes. The overlay's own rose border/title stays rose. This is a small draw-time change (thread an 'accent muted' flag, or resolve the accent color, through column_block + the highlight style); it must not touch selection/focus state.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 While the help overlay is open, the focused column's border + title and the selected-item highlight render in grey, not the rose accent
- [x] #2 Closing the overlay restores the normal rose accent immediately, with no lingering grey
- [x] #3 The overlay's own border/title stay rose (only the background chrome is muted); selection/focus state is unchanged
- [x] #4 A TestBackend test asserts the focus/selection accent is grey while the overlay is open and rose after it closes
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented on branch task-46-mute-accent-under-help. Added theme::MUTED (grey Rgb 0x808080) and App::accent() -> Color returning MUTED when show_help else ROSE; column_block (focused border/title) and highlight (selection bar) now resolve their color via self.accent(). Display-only — focus/selection state untouched. Scoped to those two per AC #1; the overlay's own border/title (draw_help_overlay uses theme::ROSE directly), the footer keys, and the reader title stay rose. Reader title deliberately excluded: it lives in the memoized reader_text, so muting it would require keying the reader cache on show_help — out of scope. New TestBackend test asserts border+selection are rose (closed) -> grey + reversed (open, overlay title still rose) -> rose again (closed). 122 tests pass, fmt+clippy clean, 5x stable. Docs: architecture Help-overlay subsection notes the muting + scope.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Shipped as PR #30 (merged to main). While the ? help overlay is open, the focus/selection accent recedes to grey so the overlay draws the eye. Added theme::MUTED + App::accent() (ROSE normally, MUTED when show_help); column_block (focused border/title) and highlight (selection bar) resolve through it. Display-only — focus/selection state untouched. Scoped to those two per AC #1; overlay border/title, footer keys, and reader title stay rose (reader title excluded as it lives in the memoized reader_text). TestBackend test asserts rose(closed)->grey+reversed(open, overlay title still rose)->rose(closed). 122 tests pass, fmt+clippy clean, 5x stable; docs updated in-commit.
<!-- SECTION:FINAL_SUMMARY:END -->
