---
id: TASK-38
title: OPML import and export
status: To Do
assignee: []
created_date: '2026-07-01 14:38'
labels:
  - feature
  - feedbin-api
dependencies: []
references:
  - 'https://github.com/feedbin/feedbin-api/blob/master/content/imports.md'
priority: low
ordinal: 24014
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Support bulk feed migration. Import via Feedbin POST /imports.json (OPML upload) with GET /imports/{id}.json to poll status. Export by generating OPML from GET /subscriptions.json (Feedbin has no export endpoint). Likely exposed as roses subcommands (e.g. roses import FILE / roses export FILE).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 roses can export current subscriptions to a valid OPML file
- [ ] #2 roses can import an OPML file via the Feedbin imports endpoint and report completion/status
- [ ] #3 Import/export paths covered by tests (mockito for the API, a golden OPML for export)
<!-- AC:END -->
