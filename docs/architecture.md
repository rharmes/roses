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
| `store` | Blocking SQLite offline cache (feeds/entries/read state) for offline-first startup — see [`persistence.md`](persistence.md). |
| `text` | `strip_control_chars()` — defuse terminal-escape injection in feed-derived display fields. |
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
on first run) → `feedbin::Client::new(&creds)`. It **no longer authenticates up front** (TASK-41): the
TUI is offline-first, so it paints from the local cache and validates lazily via the background load —
a bad-password 401 or an offline box surfaces as an in-app notice, not a pre-TUI error. (`roses list`
still hits the network immediately, so it reports auth errors on its first request. `Client::authenticate()`
is retained as a capability but off the startup path.)

## Concurrency model — sync UI loop + tokio blocking pool

This is the central design decision (TASK-6, "tokio + `spawn_blocking`"). The blocking `feedbin::Client`
is reused unchanged; the UI stays responsive by offloading every network/decode operation to tokio's
**blocking thread pool** and reporting results back over a channel.

- `tui::run` builds a **current-thread** tokio runtime and keeps a `Handle`. (Features: `tokio = {rt, sync}`.)
- Before the loop, `run_loop` opens the **offline cache** (`store::Store`) and seeds `App` from it for an
  instant first paint (TASK-41), then spawns the first background fetch to reconcile it.
- The UI runs a **synchronous immediate-mode loop** (`run_loop`), not an async task. Each iteration:
  1. Drain the `mpsc::UnboundedReceiver<Msg>` (`rx.try_recv()`); for each `Msg`, `persist_msg` writes the
     result through to the cache (main-thread store writes), then it's applied to `App` (decrementing the
     in-flight image counter on `Msg::Image`; clearing the auto-refresh in-flight guard on a completed
     fetch — `Msg::Loaded`/`Msg::NotModified`).
  2. `terminal.draw(|f| app.draw(f))` — redraw the whole UI from `App` state.
  3. If the selected article changed, `app.prioritize_selected_images()` (bump its queued images).
  4. Drain the image pre-fetch queue up to `MAX_IMAGE_FETCHES` (6) concurrent.
  5. If the selection nears the oldest loaded entry and un-hydrated unread ids remain,
     `maybe_begin_load_more()` drains the next batch and `spawn_load_more` hydrates it (TASK-40).
  6. **Auto-refresh (TASK-37):** if a `refresh_interval` is configured and the pure
     `should_auto_refresh(interval, last_fetch.elapsed(), in_flight)` predicate is true, spawn a *silent*
     background `spawn_fetch` (no `Status::Loading`) and reset the timer + in-flight guard. Replaying the
     stored validators means an unchanged unread set 304s to a no-op, so the common tick is invisible.
  7. `event::poll(TICK)` (100 ms) for input; on a key press, `app.handle_key()` returns an `Action`
     the loop executes (spawning background work). A manual `Reload` also resets the auto-refresh timer.
- Background work runs via `handle.spawn_blocking(...)`, which executes the **blocking** client/decoder
  on a pool thread and `tx.send(Msg::…)`s the result. The closures own `Client` clones (cheap — the
  inner `reqwest::blocking::Client` is `Arc`-backed) and a `tx` clone.

So `App` is only ever touched on the main thread; background threads communicate purely by sending
`Msg`s. The 100 ms poll bounds how quickly a finished fetch appears.

### Background tasks (`spawn_*`)

- `spawn_fetch` → `load(client, validators)`: a **conditional** `unread_entry_ids_conditional()` replaying
  the stored `ETag`/`Last-Modified` (TASK-42). A **`304`** → `Msg::NotModified` (keep the current view, no
  further fetch). A **`200`** → sort desc, hydrate newest `DISPLAY_LIMIT` (50) via `feed_titles()` +
  `entries(&sample)` → sort by `published` desc → `Msg::Loaded`, then `Msg::Validators` with the fresh
  validators to persist; the remaining ids ride along as `pending_ids` for lazy hydration (TASK-40). The
  main thread reads the validators from the cache before spawning and writes the new ones back in
  `persist_msg`, so the `Store` stays single-threaded.
- `spawn_load_more` → `entries(&ids)` for the next `LOAD_MORE_BATCH` (100) pending ids → `Msg::LoadedMore`
  (appended then re-sorted by `published`). **Pagination is hydrate-on-demand:** `unread_entries.json`
  already returns the *complete* unread id list, so roses just hydrates more of it as the reader nears the
  end — no Links-header paging needed. A single `in_flight_more` guard prevents overlapping batches and
  restores the ids on failure for retry.
- `spawn_write` → `client.mark_read`/`mark_unread` for one entry → `Msg::Write { op, entry, index, result }`.
- `spawn_image` → `images::fetch_and_render(url, max_cols)` → `Msg::Image { url, result }`.

### Actions (keypress → effect)

`handle_key` mutates selection/scroll directly and returns an `Action` for effects the loop must drive:
`Reload` (refetch), `MarkRead`, `MarkSourceRead`, `MarkWindowRead`, `Undo`, `OpenInBrowser`, or `None`.
All three mark variants and undo go through the optimistic **batch** flow below; open-in-browser is handled
inline by `open_selected`. `handle_key` also owns a one-key **confirmation** intercept: while
`pending_confirm` is set (only `A`/`MarkWindowRead` arms it), the next key answers it — `y`/`Y` proceeds,
anything else cancels — instead of firing its normal binding (TASK-30).

## Network layer (`feedbin.rs`)

`Client` wraps a `reqwest::blocking::Client` (reqwest 0.13, `default-features = false`, features
`blocking, json, query, rustls`). **HTTP Basic auth** (email + password) is attached to every request —
Feedbin has no tokens. Base URL `https://api.feedbin.com/v2`; a private `with_base_url` constructor lets
tests point at a mockito server.

| Method | Endpoint | Notes |
| --- | --- | --- |
| `authenticate()` | `GET /authentication.json` | 200 ⇒ ok; 401 ⇒ clear error. |
| `unread_entry_ids()` | `GET /unread_entries.json` | `Vec<i64>` — source of truth for unread state. |
| `entries(&[i64])` | `GET /entries.json?ids=…&mode=extended` | Hydrate `Entry`s, batched at 100 ids/request; `mode=extended` adds the images/enclosure/json_feed objects. |
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
 ↑↓ move · ←→ focus · o open · m read · u undo · r reload · ? help · q quit
```

A single **focus cursor** (`Focus::{Sources, Articles, Reader}`) moves between columns. The focused
column's selected row is drawn **reversed** (the cursor); an unfocused column's remembered selection is
**bold**; the focused column gets a bold border (others dim). The reader shows the selected article only
when focus is `Articles` or `Reader` — a focused *source* shows an empty reader.

**Selection is tracked by id** (`selected_source: feed_id`, `selected_article: entry id`), not by index,
so it survives entries being added/removed by mark/undo. `ListState` indices are derived per-frame from
those ids. A reload or background auto-refresh is likewise **non-disruptive** (`preserve_or_reselect` on
`Msg::Loaded`, TASK-37): when the selected source and article still exist it keeps the cursor, focus, and
reader scroll untouched; only if the selected article vanished does it fall back to that source's first
article (or a full reselect if the source itself is gone). Display order: **sources by feed name**; **articles oldest-first** (`articles()` reverses the
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

**The accent is user-configurable (TASK-45).** A `highlight_color` in `config.toml` (hex `#rrggbb`/`rrggbb`
or 3-digit `#rgb`, parsed by `theme::parse_hex`) overrides `theme::ROSE`; an unset/invalid value falls back
to rose. `run()` resolves it once and stores it on `App.base_accent`, and every chrome site reads that
instead of the `ROSE` const: `column_block`/`highlight` via `accent()` (which also layers the TASK-46
help-overlay grey mute on top), and the footer/overlay/reader-title via `base_accent` directly. Only the
chrome recolors — the `draw_caught_up()` rose keeps its own light→deep gradient + green stem, so the mascot
stays a rose whatever the accent.

When nothing is unread (`Status::Ready` + empty `entries`), `draw()` short-circuits to `draw_caught_up()`
instead of the three columns: a vertically-centered ASCII rose with petals graded light→deep rose
(`theme::lerp`) over a green stem, and an *All caught up* caption in the default text color. The footer
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
| `m` / `u` | Mark the selected article read / undo the last mark (undo restores a whole bulk batch too — TASK-30). |
| `M` | Mark **every loaded article in the selected source** read (works from any focus; TASK-30). |
| `A` | Mark the **whole loaded window** read, behind a `y`/`n` footer confirmation (TASK-30). |
| `o` | Open the selected entry in the browser — a podcast enclosure, else a link-blog `external_url`, else the article URL. |
| `r` | Reload (preserves your place by id — see the selection note above). |
| `?` | Toggle the keybinding **help overlay** (any key closes it — TASK-32). |
| `q` / `Esc` | Quit (restores the terminal). |

The footer shows only a compact subset of these; the `?` overlay lists them all. Both are rendered from a
single `BINDINGS` table (the source of truth, TASK-32): each entry carries its overlay `group`/`keys`/`desc`
and an optional compact `footer` form, so the two can't drift and a new binding shows up in both by adding
one row. `M`/`A` are flagged overlay-only (the 1-line footer can't fit everything).

### Help overlay (TASK-32)

`?` sets `App.show_help`; `draw()` then floats `draw_help_overlay()` — a `Clear`ed, rose-bordered box
centered over the main area (`centered_rect` via `Flex::Center`), listing every binding grouped under bold
headings (`help_lines()`), sized to its content and clamped to the area. It's **pure chrome**: it reads no
mutable state and sets no flags beyond `show_help`, so background loads, image pre-fetch, and the selection
are untouched while it's open (AC #2). It's modal — while open, `handle_key` treats **any** key as a
dismiss (so `?`/`Esc`/`q` all close it, and `q` closes the overlay rather than quitting) and returns
`Action::None`.

While the overlay is open the background accent **recedes to grey** so the overlay draws the eye (TASK-46):
`App::accent()` returns `theme::MUTED` instead of `theme::ROSE` when `show_help` is set, and the focused
column's border/title (`column_block`) and the selection bar (`highlight`) resolve their color through it.
This is display-only — focus/selection state is unchanged — and it's scoped to those two: the overlay's own
border/title, the footer keys, and the reader title keep their rose (the reader title lives in the memoized
`reader_text`, so muting it would mean keying the reader cache on `show_help` — deliberately out of scope).

### Optimistic mark-read + undo (TASK-7 AC #4, TASK-30)

Writes update the UI immediately and roll back on failure so client and server stay consistent. The single
and **bulk** marks share one **batch** path: a write carries a `Vec<(Entry, usize)>` (a single `m` is a
one-element batch), so one `Msg::Write`/`spawn_write` and one undo entry serve all cases, and the client
sends the whole batch in one request (it batches at its 1,000-id limit internally).

- `begin_mark_read()` removes the selected entry now, decrements `total_unread`, picks a sensible next
  selection (`reselect_after_removal`), and returns a one-element batch for the network write.
- `begin_mark_source_read()` (`M`) / `begin_mark_window_read()` (`A`) remove **every loaded entry** of the
  selected source / of the whole window via the shared `remove_batch` (remove back-to-front so indices stay
  valid, then restore ascending-index order for a clean undo). Scope is the **loaded window only** —
  un-hydrated `pending_ids` stay unread and the next batch auto-hydrates as usual (`near_tail`).
  - `Msg::Write{MarkRead, Ok}` → push one `Undone{batch}` onto `undo_stack`.
  - `Msg::Write{MarkRead, Err}` → `reinsert_batch()` the whole batch (rollback) + a red footer notice.
- `begin_undo()` (`u`) pops one `Undone` and `reinsert_batch()`es the whole batch optimistically at its
  original indices, returning it for `mark_unread` — so **one undo reverses a bulk mark in a single step**.
  - `Msg::Write{Undo, Err}` → remove the batch again, `preserve_or_reselect`, re-push (retryable) + notice.

The whole-window mark (`A`) is gated by a one-key `y`/`n` **confirmation** (`pending_confirm`); the source
mark (`M`) is instant (the source visibly disappears, and `u` undoes it).

A `Msg::Loaded` **preserves** the undo stack across a reload or auto-refresh (a silent background refresh
must not wipe a recent mark-read's undo — TASK-37); it drops any batch with an entry the fresh set re-added,
so a later undo can't duplicate a now-present row.

### Reader content pipeline

`reader_text(entry, images, max_width)` builds the reader `Text`: a header (title; then a dim meta line
of **author · published-date**; then the url), then the body from `content_blocks(html)` → `Vec<Segment>`
(`Text` runs and `Image` URLs in document order). Each segment renders as: text lines, or cached
half-block art / `[image loading… <url>]` / `[image unavailable: <url>]`.

Half-block art is rendered once at the reader width **when it was fetched** and cached by URL. If the
terminal later narrows, that stale art would be wider than the reader's `Wrap` width and wrap into a full
row + a short fragment — the "half-height rows" artifact. So `reader_text` **clips each art line to
`max_width`** (the reader's current inner width, passed from `draw_reader`) via `clip_line_to_width`: a
no-op when the art already fits, a graceful right-crop when it doesn't (until a reload re-renders it at
the new width). Text lines are *not* clipped — they still wrap. Both inline images and the extended-mode
lead image go through the shared `push_image()` helper.

**Extended-mode header & links (TASK-21/22/23, `mode=extended`).** When the entry has an `enclosure`, the
header adds a dim `Audio · 47:03` line (`podcast_indicator()` — media kind + `format_duration()`, which
turns bare seconds into `H:MM:SS`/`M:SS`). The link line prefers a link-blog's `json_feed.external_url`
(underlined, the target `o` opens) and keeps the permalink on a dim `permalink: <url>` line so it stays
visible (TASK-23); otherwise it's just the `url`. A **lead image** (`images.size_1.cdn_url`) renders as a
hero at the top of the body **only when the body has no inline `<img>`** — so image-rich articles are
unchanged and metadata-only feeds still get a picture (TASK-21); `article_image_urls()` applies the same
rule so the pre-fetch and the "N of M" count stay in sync. The `o` open target has precedence
**enclosure → external_url → url** (`selected_url()`), so `o` plays a podcast, follows a link-blog out, or
opens the permalink.

The meta line **omits the feed/blog name** — it's already the highlighted source in the left column, so
the header shows the entry's `author` instead when Feedbin provides one (TASK-18). The raw ISO-8601
`published` value is humanized by `format_published()` to e.g. `Sunday, June 15, 2026 at 6:00 AM`
(`chrono`, host-local timezone; TASK-17); a missing or unparseable date is dropped, and if neither
author nor date is present the meta line is omitted entirely. `format_published_in(raw, tz)` is the
timezone-agnostic core, split out so tests pin a fixed offset rather than depend on the host clock.
(Sorting still uses the raw `published` string compare — see `load()`.)

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

**Loading indicator (TASK-19).** `refill_image_queue` also records every distinct image URL of the load
in `image_urls`; `image_progress()` counts how many have resolved (`Ready`/`Failed`) versus the total, or
`None` when there are no images or all are done. While some remain, `draw()` shows a right-aligned footer
indicator — a braille spinner + `Loading N of M images` (`loading_indicator()`, a pure function so tests
freeze the frame) — reserving its columns via a `Layout` split so it never overlaps the help. The spinner
frame (`spinner_tick`) is advanced once per `run_loop` iteration, which ticks ~every 100 ms regardless of
input, so it animates on its own.

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
- `config.toml` holds non-secret `Settings`: `email`, `browser`, `browser_terminal`,
  `refresh_interval_secs` (auto-refresh cadence; `load_refresh_interval()` maps it to an `Option<Duration>`,
  disabling on unset/zero and clamping sub-60 s up to a politeness floor — TASK-37), and `highlight_color`
  (accent override; `load_highlight_color()` returns the raw hex string, resolved to a `Color` by
  `theme::parse_hex` with a rose fallback — TASK-45). (`.gitignore`s `config.toml` defensively.)
- The **password lives only in the OS keychain** (`keyring`, service `"roses"`, username = email). The
  backend is **cfg-gated per platform**: macOS uses the native Keychain (`apple-native-keyring-store`),
  Linux the Secret Service via the pure-Rust *zbus* backend (`zbus-secret-service-keyring-store`, so the
  static musl build links without a C libdbus dependency). keyring's `v1` feature registers the platform
  store on first use; when none is reachable (e.g. no Secret Service daemon on Linux) a keychain op fails
  with a clear, actionable error (`keyring_error()` attaches a hint) rather than panicking. `login`/`logout`
  merge settings so non-secret prefs (e.g. the browser) survive a re-login/logout.

## Testing & CI

- **Deterministic unit tests, no live network.** The Feedbin client is tested against a local `mockito`
  server; the TUI layout/render is tested with ratatui's `TestBackend`; pure functions (XDG resolution,
  HTML→text, browser resolution, half-block rendering, pre-fetch ordering) are tested directly.
- Project rule: **very low tolerance for flaky tests** — fix the root cause, never paper over with CI
  retries. The suite is routinely run 10× to confirm stability (mockito binds real sockets).
- CI (`.github/workflows/ci.yml`, job `lint-and-test`): on push + PR, install the pinned toolchain
  (rustfmt + clippy) with cargo caching, then `cargo fmt --all --check`, `cargo clippy --all-targets --
  -D warnings`, `cargo test --locked`. `main` requires the `lint-and-test` check (branch protection).
  A separate `linux-keychain` job provisions gnome-keyring under `dbus-run-session` and runs the
  `#[ignore]`d, Linux-only `keychain_round_trip_via_secret_service` test to verify the Secret Service
  path end-to-end (see [`ci.md`](ci.md)).

## Persistence & offline cache (`store.rs`, TASK-41)

A blocking **SQLite** cache under the XDG data dir makes the TUI **offline-first**: `run_loop` paints from
the cache on launch, then the background load reconciles it (Feedbin stays the source of truth for read
state). All cache writes happen on the main thread in `persist_msg` as messages drain, so the `Connection`
never crosses threads. Full schema, sync strategy, and the `rusqlite`/musl trade-off are in
[`persistence.md`](persistence.md).

## Dependencies (why)

| Crate | Why |
| --- | --- |
| `ratatui` 0.30 (`unstable-rendered-line-info`) | TUI; `line_count` for reader scroll clamping. |
| `tokio` (`rt`, `sync`) | Current-thread runtime + `spawn_blocking` pool + `mpsc` channel. |
| `reqwest` 0.13 (`blocking, json, query, rustls`) | Feedbin HTTP client; rustls for painless static builds. |
| `serde` (derive) + `toml` | Typed Feedbin models + config (de)serialization. |
| `rusqlite` 0.40 (`bundled`) | SQLite offline cache (TASK-41). `bundled` compiles SQLite from source (no system libsqlite3); it re-adds a C-compiled dep to the musl build — see [`persistence.md`](persistence.md). |
| `serde_json` | Serialize `Entry` to the cache's JSON blob column. |
| `keyring` 4 (macOS `apple-native-keyring-store`, Linux `zbus-secret-service-keyring-store`) | OS keychain for the password; the pure-Rust zbus Secret Service backend keeps the musl build free of C libdbus. |
| `rpassword` | Hidden password prompt on first run. |
| `dirs` | Home directory for the XDG fallback. |
| `image` (`png, jpeg, gif, webp`) | Decode entry images for half-block rendering. |
| `chrono` (`clock`, no default features) | Parse the RFC 3339 `published` date + format it in local time for the reader header. |
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
