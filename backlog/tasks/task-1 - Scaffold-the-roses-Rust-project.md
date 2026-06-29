---
id: TASK-1
title: Scaffold the roses Rust project
status: Done
assignee:
  - '@claude'
created_date: '2026-06-29 00:54'
updated_date: '2026-06-29 01:19'
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
- [x] #1 A 'roses' binary crate exists using edition 2024; 'cargo build' and 'cargo run' succeed on the stable toolchain
- [x] #2 target/ is gitignored and the toolchain is pinned (rust-toolchain.toml = stable)
- [x] #3 Starter module layout is in place (e.g. config, feedbin, ui modules or files) with placeholder content
- [x] #4 CLAUDE.md/docs note how to build and run the project
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. cargo init a 'roses' binary crate (edition 2024) at the repo root. 2. Add starter modules config/feedbin/ui with doc-comment placeholders pointing at TASK-2/3/4/6. 3. Pin toolchain via rust-toolchain.toml (stable + rustfmt/clippy); gitignore /target. 4. Document build/run in CLAUDE.md + README. 5. Verify cargo fmt/build/run/clippy all succeed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Scaffolded roses as a Rust 2024 binary crate. Added config/feedbin/ui placeholder modules, pinned the toolchain (rust-toolchain.toml: stable + rustfmt/clippy), gitignored /target, and documented build/run in README + CLAUDE.md. Verified on stable 1.96.0: cargo fmt --check clean, cargo build, cargo run (prints scaffold banner), and cargo clippy --all-targets -D warnings all pass.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Initialized roses as a Rust 2024 binary crate. Added placeholder modules config/feedbin/ui, pinned the toolchain (stable) via rust-toolchain.toml, ignored /target, and documented build/run in README + CLAUDE.md. Verified cargo fmt --check, build, run, and clippy --all-targets -D warnings all pass on stable 1.96.0.
<!-- SECTION:FINAL_SUMMARY:END -->
