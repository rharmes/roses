---
id: TASK-40
title: See all unread beyond the newest-50 cap (pagination)
status: To Do
assignee: []
created_date: '2026-07-01 14:38'
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
- [ ] #1 The user can reach unread entries beyond the newest 50 (load-more or lazy paging)
- [ ] #2 roses parses the Links header (rel=next) and X-Feedbin-Record-Count and stops at the last page
- [ ] #3 Unread counts and selection remain correct as pages are appended
- [ ] #4 Client pagination covered by mockito tests (multi-page with Links headers)
<!-- AC:END -->
