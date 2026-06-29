---
id: TASK-1
title: Scaffold the roses Rust project
status: To Do
assignee: []
created_date: '2026-06-29 00:54'
labels:
  - poc
  - rust
dependencies: []
documentation:
  - docs/tui_research.md
priority: high
ordinal: 1
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Create the foundational Rust binary crate so all later work has a place to live. Initialize a 'roses' cargo binary crate (2024 edition) with a sensible starter module layout (config, feedbin client, ui) and ignore build artifacts. This is the prerequisite for the proof-of-concept.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A 'roses' binary crate exists using edition 2024; 'cargo build' and 'cargo run' succeed on the stable toolchain
- [ ] #2 target/ is gitignored and the toolchain is pinned (rust-toolchain.toml = stable)
- [ ] #3 Starter module layout is in place (e.g. config, feedbin, ui modules or files) with placeholder content
- [ ] #4 CLAUDE.md/docs note how to build and run the project
<!-- AC:END -->
