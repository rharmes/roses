---
id: TASK-34
title: Full-text search via Feedbin saved searches
status: To Do
assignee: []
created_date: '2026-07-01 14:38'
labels:
  - feature
  - feedbin-api
dependencies: []
references:
  - 'https://github.com/feedbin/feedbin-api/blob/master/content/saved-searches.md'
priority: medium
ordinal: 20014
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Feedbin has no ad-hoc entries search param; search is exposed through saved_searches.json (GET list of id/name/query; GET /saved_searches/{id}.json returns matching entry ids, with include_entries=true and page for objects). Add the ability to list and run a saved search and show its results in the articles/reader panes. Creating a saved search from a typed query via POST is a possible follow-on.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 roses can list saved searches and run one, displaying the matching entries
- [ ] #2 Results integrate with the existing reader, mark, and star flows
- [ ] #3 Client methods covered by mockito tests (list and run); docs updated
<!-- AC:END -->
