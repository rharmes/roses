---
id: TASK-40
title: See all unread beyond the newest-50 cap (pagination)
status: Done
assignee:
  - '@claude'
created_date: '2026-07-01 14:38'
updated_date: '2026-07-01 15:36'
labels:
  - feature
  - feedbin-api
  - epic
dependencies: []
references:
  - 'https://github.com/feedbin/feedbin-api/blob/master/content/pagination.md'
priority: high
ordinal: 26014
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TOP-PRIORITY (user-selected). Today load() takes only the newest DISPLAY_LIMIT (50) unread ids, so older unread entries are unreachable. Add pagination so the user can page through all unread. Feedbin paginates via the Links header (RFC5988 rel=next/last) and X-Feedbin-Record-Count. Implement a Links-header follower and either a load-more affordance or lazy loading as the user scrolls, holding results in memory. Can ship without the SQLite cache.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 The user can reach unread entries beyond the newest 50 (load-more or lazy paging)
- [x] #2 Unread counts and selection remain correct as pages are appended
- [x] #3 roses hydrates the next batch of unread ids on demand and stops when the full id list is exhausted (no re-fetch of the id list)
- [x] #4 Load-more logic covered by App-level tests: correct next-batch slice, append + re-sort, selection preserved by id, stop-when-exhausted, in-flight guard, and error restores the batch for retry
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Strategy: hydrate-on-demand from the full unread-id list (chosen over Links-header paging); no feedbin.rs changes, reuse entries(&ids). Trigger: auto lazy-load on approach.
1. load() returns the ids beyond the newest 50 as pending_ids (newest-first) in the Loaded payload.
2. App gains pending_ids: Vec<i64> and in_flight_more: Option<Vec<i64>> (guard + retry buffer); Msg gains LoadedMore(Result<Vec<Entry>,String>).
3. run_loop calls maybe_begin_load_more() each iteration: when selection is within LOAD_MORE_THRESHOLD (15) of the oldest loaded entry, pending is non-empty, and nothing is in flight, drain the next LOAD_MORE_BATCH (100) ids into in_flight_more and spawn_load_more (background entries fetch to Msg::LoadedMore).
4. apply(LoadedMore Ok): append + re-sort by published desc, clear in_flight_more, refill_image_queue. Err: restore batch to front of pending_ids, clear guard, set notice.
5. Invariant total_unread = entries.len() + pending_ids.len(); mark-read/undo already adjust entries+total_unread, pending untouched.
6. Footer right slot: image indicator when loading, else 'X of Y unread' when pending non-empty.
7. Reload (r) resets via a fresh load().
8. App-level tests + docs (architecture.md, data-model.md) in the same commit.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented hydrate-on-demand (no Links-header paging; unread_entries.json already returns the full id list). load() keeps ids beyond the newest 50 as pending_ids. App gained pending_ids + in_flight_more; Msg::LoadedMore appends + re-sorts by published. run_loop calls maybe_begin_load_more() each iteration: when the selection is within LOAD_MORE_THRESHOLD (15) of the oldest loaded entry, it drains the next LOAD_MORE_BATCH (100) ids and spawn_load_more hydrates them. Error path restores the batch to pending for retry. Footer shows 'X of Y unread' while more remain. No feedbin.rs changes. Docs (architecture + data-model) updated same commit; also backfilled the TASK-28 App fields the table was missing. 3 new App-level tests; 91 tests pass, 10x stable, fmt/clippy clean.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Lazy-loads all unread beyond the newest-50 cap. Since unread_entries.json returns the complete id list, load() retains the overflow as pending_ids and run_loop auto-hydrates the next 100-id batch (spawn_load_more -> Msg::LoadedMore, appended + re-sorted) as the reader nears the oldest loaded entry; an in_flight_more guard prevents overlap and restores the batch on failure. Footer shows X of Y unread. Selection-by-id + the total_unread invariant keep counts/cursor correct. Covered by 3 App-level tests.
<!-- SECTION:FINAL_SUMMARY:END -->
