---
id: TASK-3
title: 'Feedbin API client: authenticate and fetch unread entries'
status: To Do
assignee: []
created_date: '2026-06-29 00:56'
labels:
  - poc
  - rust
dependencies:
  - TASK-2
references:
  - 'https://github.com/feedbin/feedbin-api'
documentation:
  - docs/tui_research.md
priority: high
ordinal: 3
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Implement a minimal Feedbin v2 client using HTTP Basic auth (blocking reqwest + serde for the PoC; async tokio arrives with the TUI). Validate credentials, fetch the unread entry IDs, then hydrate a small batch of entries into typed structs.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Client sends HTTP Basic auth over HTTPS to https://api.feedbin.com/v2/ using the stored credentials
- [ ] #2 GET /authentication.json validates credentials (200 valid, 401 invalid) with a clear error on failure
- [ ] #3 GET /unread_entries.json returns the array of unread entry IDs
- [ ] #4 GET /entries.json with an ids batch (max 100) hydrates typed structs (id, title, feed_id, url, published)
- [ ] #5 Network, HTTP, and JSON errors are returned as actionable errors (no panics on external input)
<!-- AC:END -->
