---
id: TASK-5
title: 'CI: format, clippy, and test on GitHub Actions'
status: In Progress
assignee:
  - '@claude'
created_date: '2026-06-29 00:56'
updated_date: '2026-06-29 14:30'
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

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. .github/workflows/ci.yml: trigger on push + pull_request; one job on ubuntu-latest using actions-rust-lang/setup-rust-toolchain@v1 (reads rust-toolchain.toml -> pinned stable + rustfmt/clippy, and enables cargo registry/build caching) (AC#2); steps run cargo fmt --check, cargo clippy --all-targets -- -D warnings, cargo test --locked (AC#1); least-privilege permissions: contents read + concurrency cancel-in-progress. 2. docs/ci.md: walkthrough of the pipeline (why each step, the toolchain pin, caching) and how to require it before merge (AC#3 = branch protection on main). 3. CLAUDE.md Development section: short CI pointer to docs/ci.md (same commit, per the docs-currency rule). 4. Validate the YAML; commit + push; watch the Actions run via gh to confirm green (verifies AC#1/#2 for real). 5. AC#3 (green required before merge) needs a one-time branch-protection setting on main: document it + offer the gh command for the repo admin to apply.
<!-- SECTION:PLAN:END -->
