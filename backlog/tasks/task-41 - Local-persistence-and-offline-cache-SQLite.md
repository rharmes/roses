---
id: TASK-41
title: Local persistence and offline cache (SQLite)
status: Done
assignee:
  - '@claude'
created_date: '2026-07-01 14:38'
updated_date: '2026-07-01 17:14'
labels:
  - epic
  - architecture
dependencies: []
priority: high
ordinal: 27014
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Introduce a local store (SQLite) for feeds, entries, and read/star state so roses can start instantly from cache, read offline, and serve as the foundation for delta-sync and large-collection pagination. Define a schema and a sync/merge strategy that keeps Feedbin as the source of truth for read/unread. Architectural epic; expect a design decision record.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 On launch roses renders from cache immediately, then reconciles with Feedbin in the background
- [x] #2 Schema and sync strategy documented in docs/ (architecture + a decision record); tests cover the store and merge logic
- [x] #3 Feeds/entries and read/unread state persist across runs in a SQLite DB under the XDG data dir; the schema includes a starred column so TASK-29 slots in without migration
- [x] #4 Mark-read/undo writes update both the cache and Feedbin, staying consistent on failure (mirror to cache on success, rollback on failure); starred write-through lands with TASK-29
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Decisions: offline-first reads; rusqlite+bundled SQLite; forward-compatible schema (starred column, read/unread wired now).
1. Deps: rusqlite (bundled) + serde_json; derive Serialize on Entry + nested types.
2. src/store.rs: blocking SQLite Store at XDG data dir; schema v1 (meta, feeds, entries(id,feed_id,published,unread,starred,json)); open/migrate; load_unread(limit) returning a Loaded for instant paint; replace_snapshot(Loaded) upserts + marks reads (unread = id in the fetched unread set); set_unread(id,bool) write-through; upsert_entries for load-more. Unit tests via in-memory DB.
3. Offline-first startup: connect() drops the pre-TUI authenticate() (defer to background); run_loop opens the Store, seeds App from cache (instant paint), then spawn_fetch reconciles. Store lives in run_loop; cache writes happen in the main-thread message drain on Loaded/Write/LoadedMore Ok. App change: apply(Loaded Err) keeps the cached view + a notice when entries are present, Failed only when empty.
4. Docs: docs/persistence.md decision record (schema + sync + rusqlite/musl rationale); update architecture.md, data-model.md, release.md (new C-compiled dep).
5. Tests: store round-trip, replace_snapshot marks reads, write-through, App offline fallback. Verify host build; musl release verified on next tag.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Added src/store.rs (rusqlite bundled) at XDG data dir: schema v1 (meta, feeds, entries(id,feed_id,published,unread,starred,json blob)); load_unread/replace_snapshot/upsert_entries/set_unread. Entry now derives Serialize (+serde_json). Offline-first: connect() drops the pre-TUI authenticate() (retained as a method, off the startup path); run_loop opens the Store, seeds App from cache for instant paint, then the background fetch reconciles via persist_msg (main-thread store writes only). apply(Loaded Err) keeps the cached view + notice when entries present, Failed only when empty. starred column present but wired by TASK-29 (read/unread only now). Docs: new docs/persistence.md decision record + architecture/data-model/release/CLAUDE updates; release.md flags the new C-compiled dep (musl re-verify on next tag). 6 store/offline tests; 97 total, 10x stable, fmt/clippy clean.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
SQLite offline cache (src/store.rs, rusqlite bundled) at the XDG data dir persists feeds/entries + read state; the TUI is offline-first (paints from cache on launch, reconciles in the background, keeps the cached view + a notice when a refresh fails). Cache writes are main-thread only (persist_msg). Schema includes a starred column for TASK-29; write-through mirrors mark-read/undo. Feedbin stays the source of truth for read state. Documented in docs/persistence.md; the new C-compiled dep is flagged for musl re-verification.
<!-- SECTION:FINAL_SUMMARY:END -->
