---
id: TASK-4
title: Display fetched Feedbin entries as basic stdout output
status: To Do
assignee: []
created_date: '2026-06-29 00:56'
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
- [ ] #1 Running roses after login prints a list of unread entry titles, each with its feed name
- [ ] #2 Output is readable plain text on stdout (no TUI yet)
- [ ] #3 The empty case (no unread entries) is handled gracefully with a friendly message
- [ ] #4 Verified end-to-end against a live Feedbin account
<!-- AC:END -->
