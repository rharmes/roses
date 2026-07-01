---
id: TASK-46
title: Mute the focus/selection accent while the help overlay is open
status: To Do
assignee: []
created_date: '2026-07-01 22:42'
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
- [ ] #1 While the help overlay is open, the focused column's border + title and the selected-item highlight render in grey, not the rose accent
- [ ] #2 Closing the overlay restores the normal rose accent immediately, with no lingering grey
- [ ] #3 The overlay's own border/title stay rose (only the background chrome is muted); selection/focus state is unchanged
- [ ] #4 A TestBackend test asserts the focus/selection accent is grey while the overlay is open and rose after it closes
<!-- AC:END -->
