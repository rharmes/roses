---
id: TASK-35
title: 'View toggle: unread / starred / all'
status: To Do
assignee: []
created_date: '2026-07-01 14:38'
labels:
  - feature
  - feedbin-api
dependencies:
  - TASK-29
priority: medium
ordinal: 21014
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Add a view toggle switching the loaded set between Unread (today's default), Starred (starred_entries.json), and All recent (entries.json with read/starred/since filters). Show the active view in the UI. Depends on starring (the Starred view needs starred state).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A key cycles the view between Unread, Starred, and All; the active view is indicated in the UI
- [ ] #2 Each view loads the correct entry set from Feedbin and reuses the existing rendering
- [ ] #3 Client fetch paths covered by tests
<!-- AC:END -->
