---
id: TASK-5
title: 'CI: format, clippy, and test on GitHub Actions'
status: Done
assignee:
  - '@claude'
created_date: '2026-06-29 00:56'
updated_date: '2026-06-29 14:41'
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
- [x] #1 Workflow runs cargo fmt --check, cargo clippy with warnings denied, and cargo test on push and pull_request
- [x] #2 CI uses the pinned stable toolchain and caches cargo registry/build for speed
- [x] #3 A green run is required before merge
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. .github/workflows/ci.yml: trigger on push + pull_request; one job on ubuntu-latest using actions-rust-lang/setup-rust-toolchain@v1 (reads rust-toolchain.toml -> pinned stable + rustfmt/clippy, and enables cargo registry/build caching) (AC#2); steps run cargo fmt --check, cargo clippy --all-targets -- -D warnings, cargo test --locked (AC#1); least-privilege permissions: contents read + concurrency cancel-in-progress. 2. docs/ci.md: walkthrough of the pipeline (why each step, the toolchain pin, caching) and how to require it before merge (AC#3 = branch protection on main). 3. CLAUDE.md Development section: short CI pointer to docs/ci.md (same commit, per the docs-currency rule). 4. Validate the YAML; commit + push; watch the Actions run via gh to confirm green (verifies AC#1/#2 for real). 5. AC#3 (green required before merge) needs a one-time branch-protection setting on main: document it + offer the gh command for the repo admin to apply.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
CI verified GREEN live: run 28379829666 (2m13s cold) and again after bumping actions/checkout v4->v7 (run 28380076131, 21s warm — cache working, Node 20 deprecation cleared). AC#1 (fmt + clippy -D warnings + test on push and pull_request) and AC#2 (pinned stable toolchain + cargo registry/target caching) confirmed by real runs. AC#3 (green required before merge) = branch protection on main, pending the user's decision since it changes repo merge policy.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Added GitHub Actions CI (.github/workflows/ci.yml): on every push and pull_request, one ubuntu job installs the pinned stable toolchain (rustfmt+clippy), caches cargo via Swatinem/rust-cache, and runs cargo fmt --all --check, cargo clippy --all-targets -- -D warnings, and cargo test --locked. Pipeline documented in docs/ci.md with CLAUDE.md pointing to it. Verified by two green runs (cold 2m13s, warm 21s after bumping actions/checkout to v7 to clear the Node 20 deprecation). Branch protection on main now requires the lint-and-test check (strict: true), satisfying 'green required before merge'. All 3 ACs verified live.
<!-- SECTION:FINAL_SUMMARY:END -->
