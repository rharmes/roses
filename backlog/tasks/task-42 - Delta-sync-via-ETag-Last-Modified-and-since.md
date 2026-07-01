---
id: TASK-42
title: Delta sync via ETag/Last-Modified and since
status: Done
assignee:
  - '@claude'
created_date: '2026-07-01 14:38'
updated_date: '2026-07-01 19:00'
labels:
  - epic
  - feedbin-api
  - perf
dependencies:
  - TASK-41
references:
  - 'https://github.com/feedbin/feedbin-api/blob/master/content/http-caching.md'
priority: medium
ordinal: 28014
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Turn full polls into cheap deltas. Feedbin sets ETag and Last-Modified on GETs and honors If-None-Match / If-Modified-Since (304 Not Modified), plus a since=ISO8601 parameter on entries/subscriptions/updated_entries. Store per-request validators (in the SQLite cache) and replay them so unchanged data returns 304 and only new/updated entries are fetched. Also consume updated_entries.json to refresh changed content.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 roses stores and replays ETag/Last-Modified (and/or uses since=) so unchanged endpoints return 304 and are not re-downloaded
- [x] #2 The conditional-request logic is covered by mockito tests (200 then 304)
- [x] #3 When the unread set is unchanged, no entries/subscriptions are re-downloaded (the 304 fast-path); a changed set triggers a normal load. Incremental hydration on change + updated_entries refresh are deferred to the TASK-44 follow-up
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Scope: ETag/Last-Modified 304 fast-path (incremental hydration + updated_entries deferred to TASK-44).
1. feedbin.rs: Validators { etag, last_modified } + Conditional<T> { NotModified, Modified { data, validators } }; unread_entry_ids_conditional(validators) sends If-None-Match/If-Modified-Since, returns NotModified on 304 else Modified + captured response validators. Keep unread_entry_ids() for roses list.
2. store.rs: get_validators/set_validators over the meta table (endpoint.etag / endpoint.last_modified) via get_meta/set_meta.
3. tui.rs: LoadOutcome { NotModified, Fresh(Loaded, Validators) }; load(client, validators) does the conditional GET and short-circuits on 304. spawn_fetch takes validators and sends Msg::NotModified (304) or Msg::Loaded(Ok(loaded)) + Msg::Validators(v) (200). Move the initial spawn into run_loop so it can read stored validators; reload reads them too. apply: NotModified settles Loading->Ready and keeps the view; Validators is a no-op (persisted). persist_msg stores validators on Msg::Validators. Loaded struct unchanged (no test churn).
4. Tests: client 200-then-304; store validator round-trip; App NotModified keeps view.
5. Docs: architecture/data-model/persistence updated (validators in meta; Msg variants); mark the persistence.md Future line done.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
feedbin.rs: Validators { etag, last_modified } + Conditional<T> + unread_entry_ids_conditional (sends If-None-Match/If-Modified-Since, returns NotModified on 304 else Modified + captured response validators); kept unread_entry_ids for roses list. store.rs: get/set_validators over the meta table (unread.etag/unread.last_modified) via get_meta/set_meta. tui.rs: LoadOutcome { NotModified, Fresh(Loaded, Validators) }; load(client, validators) conditional + 304 short-circuit; spawn_fetch sends Msg::NotModified (304) or Msg::Loaded+Msg::Validators (200); initial fetch moved into run_loop to read stored validators; reload reads them; apply(NotModified) settles Loading->Ready and keeps view; persist_msg stores validators (Store stays single-threaded). Loaded struct unchanged (no test churn). Tests: client 200-then-304, store validators round-trip, App not_modified_keeps_view. 100 tests, 10x stable, fmt/clippy clean. Follow-up TASK-44 created for incremental hydration + updated_entries.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Delta sync via the ETag/Last-Modified 304 fast-path. Reloads replay the stored unread_entries validators; a 304 short-circuits the entire reload (no subscriptions/entries fetch) and keeps the current view, while a 200 stores fresh validators and loads normally. Validators live in the cache meta table and persist across sessions, so even the first load of a session can 304. Store stays single-threaded (validators read before spawning, written back in persist_msg). Incremental hydration on change + updated_entries refresh deferred to TASK-44.
<!-- SECTION:FINAL_SUMMARY:END -->
