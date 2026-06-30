# roses — architecture

`roses` is a terminal RSS reader backed by [Feedbin](https://github.com/feedbin/feedbin-api).
It's a single Rust binary crate (edition 2024, pinned stable toolchain in `rust-toolchain.toml`).
This document describes how the code is organized and how data and control flow through it. Type
definitions and the Feedbin data shapes are in [`data-model.md`](data-model.md); stack rationale and
the build-out plan are in [`tui_research.md`](tui_research.md); CI is in [`ci.md`](ci.md).

## Modules (`src/`)

| Module | Responsibility |
| --- | --- |
| `main` | CLI entry + command dispatch; the `connect()` login flow; `roses list` stdout path. |
| `config` | Non-secret settings (TOML under XDG) + Feedbin password in the OS keychain. |
| `feedbin` | Blocking Feedbin v2 API client (`Client`) + the `Entry` type. |
| `ui` | Pure `format_unread()` — the plain-stdout entry list for `roses list`. |
| `tui` | The full-screen ratatui app: state (`App`), event loop, rendering, async orchestration. |
| `images` | Fetch + render images to Unicode half-block art (`▀`). |
| `browser` | Resolve and launch the user's browser for an article URL. |
| `theme` | The rose color palette (truecolor `Rgb` consts + a `lerp` for the gradient). |

There is no `lib.rs`; everything is a private module of the binary except items marked `pub` for
cross-module use within the crate.

## CLI dispatch (`main.rs`)

`main()` matches `args().nth(1)`:

- *(none)* → `run_tui()` → `connect()` then `tui::run(client)`.
- `list` → `run_list()` → `connect()`, fetch newest ≤20 unread, print `ui::format_unread()`. Headless
  fallback (handy over SSH / for piping).
- `logout` → `config::logout()`.
- `preview` → `tui::run_preview()` → renders the "all caught up" rose (the `Ready` + empty state) with
  **no login or network**, so the empty state can be eyeballed without marking everything read; quits on
  `q`/`Esc`.
- anything else → an error with usage.

`connect()` is the shared login path: `config::load_credentials()` (or prompt via `config::login()`
on first run) → `feedbin::Client::new(&creds)` → `client.authenticate()` (validates before the TUI
takes over the screen, so a bad-password 401 surfaces as plain text).

## Concurrency model — sync UI loop + tokio blocking pool

This is the central design decision (TASK-6, "tokio + `spawn_blocking`"). The blocking `feedbin::Client`
is reused unchanged; the UI stays responsive by offloading every network/decode operation to tokio's
**blocking thread pool** and reporting results back over a channel.

- `tui::run` builds a **current-thread** tokio runtime and keeps a `Handle`. (Features: `tokio = {rt, sync}`.)
- The UI runs a **synchronous immediate-mode loop** (`run_loop`), not an async task. Each iteration:
  1. Drain the `mpsc::UnboundedReceiver<Msg>` (`rx.try_recv()`), applying each `Msg` to `App`; decrement
     the in-flight image counter on `Msg::Image`.
  2. `terminal.draw(|f| app.draw(f))` — redraw the whole UI from `App` state.
  3. If the selected article changed, `app.prioritize_selected_images()` (bump its queued images).
  4. Drain the image pre-fetch queue up to `MAX_IMAGE_FETCHES` (6) concurrent.
  5. `event::poll(TICK)` (100 ms) for input; on a key press, `app.handle_key()` returns an `Action`
     the loop executes (spawning background work).
- Background work runs via `handle.spawn_blocking(...)`, which executes the **blocking** client/decoder
  on a pool thread and `tx.send(Msg::…)`s the result. The closures own `Client` clones (cheap — the
  inner `reqwest::blocking::Client` is `Arc`-backed) and a `tx` clone.

So `App` is only ever touched on the main thread; background threads communicate purely by sending
`Msg`s. The 100 ms poll bounds how quickly a finished fetch appears.

### Background tasks (`spawn_*`)

- `spawn_fetch` → `load(client)`: `unread_entry_ids()` → sort desc, take newest `DISPLAY_LIMIT` (50) →
  `feed_titles()` → `entries(&sample)` → sort by `published` desc → `Msg::Loaded`.
- `spawn_write` → `client.mark_read`/`mark_unread` for one entry → `Msg::Write { op, entry, index, result }`.
- `spawn_image` → `images::fetch_and_render(url, max_cols)` → `Msg::Image { url, result }`.

### Actions (keypress → effect)

`handle_key` mutates selection/scroll directly and returns an `Action` for effects the loop must drive:
`Reload` (refetch), `MarkRead`, `Undo`, `OpenInBrowser`, or `None`. Mark/undo go through the optimistic
flow below; open-in-browser is handled inline by `open_selected`.

## Network layer (`feedbin.rs`)

`Client` wraps a `reqwest::blocking::Client` (reqwest 0.13, `default-features = false`, features
`blocking, json, query, rustls`). **HTTP Basic auth** (email + password) is attached to every request —
Feedbin has no tokens. Base URL `https://api.feedbin.com/v2`; a private `with_base_url` constructor lets
tests point at a mockito server.

| Method | Endpoint | Notes |
| --- | --- | --- |
| `authenticate()` | `GET /authentication.json` | 200 ⇒ ok; 401 ⇒ clear error. |
| `unread_entry_ids()` | `GET /unread_entries.json` | `Vec<i64>` — source of truth for unread state. |
| `entries(&[i64])` | `GET /entries.json?ids=…` | Hydrate `Entry`s, batched at 100 ids/request. |
| `feed_titles()` | `GET /subscriptions.json` | `HashMap<feed_id, title>` (null titles dropped). |
| `mark_read(&[i64])` | `DELETE /unread_entries.json` | JSON body, batched at 1000; returns changed ids. |
| `mark_unread(&[i64])` | `POST /unread_entries.json` | The undo for `mark_read`. |

`check_status()` centralizes error mapping: success passes through; 401 → "rejected the stored
credentials… run `roses logout`"; other non-2xx → `HTTP <status>: <body snippet>`. Write bodies are sent
as `{"unread_entries":[…]}` with `Content-Type: application/json; charset=utf-8`.

> **Security:** entry **images** are fetched by `images.rs` with a *separate, unauthenticated* reqwest
> client — the Feedbin Basic-auth credentials are never replayed to third-party image hosts.

## The TUI (`tui.rs`)

Immediate-mode ratatui (0.30, crossterm backend). `ratatui::init()` enters the alternate screen + raw
mode and installs a panic hook that restores the terminal; `ratatui::restore()` runs on exit.

### Layout & focus (TASK-11)

A three-column [Miller-columns] layout over a 1-line footer:

```
┌ Sources (25%) ┬ Articles (35%) ┬ Reader (40%) ┐
│ feeds + counts│ titles of the  │ selected      │
│ (by name)     │ selected source│ article body  │
└───────────────┴────────────────┴───────────────┘
 ↑↓ move · ←→ focus · m read · u undo · o open · r reload · q quit
```

A single **focus cursor** (`Focus::{Sources, Articles, Reader}`) moves between columns. The focused
column's selected row is drawn **reversed** (the cursor); an unfocused column's remembered selection is
**bold**; the focused column gets a bold border (others dim). The reader shows the selected article only
when focus is `Articles` or `Reader` — a focused *source* shows an empty reader.

**Selection is tracked by id** (`selected_source: feed_id`, `selected_article: entry id`), not by index,
so it survives entries being added/removed by mark/undo. `ListState` indices are derived per-frame from
those ids. Display order: **sources by feed name**; **articles oldest-first** (`articles()` reverses the
newest-first `entries`).

All three panes share `column_block()`, which adds a `Padding::horizontal(1)` inset (TASK-12) so content
doesn't sit flush against the border — one cell of horizontal breathing room (no top/bottom inset),
consistent across the columns. Because the padding lives on the shared block, any geometry that needs the
true content rect derives it from `block.inner(area)` (which accounts for border **and** padding) rather
than a hardcoded `area − 2`: the reader's scroll clamp and the `reader_width` used to size pre-fetched
image art both do this, so they stay correct as the padding changes.

**Article titles wrap** (TASK-13). Each article is one multi-line `ListItem`: `wrap_title()` word-wraps
the full title to the pane's current inner width (`block.inner(area).width`, recomputed each draw so a
resize reflows; hard-breaks any word wider than the line; widths via `unicode-width` so wrapped lines
don't overflow). Items render contiguously (no inter-item blank line — removed by request). Because a
whole article is one item, `List` keeps navigation per-article and highlights the entire wrapped item
(it applies `highlight_style` across the full item height) — so `↑`/`↓` still step article-by-article,
never line-by-line.

### Theme & whimsy (TASK-14)

A single **rose accent** (`theme::ROSE`) is applied to *chrome only*, so just the active element draws
the eye: the **focused** pane's border + title (`column_block`), the **selection bar**
(`highlight` = `fg(ROSE).reversed()` — the reverse swaps rose onto the background, and keeps the
`REVERSED` modifier the tests rely on), the **reader title**, and the footer's **keys** — the action
letters and the arrow glyphs (`footer_help()`; the word labels stay dim). Feed names, article titles,
counts, meta, url, and body stay neutral. The palette lives in `theme` (truecolor `Rgb`; non-truecolor terminals downsample, and the
bold/dim/reversed modifiers still carry focus/selection).

When nothing is unread (`Status::Ready` + empty `entries`), `draw()` short-circuits to `draw_caught_up()`
instead of the three columns: a vertically-centered ASCII rose with petals graded light→deep rose
(`theme::lerp`) over a green stem, and an *All caught up.* caption in the default text color. The footer
is always drawn. The art
rows are equal width so `Alignment::Center` keeps the bloom aligned while centering it; if the area is
too small for the art it degrades to just the centered caption. Loading and `Failed` states are unchanged
(their text still lives in the Sources pane).

### Keybindings

| Key(s) | Action |
| --- | --- |
| `↑`/`k`, `↓`/`j` | Move the cursor within the focused column (in Reader, scroll a few lines — `READER_SCROLL_STEP`). |
| `←`/`h`, `→`/`l` | Move focus across columns (preserving each column's cursor). |
| `g`/`Home`, `G`/`End` | First / last in the focused column (or top/bottom of the reader). |
| `PgUp`/`PgDn` | Page the reader (only when the reader is focused). |
| `m` / `u` | Mark the selected article read / undo the last mark. |
| `o` | Open the selected article's URL in the browser. |
| `r` | Reload. |
| `q` / `Esc` | Quit (restores the terminal). |

### Optimistic mark-read + undo (TASK-7, AC #4)

Writes update the UI immediately and roll back on failure so client and server stay consistent:

- `begin_mark_read()` removes the entry from `entries` now, decrements `total_unread`, picks a sensible
  next selection (`reselect_after_removal`), and returns `(entry, index)` for the network write.
  - `Msg::Write{MarkRead, Ok}` → push `Undone{entry, index}` onto `undo_stack`.
  - `Msg::Write{MarkRead, Err}` → `reinsert()` the entry (rollback) + a red footer notice.
- `begin_undo()` pops `undo_stack`, re-inserts the entry optimistically, returns it for `mark_unread`.
  - `Msg::Write{Undo, Err}` → remove again, re-push to `undo_stack` (retryable) + notice.

A fresh `Msg::Loaded` clears the undo stack.

### Reader content pipeline

`reader_text(entry, feed, images)` builds the reader `Text`: a title/feed/url header, then the body from
`content_blocks(html)` → `Vec<Segment>` (`Text` runs and `Image` URLs in document order). Each segment
renders as: text lines, or cached half-block art / `[image loading…]` / `[image unavailable]`.

The HTML→text helpers (shared by text segments): `tag_name`/`is_block_tag` (block tags → line breaks),
`decode_entities`/`decode_entity` (named + numeric refs), `sanitize` (**strips control characters** so a
hostile feed can't inject terminal escape sequences; collapses blank lines), and `extract_img_src`
(pulls `src` from `<img>`, ignoring `srcset`).

Reader scroll is clamped to the **wrapped** height via `Paragraph::line_count(inner_width)` (needs
ratatui's `unstable-rendered-line-info` feature) — clamping to the raw line count pinned long
word-wrapped paragraphs at the top (a fixed bug; regression-tested). `inner_width`/`inner_height` come
from `block.inner(area)`, so the clamp stays correct under the pane padding.

A **scrollbar** (TASK-15) rides the reader's right border, shown only when the reader is the *focused*
pane **and** the wrapped content overflows the viewport (`wrapped > inner_height`). It reuses those same
values: `ScrollbarState::new(max_scroll + 1).viewport_content_length(inner_height).position(reader_scroll)`
rendered via `Scrollbar(VerticalRight)` into `area.inner(Margin{vertical:1,…})` (so it sits between the
corners). The `content_length` is the count of scroll **positions** (`max_scroll + 1`), not the line
count — ratatui's thumb only reaches the bottom of the track when `position == content_length − 1`, so
passing `wrapped` left it stopping partway down (fixed; regression-tested).

### Image pre-fetch (TASK-8)

On `Msg::Loaded`, `refill_image_queue()` enumerates every article's image URLs in **on-screen
top-to-bottom order** (sources by name → articles oldest-first), marks each `Loading` in the `images`
cache, and pushes to `image_queue`. The loop drains the queue at most `MAX_IMAGE_FETCHES` (6) at a time
(polite to hosts). `prioritize_selected_images()` bumps the focused article's *still-queued* images to
the front on a selection change, so an explicit jump pulls them forward while sequential reading stays
top-to-bottom. `images::render()` maps a decoded image to `▀` `Line`s (fg = top pixel, bg = bottom
pixel; run-length-coalesced; aspect-corrected; ≤80×40 cells).

> **Approach note:** images render *into the reader text* as half-block `Line`s rather than via the
> `ratatui-image` widget, so they flow inline within the single scrolling reader `Paragraph`. This is a
> deliberate, approved deviation from the research doc's "prefer ratatui-image".

## Browser launching (`browser.rs`, TASK-9)

`resolve(pref, $BROWSER, url) -> Launch{program, args, terminal}` is pure (env passed in, unit-tested).
Precedence: config `browser` template → `$BROWSER` (first colon-separated entry) → platform opener
(`open` on macOS, `xdg-open` elsewhere). Templates are `shlex`-split (quote-safe); a `%s`/`{url}`
placeholder is substituted, else the URL is appended. `run(&launch)` spawns a GUI browser (detached) or
`status()`-waits a terminal one. In the TUI, `open_selected` resolves + runs; for a terminal browser,
`suspend_and_run` leaves the alt-screen + raw mode, runs to completion, then restores and forces a
redraw.

## Config & credentials (`config.rs`)

- Config dir: `$XDG_CONFIG_HOME/roses` if set & non-empty, else `~/.config/roses` (honored on macOS too).
- `config.toml` holds non-secret `Settings`: `email`, `browser`, `browser_terminal`. (`.gitignore`s
  `config.toml` defensively.)
- The **password lives only in the OS keychain** (`keyring`, service `"roses"`, username = email; macOS
  Keychain via `apple-native-keyring-store`). `login`/`logout` merge settings so non-secret prefs (e.g.
  the browser) survive a re-login/logout.

## Testing & CI

- **Deterministic unit tests, no live network.** The Feedbin client is tested against a local `mockito`
  server; the TUI layout/render is tested with ratatui's `TestBackend`; pure functions (XDG resolution,
  HTML→text, browser resolution, half-block rendering, pre-fetch ordering) are tested directly.
- Project rule: **very low tolerance for flaky tests** — fix the root cause, never paper over with CI
  retries. The suite is routinely run 10× to confirm stability (mockito binds real sockets).
- CI (`.github/workflows/ci.yml`, job `lint-and-test`): on push + PR, install the pinned toolchain
  (rustfmt + clippy) with cargo caching, then `cargo fmt --all --check`, `cargo clippy --all-targets --
  -D warnings`, `cargo test --locked`. `main` requires the `lint-and-test` check (branch protection).

## Dependencies (why)

| Crate | Why |
| --- | --- |
| `ratatui` 0.30 (`unstable-rendered-line-info`) | TUI; `line_count` for reader scroll clamping. |
| `tokio` (`rt`, `sync`) | Current-thread runtime + `spawn_blocking` pool + `mpsc` channel. |
| `reqwest` 0.13 (`blocking, json, query, rustls`) | Feedbin HTTP client; rustls for painless static builds. |
| `serde` (derive) + `toml` | Typed Feedbin models + config (de)serialization. |
| `keyring` 4 (`apple-native-keyring-store`) | OS keychain for the password. |
| `rpassword` | Hidden password prompt on first run. |
| `dirs` | Home directory for the XDG fallback. |
| `image` (`png, jpeg, gif, webp`) | Decode entry images for half-block rendering. |
| `shlex` | Quote-safe splitting of the browser command template. |
| `unicode-width` | Display-width-correct wrapping of article titles (`wrap_title`). |
| `anyhow` | Error context throughout. |
| `mockito` (dev) | Local HTTP mock server for client tests. |

> **reqwest 0.13 gotchas:** the rustls feature is `rustls` (not `rustls-tls`), and `.query()` needs the
> separate `query` feature.

## Build-out status

PoC + core features (TASK-1–9, 11) plus the UI polish (TASK-12 padding, TASK-13 title wrap, TASK-14
whimsy/rose, TASK-15 reader scrollbar) are done and merged. **TASK-10** (distribution) wires up
tag-triggered releases — static musl Linux + macOS binaries, a Homebrew tap, and crates.io — via
[cargo-dist] (`dist-workspace.toml` → `.github/workflows/release.yml`) plus a `publish-crates.yml`
workflow; see [`release.md`](release.md). See `backlog/` (managed via the `backlog` CLI — do not edit
task markdown by hand).

[cargo-dist]: https://github.com/axodotdev/cargo-dist
