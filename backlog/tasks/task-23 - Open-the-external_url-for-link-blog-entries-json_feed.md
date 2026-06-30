---
id: TASK-23
title: Open the external_url for link-blog entries (json_feed)
status: To Do
assignee:
  - '@ross'
created_date: '2026-06-30 21:19'
labels:
  - rust
  - ui
dependencies: []
documentation:
  - docs/architecture.md
  - docs/data-model.md
priority: low
ordinal: 9014
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Link blogs (e.g. Daring Fireball) point the headline at an external article while the entry's own url is the permalink. Feedbin exposes this in extended mode under json_feed, which includes external_url (the linked-to target). When present, 'o' should open the external_url (the thing being linked), not just the permalink.

Requires mode=extended on the entries request (add the query param if not already present) and capturing json_feed.external_url on feedbin::Entry. Today open_selected opens entry.url; for link-blog entries that should prefer the external target, while the permalink stays reachable.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 feedbin::Entry captures json_feed.external_url as Option<String> from the extended response (tolerant of absence)
- [ ] #2 When an entry has an external_url, the open action ('o') opens that external target instead of the permalink
- [ ] #3 The original permalink (entry.url) remains accessible (e.g. a distinct key or shown in the header); entries without external_url behave exactly as today
- [ ] #4 Tests cover external_url parsing and that open prefers it; docs/architecture.md (browser launching) and docs/data-model.md (Entry) updated
<!-- AC:END -->
