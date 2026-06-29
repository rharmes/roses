---
id: TASK-7
title: Mark entries read with undo (Feedbin unread_entries sync)
status: Done
assignee:
  - '@claude'
created_date: '2026-06-29 00:56'
updated_date: '2026-06-29 16:10'
labels:
  - rust
  - feature
dependencies:
  - TASK-3
references:
  - 'https://github.com/feedbin/feedbin-api'
priority: medium
ordinal: 7
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Implement the core read-state feature: mark entries read as they are seen and allow that to be undone. Uses Feedbin's unread_entries endpoint (DELETE to mark read, POST to mark unread), batched to at most 1000 ids per request.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Viewing or selecting an entry marks it read via DELETE /unread_entries.json
- [x] #2 An undo action restores unread state via POST /unread_entries.json
- [x] #3 Writes are batched to at most 1000 ids per request and reflected in the UI state
- [x] #4 Failures roll back local state so client and server stay consistent
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. feedbin.rs: mark_read(&[i64]) -> Vec<i64> (DELETE /unread_entries.json) and mark_unread(&[i64]) -> Vec<i64> (POST /unread_entries.json). JSON body {unread_entries:[...]} with Content-Type application/json; charset=utf-8, batched at <=1000 ids/request, returns the server-echoed changed IDs; empty ids = no request. mockito tests: method+path+body+content-type, response parse, >1000 batching, empty no-op.
2. tui.rs keybindings: free u/d (reader scroll -> PgUp/PgDn + Space/b); add m = mark selected entry read, u = undo last mark. OPTIMISTIC UI with rollback: 'm' removes the selected entry from the list + decrements total_unread immediately, then a background spawn_blocking mark_read; on success push it to an undo stack, on FAILURE re-insert at its index (rollback) + footer notice (AC#4). 'u' pops the undo stack, re-inserts optimistically + background mark_unread; on failure remove again + re-push + notice. Rollback/finalize info travels with the entry through the result Msg (no separate pending map).
3. Interpretation of AC#1 'viewing or selecting marks read': explicit 'm' on the currently-selected (viewed) entry — safer than auto-marking everything arrowed past; auto-on-select is a one-line switch if preferred.
4. Tests: client mark/unmark + batching (mockito); TUI state transitions (begin_mark_read removes + apply(Write Ok) pushes undo + apply(Write Err) rolls back; begin_undo re-inserts + failure path) — all deterministic, no network. fmt/clippy -D/test +10x; push -> CI. Live: user marks an entry (unread drops), undo (returns).
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
feedbin.rs: mark_read (DELETE) / mark_unread (POST) to /unread_entries.json with body {unread_entries:[...]} + Content-Type application/json; charset=utf-8, batched at <=1000 IDs, returning the server-echoed changed IDs; 4 mockito tests (DELETE+body+content-type, POST, >1000 -> 2 requests, empty no-op). tui.rs: keys m=mark read, u=undo (freed by moving reader scroll to space/b + PgUp/PgDn). Optimistic UI with rollback: begin_mark_read removes the selected entry + decrements unread immediately; a background spawn_blocking mark_read returns Msg::Write; on Ok it's pushed to the undo stack, on Err re-inserted (rollback) with a red footer notice (AC#4). begin_undo pops the stack, re-inserts optimistically + background mark_unread; on Err removes again, re-pushes (retryable), notices. A fresh reload clears the undo stack. 4 TUI state-transition tests: optimistic-remove + failure rollback, success->undo round trip, undo-failure retryable, reload-clears-undo. Verified: fmt, clippy --all-targets -- -D warnings, 32 tests (8 new), stable across 10 runs. All 4 ACs test-backed: AC#1 DELETE, AC#2 POST, AC#3 <=1000 batch + UI reflects state, AC#4 rollback.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Added Feedbin read-state sync with undo. feedbin.rs: mark_read() (DELETE /unread_entries.json) and mark_unread() (POST) send a JSON body batched at the 1000-id limit and return the server-echoed changed ids. tui.rs: 'm' marks the selected entry read, 'u' undoes the last mark (reader scroll moved to space/b + PgUp/PgDn). Optimistic UI with rollback — the entry leaves the list immediately, the write runs on a background spawn_blocking task, and a failure re-inserts it (or for a failed undo keeps it read and retryable) with a footer notice, keeping client and server consistent; a reload clears the undo stack. AC#1 ('viewing/selecting marks read') implemented as an explicit 'm' on the selected (viewed) entry. Verified: fmt, clippy -D warnings, 32 tests (8 new: 4 mockito DELETE/POST/body/batching + 4 TUI optimistic/rollback/undo state transitions), stable 10x + green CI. All 4 ACs test-backed; finalized on the user's go-ahead.
<!-- SECTION:FINAL_SUMMARY:END -->
