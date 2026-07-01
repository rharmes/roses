---
id: TASK-32
title: Add a help overlay listing all keybindings
status: To Do
assignee: []
created_date: '2026-07-01 14:37'
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
- [ ] #1 Pressing ? opens a help overlay listing all active keybindings; Esc or ? closes it
- [ ] #2 The overlay does not interfere with background loading or lose selection state
- [ ] #3 A TestBackend test asserts the overlay renders the expected keys
<!-- AC:END -->
