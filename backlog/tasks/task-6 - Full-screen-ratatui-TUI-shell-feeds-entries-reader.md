---
id: TASK-6
title: Full-screen ratatui TUI shell (feeds / entries / reader)
status: In Progress
assignee:
  - '@claude'
created_date: '2026-06-29 00:56'
updated_date: '2026-06-29 15:20'
labels:
  - rust
  - ui
dependencies:
  - TASK-4
documentation:
  - docs/tui_research.md
priority: medium
ordinal: 6
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Replace the stdout proof-of-concept with the real terminal UI: a full-screen ratatui app (crossterm backend) with a feeds/entries list and a reader pane, driven by async tokio fetches so the UI stays responsive. See docs/tui_research.md section 3.1.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Full-screen ratatui app renders a list/detail layout (entries list + reader pane) on the crossterm backend
- [ ] #2 Entries load from Feedbin asynchronously without blocking input
- [ ] #3 Keyboard navigation moves selection and scrolls the reader; quitting restores the terminal cleanly
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Async model: Option 1 (tokio + spawn_blocking) — keep the blocking client; run fetches on tokio's blocking pool, results to the UI over a tokio mpsc channel.
1. Deps: ratatui (crossterm backend, re-exported) + tokio { rt, sync }. Hand-rolled HTML->text (no ammonia/html2text yet).
2. feedbin.rs: derive Clone on Client; add summary + content (Option<String>) to Entry for the reader.
3. tui.rs: ratatui app on crossterm. ratatui::init()/restore() for alt-screen + raw-mode + panic-safe teardown (AC#3). Sync draw/poll loop (event::poll 100ms) draining a tokio mpsc; initial load via rt.spawn_blocking(unread -> feed_titles -> entries newest N) -> Msg::Loaded(Result). Two-pane layout: List+ListState ('title — feed') | scrollable Paragraph reader (title/feed/date/url + html_to_text(content||summary)) + help/status footer (AC#1). Keys: up/down|k/j select (reset scroll), g/G first/last, PgUp/PgDn|u/d scroll, r reload, q/Esc quit (AC#3). Loading/empty/error states.
4. html_to_text(): strip tags, block tags -> newlines, decode common entities, drop control chars (prevents feed ANSI-escape injection). Pure + unit-tested.
5. main.rs: default launches the TUI (sync login first, then tui::run(client)); keep logout; add 'list' subcommand for the old stdout path (preserves ui::format_unread, useful headless).
6. CLAUDE.md: update source-layout (ui = stdout list, tui = ratatui app), same commit.
7. Tests (deterministic): html_to_text; selection/scroll transitions; ratatui TestBackend buffer snapshot of the Ready layout (AC#1). fmt/clippy -D warnings/test +10x; push -> CI. AC#2/#3 confirmed by a real cargo run.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented src/tui.rs (ratatui 0.30 + crossterm): two-pane list/detail — entries List/ListState + scrollable Paragraph reader + footer key help (AC#1). Async load via tokio spawn_blocking reusing the blocking client; results over an mpsc channel drained each 100ms tick, so input never blocks (AC#2). Keys: up/down|j/k select (reset reader scroll), g/G first/last, PgUp/PgDn|u/d scroll, r reload, q/Esc quit; ratatui::init/restore + panic hook for clean teardown (AC#3). html_to_text() strips tags, maps block tags to newlines, decodes common entities, and drops control chars (blocks feed ANSI-escape injection). feedbin.rs: Client now derives Clone; Entry gained summary+content. main.rs: default launches the TUI, 'list' keeps the stdout fallback, 'logout' unchanged. Deps: ratatui 0.30, tokio {rt, sync}; CLAUDE.md source-layout updated. Verified: fmt, clippy --all-targets -- -D warnings, 24 tests (9 new: TestBackend two-pane layout snapshot, empty 'caught up' state, html_to_text incl. an escape-injection case, selection/scroll transitions, quit/reload keys), stable across 10 runs. AC#1 is test-backed and checked. AC#2 (non-blocking async load) and AC#3 (nav/scroll + clean terminal restore) are implemented and partly unit-tested, but the live feel — responsiveness and terminal restoration — needs a real 'cargo run' against the account.
<!-- SECTION:NOTES:END -->
