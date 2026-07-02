---
id: TASK-44
title: 'Delta sync (part 2): incremental hydration + updated_entries refresh'
status: In Progress
assignee:
  - '@ross'
created_date: '2026-07-01 18:52'
updated_date: '2026-07-02 18:25'
labels:
  - feature
  - feedbin-api
  - perf
dependencies:
  - TASK-42
  - TASK-41
priority: medium
ordinal: 29014
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Follow-up to TASK-42 (which did the ETag/Last-Modified 304 fast-path). On a 200 unread change, hydrate only entry ids not already in the SQLite cache (diff vs the store) instead of re-fetching the newest window. Also consume GET /updated_entries.json (with a since cursor / validators in meta) to re-hydrate articles whose content changed after caching, clearing them via the updated_entries delete endpoint.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 On a changed unread set, only entry ids not already cached are hydrated (diff vs the store); cached entries are reused
- [x] #2 updated_entries.json is consumed to re-hydrate changed content and then cleared
- [x] #3 Covered by mockito tests
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. feedbin.rs: refactor the conditional id-list fetch into a shared helper; add updated_entry_ids_conditional() (GET /updated_entries.json, ETag/Last-Modified) reusing it. Refactor write_unread into a shared write_entry_ids(method, endpoint, key, ids); add delete_updated_entries() (DELETE /updated_entries.json, {"updated_entries":[...]}, batched 1000). Mockito tests for parse/304/delete-body.
2. store.rs: add refresh_entries(&[Entry]) that UPDATEs json+published+feed_id WHERE id=? (preserves unread/starred) for content refresh. Unit test that it preserves read/star state.
3. tui.rs AC#1: load() gains reuse: &HashMap<i64,Entry>; hydrate only newest-window ids absent from reuse, assemble the full window from reuse+fetched; skip feed_titles fetch when nothing missing (apply keeps existing titles when empty). Loaded shape unchanged. reuse built from app.entries at each spawn_fetch site.
4. tui.rs AC#2: refresh_updated(client, validators, reuse) fetches updated ids (conditional), re-hydrates reuse-intersection, DELETEs the whole batch to drain, returns refreshed bodies + validators. spawn_fetch runs it after the unread reconcile in the same blocking task; generalize Msg::Validators to carry an endpoint; add Msg::UpdatedEntries(Vec<Entry>) -> swap bodies in place, bump image_generation, invalidate reader_cache, refill_image_queue. persist via store.refresh_entries.
5. Wire stored_validators(store, endpoint) + reuse pool into the 3 spawn_fetch call sites (initial, auto-refresh, reload).
6. Tests: mockito for incremental hydration (only new id fetched; titles skipped when all reused) + updated refresh (only cached intersection re-hydrated, whole batch deleted); store refresh_entries preserves state; apply UpdatedEntries swaps body. Run suite 5x.
7. Update docs/architecture.md + docs/data-model.md in the same commit.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented. AC#1: load() takes a reuse pool (in-memory entries) and hydrates only newest-window ids absent from it, assembling the window from fetched+reused; skips the subscriptions.json feed-titles fetch when nothing is missing. AC#2: refresh_updated() conditionally fetches updated_entries.json, re-hydrates only the ids we hold, DELETEs the whole batch to drain, and emits Msg::UpdatedEntries (in-place body swap) — runs even on a 304-unread. Both share one blocking task in spawn_fetch. New feedbin methods updated_entry_ids_conditional + delete_updated_entries (shared conditional_ids/write_entry_ids helpers); store.refresh_entries preserves unread/starred; validators generalized per endpoint (unread/updated). Covered by 3 feedbin + 1 store + 5 tui mockito/unit tests; suite 145 pass, 10x stable, fmt+clippy clean.
<!-- SECTION:NOTES:END -->
