# roses — a TUI RSS reader, backed by Feedbin

The global `~/.claude/CLAUDE.md` defaults apply (branch flow, no subagents, flaky-test
rule, credentials). This file holds what's specific to this repo. Tasks live in
Backlog.md (see the block at the bottom), not GitHub Issues.

## Development

`roses` is a Rust binary crate (edition 2024). The toolchain is pinned in
`rust-toolchain.toml`; rustup puts these commands on your PATH:

- `cargo run` — build and run the app
- `cargo build` (`--release` for optimized) — compile
- `cargo test` — run tests
- `cargo fmt` — format; `cargo clippy` — lint

CI mirrors these: every push and PR must pass `cargo fmt --all --check`,
`cargo clippy --all-targets -- -D warnings`, and `cargo test --locked`. Run
those three before pushing — see `docs/ci.md` for the pipeline walkthrough.

**Releases** are automated from a git tag (`vX.Y.Z`): [cargo-dist] builds the static
musl Linux + macOS binaries and the Homebrew formula, and a separate workflow
publishes to crates.io. The release config lives in `dist-workspace.toml` (never
hand-edit `.github/workflows/release.yml` — regenerate with `dist generate`). Full
process + one-time secret setup: `docs/release.md`.

[cargo-dist]: https://github.com/axodotdev/cargo-dist

Source layout under `src/`: `main` (CLI dispatch), `config` (settings +
keychain credentials), `feedbin` (Feedbin API client), `ui` (plain-stdout
list — `roses list`), `tui` (full-screen ratatui app — the default `roses`),
`browser` (open article URLs), `images` (half-block image rendering),
`store` (SQLite offline cache — see `docs/persistence.md`), `text`
(control-char stripping for feed-derived display fields), `theme` (the rose
color palette + gradient `lerp`).

Architecture and data model are documented in @docs/architecture.md and
@docs/data-model.md (imported into context) — keep them current in the *same
commit* when the architecture or types change. Stack rationale and the
build-out plan live in `docs/tui_research.md`.

<!-- BACKLOG.MD GUIDELINES START -->
<CRITICAL_INSTRUCTION>

## Backlog.md Workflow

This project uses Backlog.md for task and project management.

**For every user request in this project, run `backlog instructions overview` before answering or taking action.**

Use the overview to decide whether to search, read, create, or update Backlog tasks.

Use the detailed guides when needed:
- `backlog instructions task-creation` for creating or splitting tasks
- `backlog instructions task-execution` for planning and implementation workflow
- `backlog instructions task-finalization` for completion and handoff

Use `backlog <command> --help` before running unfamiliar commands. Help shows options, fields, and examples.

Do not edit Backlog task, draft, document, decision, or milestone markdown files directly. Use the `backlog` CLI so metadata, relationships, and history stay consistent.

</CRITICAL_INSTRUCTION>
<!-- BACKLOG.MD GUIDELINES END -->
