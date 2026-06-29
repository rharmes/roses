---
id: TASK-10
title: 'Distribution: static binary + Homebrew release pipeline'
status: To Do
assignee: []
created_date: '2026-06-29 00:56'
labels:
  - rust
  - release
dependencies:
  - TASK-4
documentation:
  - docs/tui_research.md
priority: low
ordinal: 10
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Make roses installable as a single static binary. Build musl static Linux targets and macOS binaries, automate releases (e.g. cargo-dist) to GitHub Releases with a generated Homebrew formula, and publish to crates.io. See docs/tui_research.md sections 3.1 and 4.5.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Tagged releases produce static Linux (musl) and macOS binaries for x86_64 and aarch64
- [ ] #2 A Homebrew tap formula installs the binary via brew install
- [ ] #3 The release process is automated from a git tag (e.g. cargo-dist / GitHub Actions)
<!-- AC:END -->
