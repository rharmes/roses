---
id: TASK-41
title: Local persistence and offline cache (SQLite)
status: To Do
assignee: []
created_date: '2026-07-01 14:38'
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
- [ ] #1 Feeds/entries and read plus star state persist across runs in a local database under the XDG data dir
- [ ] #2 On launch roses renders from cache immediately, then reconciles with Feedbin in the background
- [ ] #3 Read/unread and starred writes update both the cache and Feedbin, staying consistent on failure
- [ ] #4 Schema and sync strategy documented in docs/ (architecture + a decision record); tests cover the store and merge logic
<!-- AC:END -->
