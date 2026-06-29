---
id: TASK-7
title: Mark entries read with undo (Feedbin unread_entries sync)
status: To Do
assignee: []
created_date: '2026-06-29 00:56'
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
- [ ] #1 Viewing or selecting an entry marks it read via DELETE /unread_entries.json
- [ ] #2 An undo action restores unread state via POST /unread_entries.json
- [ ] #3 Writes are batched to at most 1000 ids per request and reflected in the UI state
- [ ] #4 Failures roll back local state so client and server stay consistent
<!-- AC:END -->
