---
id: TASK-26
title: Add request timeouts to the Feedbin API client
status: Done
assignee:
  - '@claude'
created_date: '2026-07-01 14:37'
updated_date: '2026-07-01 15:05'
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
- [x] #1 The reqwest client built in feedbin.rs sets both a connect timeout and a total request/read timeout
- [x] #2 An unresponsive endpoint fails with a clear timeout error instead of hanging (covered by a mockito test that delays the response beyond the timeout)
- [x] #3 Existing client tests still pass unchanged
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Add CONNECT_TIMEOUT (10s) and REQUEST_TIMEOUT (30s) consts in feedbin.rs. 2. Set .connect_timeout()+.timeout() on the reqwest builder in with_base_url. 3. Add a test-only constructor taking a short timeout; test against a local TcpListener that accepts but never responds, asserting a timeout error (~250ms, deterministic, non-flaky). 4. fmt/clippy/test.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
feedbin.rs: added CONNECT_TIMEOUT (10s) and REQUEST_TIMEOUT (30s); with_base_url now delegates to with_base_url_and_timeout, which sets .connect_timeout()/.timeout() on the builder. AC#2 test uses a non-responding local TcpListener + a 250ms client timeout (mockito has no response-delay API); asserts the request errors in <750ms with a timeout in the error chain. Verified 10x for flakiness (10/10 pass). All 13 client tests green, clippy clean.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Added 10s connect + 30s overall request timeouts to the Feedbin client (via a with_base_url_and_timeout seam); a hung/half-open connection now errors instead of wedging a spawn_blocking thread. Verified by a deterministic timeout test against a non-responding local socket (10/10 stable) plus the existing client tests.
<!-- SECTION:FINAL_SUMMARY:END -->
