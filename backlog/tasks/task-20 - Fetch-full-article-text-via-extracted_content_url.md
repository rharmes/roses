---
id: TASK-20
title: Fetch full article text via extracted_content_url
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
priority: medium
ordinal: 6014
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Many feeds syndicate only a truncated summary or partial content. Feedbin returns extracted_content_url on every entry (a Mercury-Parser readability endpoint) that yields the full cleaned article. Surface it as an on-demand 'read full article' action in the reader.

extracted_content_url is a standard field already present in the entries.json response roses fetches (serde currently ignores it). Add extracted_content_url: Option<String> to feedbin::Entry. The URL is a pre-signed Feedbin extraction endpoint that returns JSON (title, author, content as HTML, ...); fetch it on the blocking pool and render its content through the existing reader pipeline (content_blocks). Keep roses' security rule: do not send Feedbin Basic-auth credentials to any non-Feedbin host.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 feedbin::Entry deserializes extracted_content_url as Option<String>
- [ ] #2 A keypress in the reader (e.g. 'e') fetches the extracted full article and renders it in place of the feed-provided body; a second press toggles back to the original
- [ ] #3 The fetch runs in the background (does not block the UI) and shows a loading/failure state; a feed with no extracted_content_url disables the action gracefully
- [ ] #4 Credentials are never sent to a non-Feedbin host (per the project security rule)
- [ ] #5 Tests cover the entry field parsing and the extracted-JSON-to-reader rendering; docs/architecture.md (reader pipeline + network layer) and docs/data-model.md (Entry) updated
<!-- AC:END -->
