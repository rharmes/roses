---
id: TASK-26
title: Add request timeouts to the Feedbin API client
status: To Do
assignee: []
created_date: '2026-07-01 14:37'
labels:
  - hardening
  - reliability
dependencies: []
priority: high
ordinal: 12014
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The Feedbin HTTP client in src/feedbin.rs builds a reqwest blocking client with a user-agent but no timeout, so a hung or half-open connection blocks a spawn_blocking pool thread indefinitely; repeated reloads leak more stuck threads. The image client (src/images.rs) already sets a 10s timeout; the API client should too. Add a connect timeout and an overall request timeout in Client::with_base_url.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The reqwest client built in feedbin.rs sets both a connect timeout and a total request/read timeout
- [ ] #2 An unresponsive endpoint fails with a clear timeout error instead of hanging (covered by a mockito test that delays the response beyond the timeout)
- [ ] #3 Existing client tests still pass unchanged
<!-- AC:END -->
