---
id: TASK-29
title: Star and unstar entries with a Starred view
status: To Do
assignee: []
created_date: '2026-07-01 14:37'
labels:
  - feature
  - feedbin-api
dependencies: []
references:
  - >-
    https://github.com/feedbin/feedbin-api/blob/master/content/starred-entries.md
priority: high
ordinal: 15014
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Add starring backed by Feedbin starred_entries.json (GET list of starred ids; POST to star, DELETE to unstar; id-array bodies batched at 1000, the same shape as the existing unread writes). Bind a key (s) to toggle the selected entry, show a star marker in the articles list, and provide a way to view starred entries. Fits the existing spawn_write / Msg::Write optimistic flow.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Pressing the star key toggles the selected entry starred state optimistically and syncs to Feedbin, rolling back on failure (mirroring mark-read undo)
- [ ] #2 Starred entries are visually marked in the articles list
- [ ] #3 There is a way to view starred entries; client methods covered by mockito tests asserting method, path, and JSON body
- [ ] #4 docs/architecture.md and docs/data-model.md updated in the same commit
<!-- AC:END -->
