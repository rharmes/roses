- **Do main-agent work in this checkout, on a branch — never in a worktree, never on `main`.**
  Edit directly in `/Users/ross/Documents/roses/` (a worktree would hide changes from the working copy
  the user is running; subagents may still isolate). **Commit *and* push as you go.** Aim to land a
  single pull request with all the changes and tests for a complete feature or change — and **ask before
  opening the PR.**
- **Credentials never live in the repo.** Keep a defensive `.gitignore` entry for the config filename 
  in case a copy strays in.
- **Very low tolerance for flaky tests.** A test that passes only *sometimes* is a defect — in the
  test or the app — not noise to shrug off. Fix the root cause: wait for the real condition instead
  of a fixed timeout, click-and-verify-with-retry on a flaky control, freeze animations, or surface
  a genuine app race. CI `retries` are a backstop for truly unavoidable timing — **never the fix.**
- **Keep the always-loaded instructions lean.** `CLAUDE.md` should hold the summary, the non-obvious 
  constraints, and the workflow — push deep detail into `docs/` and link to them rather than inlining it.
- **Keep `docs/` current in the same commit.** When architecture or tooling changes significantly,
  update `CLAUDE.md` and the relevant doc in the *same* commit as the change.
- **Explain the tooling, not just the result.** Provide detailed walkthroughs of dev/ops tooling
  and conventions (CI, package manager, shell, deploy) — the reasoning, not only working code.
- **Suggest guardrails when a pattern emerges.** If the same kind of command keeps coming up,
  suggest a permission allow-rule or a small wrapper script to remove the repeated prompt.

## Development

`roses` is a Rust binary crate (edition 2024). The toolchain is pinned in
`rust-toolchain.toml`; rustup puts these commands on your PATH:

- `cargo run` — build and run the app
- `cargo build` (`--release` for optimized) — compile
- `cargo test` — run tests
- `cargo fmt` — format; `cargo clippy` — lint

Source layout under `src/`: `config` (settings + keychain credentials),
`feedbin` (Feedbin API client), `ui` (output — stdout now, ratatui later).
Stack rationale and the build-out plan live in `docs/tui_research.md`.

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
