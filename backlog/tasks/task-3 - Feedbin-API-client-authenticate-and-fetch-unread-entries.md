---
id: TASK-3
title: 'Feedbin API client: authenticate and fetch unread entries'
status: Done
assignee:
  - '@claude'
created_date: '2026-06-29 00:56'
updated_date: '2026-06-29 14:18'
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
- [x] #1 Client sends HTTP Basic auth over HTTPS to https://api.feedbin.com/v2/ using the stored credentials
- [x] #2 GET /authentication.json validates credentials (200 valid, 401 invalid) with a clear error on failure
- [x] #3 GET /unread_entries.json returns the array of unread entry IDs
- [x] #4 GET /entries.json with an ids batch (max 100) hydrates typed structs (id, title, feed_id, url, published)
- [x] #5 Network, HTTP, and JSON errors are returned as actionable errors (no panics on external input)
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Deps: reqwest (no-default-features; blocking, json, rustls-tls) for an HTTPS Basic-auth client; serde already present; mockito as a dev-dependency for deterministic local-server tests (no network, no creds). 2. feedbin.rs: Client { blocking reqwest client, base_url, email, password }, private with_base_url() for tests, DEFAULT_BASE_URL=https://api.feedbin.com/v2, user-agent roses/0.x. Methods: authenticate() (GET /authentication.json -> Ok on 200, clear error on 401/other), unread_entry_ids() (GET /unread_entries.json -> Vec<i64>), entries(&[i64]) (GET /entries.json?ids=..., chunked <=100 -> Vec<Entry>). Entry: typed serde struct (id, feed_id, title/url/published as Option to tolerate JSON nulls). Central check_status() maps non-2xx to actionable anyhow errors; no unwraps on external input (AC#5). 3. main.rs: build client from stored creds, authenticate(), fetch unread ids, hydrate first <=100, print terse counts (rich rendering deferred to TASK-4); drop the now-used #[allow(dead_code)] on Credentials.password. 4. Tests (mockito): 200 vs 401 auth, unread-id parsing, entries hydration incl. null title/url, and >100 ids -> multiple batched requests. 5. Verify fmt/build/clippy -D warnings/test; user runs cargo run against the live account to confirm AC#1-4 end-to-end.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented src/feedbin.rs: blocking reqwest 0.13 (rustls TLS) client, HTTP Basic auth on every request, DEFAULT_BASE_URL=https://api.feedbin.com/v2, UA roses/<ver>. authenticate() (200 -> Ok, clear error on 401/other), unread_entry_ids() -> Vec<i64>, entries(&[i64]) chunked at the 100-id limit -> Vec<Entry>; Entry tolerates null title/url/published. check_status() turns non-2xx into actionable anyhow errors (no panics on external input). main.rs now authenticates, counts unread, hydrates the first 20 (rich display deferred to TASK-4). Removed the now-used #[allow(dead_code)] on Credentials.password.

Deps: reqwest { blocking, json, query, rustls } (no-default-features); mockito (dev). Gotchas for the next dev: reqwest 0.13 renamed the rustls feature ('rustls', not 'rustls-tls') and gates .query() behind a separate 'query' feature.

Verification: cargo fmt --check clean; cargo clippy --all-targets -- -D warnings clean; cargo test 11/11; suite run 10x back-to-back with ZERO flakes (mockito binds local sockets, so this matters). Six mockito-backed tests cover AC#1 (Basic auth header actually sent), AC#2 (200 ok / 401 -> clear error), AC#3 (id-array parse), AC#4 (hydration + null tolerance + >100-id batching + no-request-on-empty), AC#5 (Option fields + actionable errors).
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Added a minimal blocking Feedbin v2 client (src/feedbin.rs) on reqwest 0.13 + rustls with HTTP Basic auth on every request: authenticate() validates credentials (200/401 with a clear error), unread_entry_ids() returns the unread ID array, and entries() hydrates typed Entry structs in <=100-id batches while tolerating null fields. Non-2xx responses become actionable errors with no panics on external input. main.rs wires verify -> count unread -> hydrate a sample (rich rendering is TASK-4). Verified with cargo fmt, clippy -D warnings, and 11 unit tests (6 new, mockito local-server) run 10x with no flakes. A live cargo run against the real Feedbin account is recommended as end-to-end confirmation and is the natural input to TASK-4.
<!-- SECTION:FINAL_SUMMARY:END -->
