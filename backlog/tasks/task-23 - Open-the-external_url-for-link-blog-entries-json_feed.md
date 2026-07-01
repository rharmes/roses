---
id: TASK-23
title: Open the external_url for link-blog entries (json_feed)
status: Done
assignee:
  - '@ross'
created_date: '2026-06-30 21:19'
updated_date: '2026-07-01 02:17'
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
- [x] #1 feedbin::Entry captures json_feed.external_url as Option<String> from the extended response (tolerant of absence)
- [x] #2 When an entry has an external_url, the open action ('o') opens that external target instead of the permalink
- [x] #3 The original permalink (entry.url) remains accessible (e.g. a distinct key or shown in the header); entries without external_url behave exactly as today
- [x] #4 Tests cover external_url parsing and that open prefers it; docs/architecture.md (browser launching) and docs/data-model.md (Entry) updated
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Entry gains json_feed: Option<JsonFeed{external_url}>. o opens external_url when present (precedence: enclosure > external_url > url). Reader header shows external_url (underlined, primary) plus a dim 'permalink: <url>' line so the permalink stays accessible; non-link-blog entries show url as today. Tests: parse, o opens external_url, header shows both.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Entry gains json_feed: Option<Box<JsonFeed{external_url}>>. selected_url opens external_url when present (precedence enclosure > external_url > url). Reader header shows external_url underlined (primary link) plus a dim 'permalink: <url>' line so the permalink stays visible/accessible; non-link-blog entries show url as before. Tests: parse, o-opens-external, header shows external + permalink. fmt+clippy clean, 83 tests, stable 10/10.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Link-blog entries (Daring Fireball style) now open their external target: json_feed.external_url is parsed (extended mode), 'o' opens it in preference to the permalink, and the reader header shows the external link as primary while keeping a dim 'permalink: <url>' line so the original stays accessible. Non-link-blog entries are unchanged. Verified: fmt+clippy clean, 83 tests (parse, o-opens-external-url, header shows both), stable 10/10. docs updated.
<!-- SECTION:FINAL_SUMMARY:END -->
