---
id: TASK-5
title: 'CI: format, clippy, and test on GitHub Actions'
status: To Do
assignee: []
created_date: '2026-06-29 00:56'
labels:
  - rust
  - ci
dependencies:
  - TASK-1
priority: medium
ordinal: 5
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Add a GitHub Actions workflow that keeps the codebase healthy from the start: formatting, linting, and tests run on every push and PR. Supports the project rule of very low tolerance for flaky tests.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Workflow runs cargo fmt --check, cargo clippy with warnings denied, and cargo test on push and pull_request
- [ ] #2 CI uses the pinned stable toolchain and caches cargo registry/build for speed
- [ ] #3 A green run is required before merge
<!-- AC:END -->
