---
id: TASK-4
title: Display fetched Feedbin entries as basic stdout output
status: Done
assignee:
  - '@claude'
created_date: '2026-06-29 00:56'
updated_date: '2026-06-29 14:27'
labels:
  - poc
  - rust
dependencies:
  - TASK-3
priority: high
ordinal: 4
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Tie the proof-of-concept together: using the stored credentials and the Feedbin client, print a basic, readable list of recent unread entries to the terminal. This proves the credentials -> API -> output path end to end. The full ratatui TUI is a separate later task.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Running roses after login prints a list of unread entry titles, each with its feed name
- [x] #2 Output is readable plain text on stdout (no TUI yet)
- [x] #3 The empty case (no unread entries) is handled gracefully with a friendly message
- [x] #4 Verified end-to-end against a live Feedbin account
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. feedbin.rs: add feed_titles() -> HashMap<feed_id, String> via GET /v2/subscriptions.json (private Subscription { feed_id, title }); mockito test for parsing. 2. ui.rs: pure format_unread(entries, feed_titles, total_unread) -> String. Empty case -> friendly all-caught-up message (AC#3). Else a header 'Unread entries (showing X of Y):' then per-entry lines (number, title, indented 'feed name . url') with (untitled)/(unknown feed) fallbacks (AC#1, #2). Unit tests for empty + populated. 3. main.rs: authenticate; fetch unread IDs; if empty print the friendly message; else fetch feed_titles, take the newest ~20 (IDs sorted desc), hydrate, sort by published desc, print ui::format_unread. 4. Verify fmt/clippy -D warnings/test + 10x stability. 5. AC#4 is inherently live: the user runs cargo run against their real account to confirm titles+feed names render end-to-end.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented the stdout renderer end to end.
- feedbin.rs: feed_titles() -> HashMap<feed_id, String> via GET /v2/subscriptions.json (private Subscription { feed_id, title }; null-titled feeds dropped) + a mockito test.
- ui.rs: pure format_unread(entries, feed_titles, total_unread) -> String. total==0 -> friendly 'You're all caught up' message (AC#3). Otherwise a 'Unread entries (showing X of Y):' header and per-entry 'N. <title>' plus an indented '<feed> · <url>' line, with (untitled)/(unknown feed) fallbacks and the URL/separator omitted when absent (AC#1, #2). Three unit tests.
- main.rs: authenticate -> unread IDs -> if empty print the friendly message; else fetch feed_titles, take the newest DISPLAY_LIMIT=20 (IDs sorted desc), hydrate, sort by published desc, print. Entry's dead_code allow narrowed to just 'id' (reserved for TASK-7's read/unread sync).

Verification: cargo fmt --check clean; clippy --all-targets -- -D warnings clean; 15 tests pass (3 new ui + 1 new feedbin), stable across 10 back-to-back runs. AC#1-3 are covered by unit tests + design and are checked. AC#4 (live end-to-end) is the user's run: 'cargo run' against the real Feedbin account should print their newest unread entries with feed names.

AC#4 finalized on the user's explicit instruction ('mark it done now') without a recorded live run in this session. The credentials -> API -> render path is exercised by the mockito-backed client tests plus the ui tests; a live 'cargo run' remains available as a spot check whenever the user wants it.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Rendered unread Feedbin entries as readable plain stdout, completing the PoC's credentials -> API -> output path. Added feedbin::feed_titles() (subscriptions.json -> feed_id->name map) and a pure, unit-tested ui::format_unread() that prints a 'showing X of Y' header with per-entry 'title' / 'feed · url' lines and a friendly all-caught-up message for the empty case. main.rs authenticates, fetches unread IDs, hydrates the newest 20, and prints newest-first. Verified by cargo fmt, clippy -D warnings, and 15 unit tests (4 new) stable across 10 runs; AC#1-3 unit-tested. AC#4 (live account) finalized on the user's go-ahead. Committed on rust-poc (358cfd7).
<!-- SECTION:FINAL_SUMMARY:END -->
