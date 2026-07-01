---
id: TASK-30
title: Mark all read (current source and whole window)
status: Done
assignee:
  - '@ross'
created_date: '2026-07-01 14:37'
updated_date: '2026-07-01 22:16'
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
- [x] #1 A key marks every article in the selected source read; another key or prefix marks the whole loaded window read
- [x] #2 The bulk write goes out in one batched request; entries are removed optimistically and restored on failure
- [x] #3 Undo restores the whole batch
- [x] #4 Tests cover the batched write and the optimistic removal/rollback
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. feedbin: no change — mark_read/mark_unread already take &[i64] and batch at 1000.
2. tui: generalize the optimistic write path to a *batch* — Undone { batch: Vec<(Entry,usize)> } and Msg::Write { op, batch, result }; single 'm' becomes a batch of one, so one apply arm + one spawn_write serve all four cases.
3. tui: add begin_mark_source_read (all loaded entries of selected_source) and begin_mark_window_read (all loaded entries) via a shared remove_batch (remove back-to-front, restore ascending-index order so undo reinserts correctly); reinsert_batch restores a whole batch.
4. tui: keys — M = mark selected source read (instant); A = mark whole loaded window read behind a y/n footer confirmation (pending_confirm: Option<Confirm>, next key intercepted). Scope = loaded entries only; pending_ids stay unread (near_tail auto-hydrates the next batch).
5. tui: footer — add 'M src · A all' hints; render the confirm prompt in place of help while pending.
6. Tests: batched-write batch contents, source/window optimistic removal + batch undo restore (order preserved), rollback on failure, and the y/n confirmation gate. Update the 3 existing single-mark tests to the batch shape.
7. Docs: README shortcuts + docs/architecture.md (keybindings, bulk mark section, Action/Msg/Confirm) + docs/data-model.md (Undone/Msg::Write/Action/pending_confirm) in the same commit.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented on branch task-30-mark-all-read. Unified single + bulk mark-read into one batch path: Msg::Write/Undone/spawn_write now carry Vec<(Entry,usize)> (single m/u = batch of one). New keys: M = mark selected source's loaded articles read (instant, works from any focus); A = mark whole loaded window read behind a y/n footer confirmation (pending_confirm intercepts the next key). Scope = loaded entries only; pending_ids stay unread and auto-hydrate via near_tail. begin_mark_source_read/begin_mark_window_read share remove_batch (remove back-to-front, restore ascending-index order); reinsert_batch restores a whole batch at original indices so undo reverses a bulk mark in one step. 8 new tests (batch contents, source/window optimistic removal + ordered undo restore, failure rollback, the y/n confirmation gate incl. cancel + empty-window guard); 3 existing single-mark tests updated to the batch shape. 116 tests pass, fmt+clippy clean, suite run 5x — stable. Docs updated in-commit (README shortcuts, architecture keybindings + optimistic-mark section + Action/Confirm, data-model App field + enums).
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Shipped as PR #28 (merged to main as 6149598). Two bulk mark-read actions: M marks every loaded article in the selected source read (instant, any focus); A marks the whole loaded window read behind a y/n footer confirmation. Unified the single + bulk paths into one batch write (Msg::Write/Undone/spawn_write carry Vec<(Entry,usize)>; single m/u = batch of one), so each bulk mark is one batched Feedbin request and one u restores the whole batch in a single step; failed writes roll the whole batch back. Scope = loaded window only (pending_ids stay unread, auto-hydrate via near_tail); order preserved across undo via remove_batch (back-to-front) + reinsert_batch (original indices). 116 tests pass (8 new + 3 migrated), fmt+clippy clean, 5x stable. Docs updated in-commit.
<!-- SECTION:FINAL_SUMMARY:END -->
