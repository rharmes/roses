---
id: TASK-43
title: 'CI: verify the static-musl release build (bundled SQLite)'
status: Done
assignee:
  - '@claude'
created_date: '2026-07-01 17:39'
updated_date: '2026-07-01 17:53'
labels:
  - ci
  - hardening
dependencies: []
priority: medium
ordinal: 29014
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-41 added rusqlite with bundled SQLite, which compiles SQLite (C) and re-introduces a C-compiled dependency into the static-musl release build. That path was not verified locally. Add a CI job that builds the musl target so a linking regression is caught before a release tag rather than at release time.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 CI builds the x86_64-unknown-linux-musl target with rusqlite bundled and the build succeeds
- [x] #2 The produced roses binary is verified to be statically linked
- [x] #3 The job is a separate/advisory job (not initially a required check), documented in docs/ci.md
- [x] #4 The TASK-41 're-verify on next tag' caveat in docs is resolved now that CI covers it
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Add a musl-build job to .github/workflows/ci.yml: install the x86_64-unknown-linux-musl target + musl-tools, then cargo build --release --locked --target x86_64-unknown-linux-musl with CC_x86_64_unknown_linux_musl=musl-gcc so rusqlite's bundled sqlite3.c compiles for musl; assert the binary is statically linked via file. Separate/advisory job (like linux-keychain), own rust-cache key. Update docs/ci.md; resolve the TASK-41 musl caveat in persistence.md + release.md. aarch64-musl left as a future extension (needs a cross C toolchain).
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Added the musl-build job to .github/workflows/ci.yml (rustup target add x86_64-unknown-linux-musl + musl-tools; cargo build --release --locked --target x86_64-unknown-linux-musl with CC_x86_64_unknown_linux_musl=musl-gcc; assert 'statically linked' via file). Separate/advisory job, own rust-cache key. YAML validated (3 jobs parse). Docs: documented in ci.md; TASK-41 musl caveat resolved in persistence.md + release.md. Can't run musl build on this macOS host, so AC#1/#2 are verified by the job's first CI run on the PR (watching).

CI re-run is GREEN: musl-build pass (3m11s), lint-and-test + linux-keychain pass. Confirms the x86_64-unknown-linux-musl build succeeds (rusqlite/bundled SQLite links) and the binary is static (static-pie). The first run usefully caught that modern musl emits 'static-pie linked' not 'statically linked' — assertion fixed to accept both and fail only on dynamic. ACs #1/#2 verified by CI.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Added a CI musl-build job (x86_64-unknown-linux-musl + musl-tools; cargo build --release --locked with CC=musl-gcc so rusqlite's bundled sqlite3.c compiles; asserts static linking via file). Verifies TASK-41's bundled-SQLite C dependency links under static musl on every push/PR, closing the previously-unverified release path. All CI green. Docs: ci.md documents the job; the TASK-41 musl caveat is resolved in persistence.md + release.md. aarch64-musl left as a follow-up (needs a cross C toolchain).
<!-- SECTION:FINAL_SUMMARY:END -->
