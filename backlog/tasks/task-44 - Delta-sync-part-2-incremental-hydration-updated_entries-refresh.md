---
id: TASK-44
title: 'Delta sync (part 2): incremental hydration + updated_entries refresh'
status: To Do
assignee: []
created_date: '2026-07-01 18:52'
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
- [ ] #1 On a changed unread set, only entry ids not already cached are hydrated (diff vs the store); cached entries are reused
- [ ] #2 updated_entries.json is consumed to re-hydrate changed content and then cleared
- [ ] #3 Covered by mockito tests
<!-- AC:END -->
