---
id: TASK-30
title: Mark all read (current source and whole window)
status: To Do
assignee: []
created_date: '2026-07-01 14:37'
labels:
  - feature
dependencies: []
priority: high
ordinal: 16014
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Add a bulk mark-read action. The Feedbin client already batches DELETE /unread_entries.json at 1000 ids. Provide mark-all-read for the selected source and for the whole loaded window, with optimistic removal plus undo consistent with the single-entry flow (a single undo restores the batch).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A key marks every article in the selected source read; another key or prefix marks the whole loaded window read
- [ ] #2 The bulk write goes out in one batched request; entries are removed optimistically and restored on failure
- [ ] #3 Undo restores the whole batch
- [ ] #4 Tests cover the batched write and the optimistic removal/rollback
<!-- AC:END -->
