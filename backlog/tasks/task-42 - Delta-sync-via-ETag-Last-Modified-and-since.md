---
id: TASK-42
title: Delta sync via ETag/Last-Modified and since
status: To Do
assignee: []
created_date: '2026-07-01 14:38'
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
- [ ] #1 roses stores and replays ETag/Last-Modified (and/or uses since=) so unchanged endpoints return 304 and are not re-downloaded
- [ ] #2 Only new/updated entries are fetched on a refresh
- [ ] #3 The conditional-request logic is covered by mockito tests (200 then 304)
<!-- AC:END -->
