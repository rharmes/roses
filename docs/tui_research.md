# roses — Language, Stack & TUI Research

Research into which language and terminal-UI stack to build **roses** on: a full-screen
TUI RSS reader backed by the [Feedbin API](https://github.com/feedbin/feedbin-api).

*Compiled June 2026. Library versions and maintenance status verified against current
sources as of that date — re-check before committing, ecosystems move.*

---

## 1. What roses needs

The app is a full-screen terminal program that:

- Logs into a user's **Feedbin** account and syncs RSS state over Feedbin's REST API
  (HTTP Basic auth, JSON).
- Displays feeds and entries in a list/detail TUI; **marks entries read** as they're seen
  and lets that be **undone** (mark unread).
- Shows **low-fidelity image approximations** (Unicode/ANSI block-art) inline — *not* Sixel/Kitty
  graphics protocols.
- Opens the full article in a **browser of the user's choice** — a GUI browser (Chrome/Firefox/Safari)
  or a CLI browser (Carbonyl/w3m/lynx/browsh).
- Stores **only** the logged-in user and app settings locally (everything else comes from the API).

### Decisions already made (these scope the research)

| Decision | Choice | Consequence |
|---|---|---|
| **Image fidelity** | Approximations are fine (block-art, works on any terminal) | We don't need graphics-protocol libraries; widens language choices |
| **Distribution** | Single self-contained **static binary** | Filters *out* runtime-dependent languages (Python/Ruby/Node) as primary picks |
| **Priority axis** | **Max performance & polish** (willing to take a steeper language) | Tilts toward Rust/Zig over "easiest to ship" |
| **Developer background** | Knows **JS/TypeScript**; happy to learn a new compiled language | Frames learning-curve notes; React/Elm mental models transfer well |
| **Targets** | Most Linux/Unix + macOS | Windows is a nice-to-have, not a requirement |

### Evaluation criteria

1. **TUI framework** maturity, ergonomics, and how much hand-written plumbing a list/detail UI needs.
2. **Runtime performance & polish** — startup, redraw smoothness, memory, binary size.
3. **Image-approximation** support (block-art without graphics protocols).
4. **HTTP + JSON** ergonomics for the Feedbin client.
5. **HTML→text** rendering of entry content.
6. **Distribution** — true static binary, cross-compilation, Homebrew + ecosystem package manager.
7. **Learning curve** from a JS/TS background.

---

## 2. TL;DR — recommendation & comparison

> **Primary recommendation: Rust + [ratatui](https://ratatui.rs/).**
> It uniquely satisfies "single static binary, very fast, maximum polish": a 2–5 MB
> static (musl) binary with no GC jitter, and *every* feature roses needs maps to a mature,
> actively-maintained 2026 crate — including `ratatui-image`'s `halfblocks` mode, which is
> exactly the "Unicode block-art, no graphics protocol" requirement, and `keyring` for
> OS-keychain credential storage. The cost is the steepest learning curve.
>
> **Strong runner-up: Go + [Bubble Tea](https://github.com/charmbracelet/bubbletea).**
> If you'd trade a little runtime performance for the *easiest* path to ship and the
> *fastest* learning curve, Go wins. Its single-binary + Homebrew automation story is the
> best of any option. The one compromise is image rendering (shell out to `chafa`).
>
> **Interesting wildcard: Zig + [libvaxis](https://github.com/rockorager/libvaxis).**
> Best-in-class static binaries and a graphics-capable TUI, but pre-1.0 churn and a
> hand-rolled HTTP/JSON layer make it a project for someone who wants the adventure.

| Stack | Perf & polish | TUI maturity | Images (block-art) | HTTP/JSON | Distribution | Learn from JS/TS | Overall fit |
|---|---|---|---|---|---|---|---|
| **Rust + ratatui** | ★★★★★ | ★★★★★ | ★★★★★ (`ratatui-image` halfblocks) | ★★★★★ | ★★★★☆ | ★★☆☆☆ (hard) | **Best for stated priorities** |
| **Go + Bubble Tea** | ★★★★☆ | ★★★★★ | ★★★☆☆ (shell out to chafa) | ★★★★★ | ★★★★★ | ★★★★★ (easy) | **Best balance / lowest risk** |
| **Zig + libvaxis** | ★★★★★ | ★★★☆☆ | ★★★★☆ | ★★☆☆☆ (immature std) | ★★★★★ | ★★☆☆☆ | Adventurous |
| **C + notcurses** | ★★★★★ | ★★★★☆ | ★★★★★ (gold standard) | ★★★★★ (libcurl) | ★★★☆☆ (fiddly static) | ★★☆☆☆ | Library, not primary |
| **Nim + illwill** | ★★★★☆ | ★★☆☆☆ | ★☆☆☆☆ (DIY) | ★★★★☆ (stdlib) | ★★★★☆ | ★★★★☆ | Thin TUI layer |
| **Crystal** | ★★★★☆ | ★★☆☆☆ | ★☆☆☆☆ (DIY) | ★★★★★ (stdlib) | ★★★☆☆ | ★★★★☆ | Weak TUI ecosystem |
| **OCaml + nottui** | ★★★★☆ | ★★★★☆ | ☆☆☆☆☆ (no images) | ★★★★☆ | ★★★☆☆ | ★★☆☆☆ | No image story |
| **JS/TS + Bun compile** | ★★☆☆☆ | ★★★★☆ (Ink) | ★★☆☆☆ (DIY) | ★★★★★ | ★★☆☆☆ (~50–90 MB) | ★★★★★ (known) | Familiar fallback |

★ ratings are a qualitative read of the research below *for this specific app*, not absolute quality scores.

---

## 3. The contenders, in detail

### 3.1 Rust + ratatui — *the performance-and-polish pick*

**Language.** Native, no GC, memory-safe. The "max performance & polish" priority points here first.

**TUI framework.** [**ratatui**](https://ratatui.rs/) `v0.30.2` (Jun 2026) — the de-facto standard
(community fork of the abandoned `tui-rs`; ~5M downloads/mo, ~21k stars, no serious competitor).
It's **immediate-mode**: you redraw the whole UI from your app state every frame inside
`terminal.draw(...)`. Widgets are cheap value structs; ratatui diffs and only writes changed cells.
- **Backend:** [crossterm](https://crates.io/crates/crossterm) `v0.29` (cross-platform, the default).
  Avoid termion (Unix-only, less maintained).
- **Plumbing cost:** moderate. You hand-write the event loop, the layout split (feeds | entries |
  reader), `List` + `ListState` for selection/scroll, and a scrollable `Paragraph` for the body —
  a few hundred lines for a two-pane list/detail, a well-trodden pattern.
- **Reduce boilerplate with:** [tui-realm](https://github.com/veeso/tui-realm) (Elm/React-style
  components & messages), [tui-widgets](https://github.com/ratatui/tui-widgets) (scrollbar, popup),
  or ratatui's official component template.
- **Proven at scale:** `gitui`, `yazi`, `bottom`, `television` are all ratatui.

**Performance.** ~2–5 MB static musl binary (vs ~10–15 MB Go); no GC pauses to break frame pacing;
lowest steady-state memory. Startup is ~1–5 ms (a non-differentiator vs Go). The real cost is
*compile times* (minutes vs Go's seconds) — a slower edit/run loop.

**Images (block-art).** [**ratatui-image**](https://github.com/ratatui/ratatui-image) `v11.0.6` has a
**`halfblocks` mode** — Unicode half-block chars with truecolor fg/bg, works in *any* terminal with
**no graphics protocol**. It's a real ratatui widget (handles resize/area). This is the cleanest match
for the image requirement. For a more stylized ASCII look: [artem](https://docs.rs/artem),
[rascii_art](https://crates.io/crates/rascii_art), [viuer](https://crates.io/crates/viuer).

**HTTP + JSON.** [reqwest](https://crates.io/crates/reqwest) `v0.13` (`.basic_auth()`, `.json()`) +
[serde](https://serde.rs/) for typed Feedbin models. **Recommended: async with tokio** so the TUI stays
responsive during fetch/mark-read, feeding results back to the render loop over a channel. Lighter
alternative: [ureq](https://crates.io/crates/ureq) (synchronous, smaller) on a background thread + `mpsc`.
Use `rustls` (pure-Rust TLS) to keep musl builds painless.

**HTML→text.** [ammonia](https://crates.io/crates/ammonia) (sanitize third-party HTML) →
[html2text](https://docs.rs/html2text/) `from_read_coloured()` (emits styled terminal output that maps
onto ratatui `Span`/`Line`) → render in a `Paragraph`. [scraper](https://crates.io/crates/scraper) for
pulling out `<img>` URLs.

**Distribution.** Build `x86_64-unknown-linux-musl` / `aarch64-...-musl` for a *truly static* Linux
binary; `*-apple-darwin` for macOS (self-contained, not statically linked to libSystem — that's normal
and fine). [**cargo-dist**](https://github.com/axodotdev/cargo-dist) generates the whole release pipeline
including an **auto-generated Homebrew formula**; [cargo-binstall](https://github.com/cargo-bins/cargo-binstall)
lets users grab prebuilt binaries; `cargo install roses` works from crates.io.
*(Sanity-check cargo-dist's maintenance cadence before relying on it; otherwise a hand-written GoReleaser-style
or GitHub Actions matrix works too.)*

**Learning curve from JS/TS.** The steepest of the mainstream options. Budget **2–4 weeks** to productivity
and **~2–3 months** to borrow-checker fluency; expect an initial "fighting the borrow checker" phase, plus
`async` (futures, `Send`/`Sync`). **But:** Rust's **enums + exhaustive `match`** model TUI screen/mode state
machines beautifully (`enum Screen { FeedList, Entry(EntryId), Help }`) and `Result`/`?` give clean HTTP/JSON
error handling — TS devs already think in tagged unions, so that transfers directly. A mostly-single-threaded
UI loop with bounded async fetches is close to an ideal *first* Rust project.

**Pros**
- Best perf/footprint: 2–5 MB static binary, no GC jitter, smooth full-screen redraws.
- ratatui is mature, dominant, battle-tested at exactly this scale.
- Every feature has a maintained 2026 crate, incl. the *exact* block-art and OS-keychain needs.
- Enums/pattern-matching are a superb fit for TUI state and a clean undo stack.

**Cons**
- Steepest learning curve for a JS/TS dev (borrow checker + async).
- Slow compile/iteration loop.
- Immediate-mode = more hand-written UI plumbing (mitigated by tui-realm).

**Verdict.** The strongest stack for roses *as specified*. It's the only option that maxes out every stated
priority at once. Pay for it in learning time — front-loaded and manageable for a perf-first solo project.

---

### 3.2 Go + Bubble Tea — *the balanced, lowest-risk pick*

**Language.** Native, garbage-collected, deliberately small. Famous for fast onboarding and the best
single-binary distribution story going.

**TUI framework.** [**Bubble Tea**](https://github.com/charmbracelet/bubbletea) `v2.0.7` (stable v2.0.0
shipped Feb 2026) + [Lip Gloss](https://github.com/charmbracelet/lipgloss) (styling) +
[Bubbles](https://github.com/charmbracelet/bubbles) (components). It's the **Elm architecture**
(`Model` / `Update(msg)` / `View()`) — a near-perfect match for a dev who's touched React/Redux. Async
work returns `tea.Cmd`s that emit messages, so the UI thread is non-blocking *by construction*.
- v2 adds a faster ncurses-based renderer, synchronized output (no tearing), and richer key handling.
- **Bubbles you'd use directly:** `list` (filterable/paginated — ideal for feeds/entries), `viewport`
  (scrollable article body), `textinput`, `spinner`, `help`. Covers most of a list/detail UI out of the box.
- **Alternative:** [tview](https://github.com/rivo/tview) + [tcell](https://github.com/gdamore/tcell) —
  a traditional retained-mode widget toolkit. Faster to assemble stock screens, but less "polish" headroom
  and manual concurrency. Bubble Tea is the better fit for a *polished* reader.
- **Proven RSS TUIs in Go:** [nom](https://github.com/guyfedwards/nom),
  [goread](https://github.com/TypicalAM/goread), [newsgoat](https://github.com/jarv/newsgoat) — the exact
  app category already exists here.

**Performance.** Startup is single-digit ms (slight edge over Rust). Redraw is fast, though Go TUIs sit at
~6–8% CPU vs Rust's ~2% during continuous 60 FPS redraws (GC) — **irrelevant for a mostly-idle reader**.
Binary ~2 MB stripped (vs Rust ~300 KB). Memory a few × Rust's. None of this matters for this workload.

**Images (block-art) — the one weak spot.** In-process Go libraries are CLI-first and rough
([ascii-image-converter](https://github.com/TheZoraiz/ascii-image-converter) has a library API + braille
mode but admits it needs polish; [pixterm](https://github.com/eliukblau/pixterm) is CLI-first). **The clean
answer is to shell out to [`chafa`](https://hpjansson.org/chafa/)** — the gold standard for character-art —
and pipe its output into a Lip Gloss box, with `ascii-image-converter` as a pure-Go fallback. Cost: an
external Homebrew `depends_on "chafa"`.

**HTTP + JSON.** Stdlib only — `net/http` (`req.SetBasicAuth`) + `encoding/json`. No dependencies. Run calls
as `tea.Cmd`s so the UI never blocks.

**HTML→text.** [bluemonday](https://github.com/microcosm-cc/bluemonday) (sanitize) →
[goquery](https://github.com/PuerkitoBio/goquery) (extract image URLs / strip boilerplate) →
[html2text](https://github.com/jaytaylor/html2text) for the body. *Or* convert to Markdown and render with
[Glamour](https://github.com/charmbracelet/glamour) for styled output (what several Go RSS TUIs do).

**Distribution — Go's standout strength.** `CGO_ENABLED=0` → a *truly static* binary (every library above is
pure Go). Cross-compile with `GOOS`/`GOARCH` — **no cross-toolchain, no Docker** (materially simpler than Rust).
[**GoReleaser**](https://goreleaser.com/customization/homebrew/) automates multi-platform builds + checksums +
GitHub Release + an **auto-committed Homebrew tap formula**. End result: `brew install you/tap/roses` with
zero manual packaging. *(Add `depends_on "chafa"` if you shell out for images.)*

**Learning curve from JS/TS.** The easiest here — productive in **days**, comfortable in **2–4 weeks**.
goroutines/channels map onto Bubble Tea's `Cmd`/`Msg` model; explicit `if err != nil` instead of try/catch;
structural interfaces feel familiar to a TS dev. Adjustments (no classes, pointers, zero values) are minor.

**Pros**
- Best-in-class static-binary + cross-compile + GoReleaser→Homebrew automation.
- Bubble Tea `list`/`viewport` map directly onto a polished list/detail RSS UI; prior art exists.
- Elm + goroutines make async Feedbin calls and undo clean and non-blocking.
- Stdlib covers Feedbin; mature sanitize/HTML libs; OS keychain via go-keyring.
- Shortest learning curve of the compiled options.

**Cons**
- In-process image→block-art is immature → external `chafa` dependency.
- Loses to Rust on raw CPU/memory/binary size (irrelevant for this app, but real if you obsess over it).
- GC overhead and a larger binary than Rust.

**Verdict.** Excellent, low-risk choice: the Charm stack gives a polished UI with minimal effort, the whole
Feedbin/HTTP/keychain/browser surface is covered by mature pure-Go libs, and distribution is nearly free.
The only compromise is images (accept the `chafa` dependency). If "easiest to ship + quick to learn" ever
outranks "max raw performance," **Go wins outright.**

---

### 3.3 Zig + libvaxis — *the adventurous wildcard*

**Language.** Zig `0.16.0` (Apr 2026) — native, no GC, trivial static binaries and cross-compilation
(its headline feature). **Still pre-1.0 and explicitly unstable.**

**TUI.** [**libvaxis**](https://github.com/rockorager/libvaxis) by a Ghostty maintainer — pure-Zig, tracks
Zig `0.16` on `main` (you pin a git commit, not a semver tag), with a low-level API plus `vxfw`, a
Flutter-like high-level framework. **Supports the Kitty graphics protocol** (real images) *and* easy
half-block/ASCII fallback — strong for the image feature. Can also FFI into C notcurses.

**The catch — HTTP/JSON.** `std.http.Client` + `std.json` exist but are **immature** (e.g. large-HTTPS-payload
bugs in 0.16-dev); few mature third-party HTTP libs. You'll hand-roll more of the Feedbin layer and own edge
cases. Pre-1.0 churn ("Writergate", the new `std.Io` interface) breaks std APIs between releases — libraries
chase each release, a real maintenance tax.

**Pros:** best distribution/cross-compile story of all; graphics-capable TUI; C-grade performance; can borrow
notcurses. **Cons:** pre-1.0 instability, immature HTTP/JSON, manual memory management, steepest jump after C.

**Verdict.** The strongest *niche* fit on performance + distribution + images — *if* you accept pre-1.0 churn
and writing the HTTP/JSON plumbing yourself. A great choice for someone who wants the language adventure as
much as the app; a risky one if you just want roses shipped.

---

### 3.4 Other compiled options (honest, briefer takes)

**C + [notcurses](https://github.com/dankamongmen/notcurses).** notcurses is the **gold standard** for
terminal character-graphics (block-art *and* true bitmaps), and libcurl + a JSON lib are the most mature
HTTP/JSON stack anywhere. But C's manual memory, no package manager, error-prone strings, and *fiddly*
static-binary packaging (sourcing static `.a`s for notcurses + deps) make it a poor *primary* choice for a
polished consumer app. **Best consumed as a library** (FFI from Zig/OCaml) when image fidelity is paramount.

**Nim + [illwill](https://github.com/johnnovak/illwill).** Nim `2.2.6` compiles via C → native speed, with
`std/httpclient` + `std/json` built in and clean musl static binaries. Gentle, Python-ish syntax. **But the
TUI story is thin:** illwill is low-level and effectively stalled; [nimwave](https://github.com/ansiwave/nimwave)
adds a box hierarchy but the ecosystem is small. No image library — block-art is DIY or notcurses FFI. Pleasant
language, lots of from-scratch UI work.

**Crystal.** `1.20.x` (May 2026), Ruby-like, native via LLVM, with **first-class stdlib HTTP + JSON**
(`JSON::Serializable` makes Feedbin DTOs trivial) — arguably the best ergonomics here. **But the TUI ecosystem
is the weakest:** small/semi-abandoned options (`crysterm`, `crt`, `term-screen`); no image library; fully-static
binaries are easiest on Alpine/musl and finicky elsewhere; small `shards` community. Lovely syntax, threadbare
terminal-graphics support.

**OCaml + [nottui](https://opam.ocaml.org/packages/nottui/).** Strongest type system here → fewer runtime bugs,
great long-term maintainability; nottui is a genuinely good declarative TUI; cohttp + yojson are solid. **But
notty/nottui render no images** — the block-art feature is entirely DIY or C FFI — and the FP learning curve
(Hindley–Milner, Lwt async) is steep for a JS dev. Static linking is feasible but fiddly. Great for correctness,
undercut by the missing image story for a graphics-forward reader.

---

### 3.5 JS/TypeScript single-binary — *the familiar path you already know*

You already know JS/TS, so it's worth being explicit about why it *isn't* the top pick despite the lowest
learning effort.

- **Single binary is possible:** [Bun](https://bun.sh) `build --compile` is the best path (~<10 ms cold start,
  single executable; Bun was acquired by Anthropic in Dec 2025). Deno `compile` and Node SEA also work
  (Node SEA least mature).
- **TUI:** [Ink](https://github.com/vadimdemedes/ink) `7.0` (React for CLIs — mature, powers Claude Code, but
  historically ~30 fps and ~50 MB baseline memory); blessed is unmaintained (use neo-blessed/reblessed);
  [OpenTUI](https://github.com/anomalyco/opentui) (Zig native core + Bun, sub-ms frames) is promising but **not
  production-ready** and Bun-coupled.
- **Why it loses on the stated priority:** it is **not a true static native binary** — the JS runtime is
  *embedded*, so binaries are **~50–90 MB** and startup, while fine for JS, is slower than any native option.
  It's GC'd with higher memory. That directly conflicts with "max performance & a lean single static binary."

**Verdict.** The pragmatic, lowest-risk-to-*ship* option that reuses your existing skills and gets a working
reader fastest — but it structurally compromises roses's top stated priority. Present it as the **fast
path / fallback**, not the performance answer. (Amusing footnote: OpenTUI's native core is Zig, so even the
JS route leans on §3.3's language under the hood.)

---

## 4. Cross-cutting concerns (apply to every option)

These are app-level requirements that any language choice must satisfy. None of them changes the language
ranking, but they're the meat of the implementation.

### 4.1 Feedbin API shape

Authoritative docs: <https://github.com/feedbin/feedbin-api> (`content/*.md`).

- **Base / format:** `https://api.feedbin.com/v2/`, HTTPS only, all paths end in `.json`. Write requests must
  send `Content-Type: application/json; charset=utf-8` or you get **415**. Dates are ISO-8601 with microseconds.
- **Auth:** **HTTP Basic with email + password on every request.** There are **no API tokens / OAuth / app
  passwords** — the raw email+password is the only credential, which is *why* secure local storage (§4.4)
  matters. Validate at login with `GET /v2/authentication.json` → **200** valid / **401** invalid.
- **Read/unread state machine** (`unread_entries`) — this is the core of roses:
  - `GET /v2/unread_entries.json` → array of **unread entry IDs** (source of truth; hydrate with
    `GET /v2/entries.json?ids=…`).
  - `DELETE /v2/unread_entries.json` with `{"unread_entries":[…]}` → **mark read**.
  - `POST /v2/unread_entries.json` with `{"unread_entries":[…]}` → **mark unread — this is your UNDO.**
  - **Max 1,000 IDs per request.** Response echoes which IDs actually changed. Clients that can't send a
    DELETE body use `POST /v2/unread_entries/delete.json`.
- **Starred entries** mirror the same shape (`GET`/`POST`/`DELETE /v2/starred_entries.json`, 1,000-ID cap).
- **Entries:** `GET /v2/entries.json` (paginated, `created_at` DESC). Params: `page`, `since`, `ids` (≤100),
  `read`, `starred`, `per_page`, `mode=extended` (adds `images`, `enclosure`, `content_diff`, etc.). Fields
  include `content` (HTML), `summary`, `url`, `published`.
- **Sync hygiene:** default 100 items/page; `Links` header for `rel="next"`; `X-Feedbin-Record-Count` for totals.
  Every GET returns `ETag` + `Last-Modified` — send `If-None-Match`/`If-Modified-Since` for **304** to save
  bandwidth. The `since` param does incremental sync — **echo back the server's exact microsecond timestamp**
  or you'll get overlap. Rate limits aren't documented; be a good citizen (ID-array + conditional GETs +
  batched writes).
- **Content:** `content` is HTML with links/`<img src>` rewritten to absolute URLs, but **final
  sanitizing/escaping is the client's job** (see §3 per-language HTML pipelines). `<img>` URLs feed the image
  renderer (§4.2).

### 4.2 Image-approximation tooling

Target the **lowest common denominator: Unicode block-art + ANSI truecolor**, which works in any modern
terminal without graphics protocols.

- **[chafa](https://hpjansson.org/chafa/) — the leading choice.** Widest symbol repertoire (half-blocks,
  quadrants, sextants, octants, braille, ASCII), truecolor/256/16/8, animation. Degrades gracefully to
  block-art when Sixel/Kitty aren't available. Has a stable C API (`libchafa`) *and* a CLI; shelling out
  (`chafa --format symbols --size 80x25 img.png`) is the most language-agnostic path. Needs a Unicode
  terminal + GLib; half-block mode is universally safe (sextants/octants want a "Symbols for Legacy
  Computing" font).
- **[timg](https://github.com/hzeller/timg)** — 24-bit half/quarter blocks, ~3–5× faster on image grids
  (parallel decode); fewer symbol modes. Good speed-focused fallback.
- **[viu](https://github.com/atanunq/viu)** (Rust, half-block, simple) and **catimg** (C, basic) round it out.

Recommendation: shell out to **chafa** for best fidelity regardless of language; in Rust you can instead use
the in-process `ratatui-image` halfblocks widget (§3.1) to avoid an external dependency.

### 4.3 Browser launching

- **Default open:** macOS `open <url>` (or `open -a "Google Chrome" <url>`); Linux/BSD `xdg-open <url>`.
  Honor the **`$BROWSER`** env var (colon-separated list, `%s` = URL) — the cleanest way to respect a user's
  choice. Mirror [sindresorhus/open](https://github.com/sindresorhus/open) for the platform matrix.
- **CLI/TUI browsers** (open the URL *inside* the terminal): **[Carbonyl](https://github.com/fathyb/carbonyl)**
  (Chromium fork, real DOM/CSS/JS to Unicode at 60fps — highest fidelity, heavyweight), **browsh** (headless
  Firefox, resource-heavy), **w3m** (fast text + inline images on some terminals), **lynx** (lightest, text-only).
- **Config:** a `browser` setting that accepts either an app name or a command template with a `%s`/`{url}`
  placeholder. **Gotchas:** over SSH/headless, GUI open fails — detect and fall back to a CLI browser or print
  the URL; always escape the URL before handing it to a shell; spawning a *TUI* browser means suspending and
  restoring roses's alternate screen + raw mode around the child process.

### 4.4 Local state & secure credentials

- **Paths (XDG + macOS):** config → `$XDG_CONFIG_HOME` (`~/.config/roses/config.toml`); state (sync cursors,
  last-`since`) → `$XDG_STATE_HOME` (`~/.local/state/roses/`); image cache → `$XDG_CACHE_HOME`. On macOS, CLI
  tools widely follow XDG `~/.config` too (gh, git, kubectl all do) — recommend honoring the env vars on both
  platforms.
- **Settings format:** **TOML** (idiomatic for human-edited CLI config — comments, used by cargo/ripgrep/starship).
  Holds non-secret settings (theme, keybinds, browser, image tool, per_page).
- **Credential — use the OS keychain.** Because Feedbin makes you replay the raw email+password, store the
  password in the **OS keychain** (macOS Keychain / Linux Secret Service via libsecret / Windows Credential
  Manager) via a keyring-style library (Rust [keyring](https://crates.io/crates/keyring), Go
  [go-keyring](https://github.com/zalando/go-keyring), etc.). Keep username + settings in the config file.
  **Fallback** for headless boxes with no Secret Service: a separate `0600`-perm file (document the weaker
  guarantee) or an env var for CI. **Never** commit the credential — keep a defensive `.gitignore` entry for
  the config/credential filename (matches the repo's CLAUDE.md rule). Validate via `authentication.json`
  before storing.

### 4.5 Distribution mechanics

- **Homebrew (primary for macOS, also Linux):**
  - **Custom tap (recommended for a personal CLI):** users run `brew tap rossharmes/roses && brew install roses`.
    Homebrew-core rarely accepts niche CLIs; a tap is the expected route and you control releases.
  - **Bottles (precompiled):** build per-OS/arch in GitHub Actions (`brew install --build-bottle` → `brew bottle`),
    host via a GitHub Release `root_url` for instant installs. Both **cargo-dist** (Rust) and **GoReleaser** (Go)
    *generate the formula and bottles for you* — the single biggest distribution convenience.
- **Ecosystem package managers (layer on top of prebuilt GitHub Release binaries):**
  - **Rust:** `cargo install roses` (crates.io, source build); `.deb` via cargo-deb; AUR via cargo-aur; cargo-dist
    for cross-built artifacts + installers.
  - **Go:** `go install github.com/rossharmes/roses@latest`.
  - **Debian/Ubuntu:** `.deb` + apt repo/PPA. **Arch:** a PKGBUILD on the AUR (broadest niche-CLI coverage).
- **Recommended progression:** ship prebuilt per-OS/arch binaries on **GitHub Releases** as the universal
  baseline → add a **Homebrew tap** → add **AUR / apt / cargo / go install** as reach allows.

---

## 5. Recommendation & next steps

**Given your stated priorities** — single static binary, max performance & polish, block-art images are fine,
and a willingness to learn a steeper language — the ranking is:

1. **Rust + ratatui** — best fit for the priorities as written. Smallest static binary, no GC jitter, the
   `ratatui-image` halfblocks widget removes even the external `chafa` dependency, and every other need has a
   mature crate. Cost: a real but front-loaded learning curve.
2. **Go + Bubble Tea** — pick this if you'd rather optimize for *shipping speed and learnability* than for the
   last 10% of runtime performance. The distribution and TUI-ergonomics stories are the best of any option; the
   only compromise (images via `chafa`) is minor.
3. **Zig + libvaxis** — pick this only if the *language adventure* is part of the appeal and you're OK hand-rolling
   HTTP/JSON and tracking pre-1.0 churn.

The remaining options (C, Nim, Crystal, OCaml, JS/TS) are documented above for completeness but each has a
disqualifying weakness for *this* app (packaging/ergonomics, thin TUI, no image story, or non-native binaries).

**A note on the trade-off you're actually choosing between:** Rust vs Go here is "lowest-level control and a
2–5 MB binary, paid for in learning time and compile speed" vs "near-identical end-user experience for a
mostly-idle reader, shipped sooner, learned faster, with a slightly larger binary and an external image tool."
For an I/O-bound RSS reader, the *runtime* difference is largely imperceptible — so the honest decision is really
**how much you want to invest in learning Rust** versus getting roses in front of users quickly. You said max
performance & polish, so Rust is the recommendation; but Go is the answer that respects "easy to build and
maintain" hardest, and it would be a perfectly defensible choice.

### Suggested next steps

- **Decide Rust vs Go** (the two real finalists). If undecided, a 1–2 day spike in each — render a static list,
  fetch `unread_entries.json`, and open a URL in `$BROWSER` — will tell you more than more reading.
- Lock in the cross-cutting building blocks regardless of language: Feedbin client around `authentication.json`
  + `unread_entries` (with POST-to-undo), OS-keychain credential storage, `$BROWSER` launching, TOML settings
  under XDG paths.
- Stand up the **distribution pipeline early** (cargo-dist or GoReleaser → GitHub Releases → Homebrew tap) so
  "single binary on Homebrew" is proven from the first tagged build, not retrofitted.

---

## 6. Rust and Go as languages — a programmer's view

Rust and Go are the two finalists, so they deserve a portrait at the language level — not "which has the
better RSS crate," but what each is actually *like* to write, where its ecosystem stands in 2026, and where
it's headed. They were designed in opposite directions and that shows in every line you write.

The one-sentence framing: **Rust maximizes control and correctness, and charges you complexity for it;
Go maximizes simplicity and velocity, and charges you expressiveness for it.**

### 6.1 Rust

**The pitch.** A systems language that gives you C/C++-level performance and control with *compile-time
memory safety and no garbage collector*. Its defining idea — **ownership and borrowing** — lets the compiler
prove, statically, that you never have a use-after-free, double-free, or data race, without a runtime to
police it. The slogan "fearless concurrency" is the payoff: if it compiles, a whole class of bugs is
provably absent.

**The type system is the point.** Coming from TypeScript, Rust will feel like the type system you reach for
in TS made mandatory and far stronger:

- **Algebraic data types.** `enum`s are real tagged unions (`enum Screen { FeedList, Reading(EntryId), Help }`),
  and `match` is *exhaustive* — add a variant and the compiler lists every place you forgot to handle it.
  This is the single feature that makes Rust so good at modeling state machines (exactly a TUI's job).
- **No null.** Absence is `Option<T>`; you can't accidentally deref nothing.
- **Errors are values.** `Result<T, E>` plus the `?` operator give clean, explicit error propagation with no
  exceptions and no hidden control flow.
- **Traits** (≈ interfaces / typeclasses) drive generics and polymorphism; **lifetimes** are the annotations
  that let the borrow checker reason about references.

**The catch is also the type system / memory model.** Ownership, borrowing, and lifetimes are a genuinely new
mental model — there's a well-known "fighting the borrow checker" phase, and `async` adds its own complexity
(`Send`/`Sync`, pinning, executor choice). Compile times are slow, so the edit→build→run loop drags compared
to Go or a scripting language.

**Tooling (a real strength).** `cargo` is widely considered a best-in-class build tool + package manager:
one command for build/test/bench/docs, a single manifest, and [crates.io](https://crates.io) for dependencies.
`rustup` manages toolchains/targets, `clippy` lints aggressively, `rustfmt` formats, and `rust-analyzer` gives
excellent editor support. The compiler's error messages are famously good — they often tell you the fix.

**Ecosystem (2026).** Mature and still growing fast. Stable since **1.0 in 2015**, evolving through *editions*
(2015/2018/2021/**2024**, the 2024 edition now current) so the language can advance without breaking old code;
point releases ship every six weeks. Rust has crossed from "promising" to "load-bearing infrastructure": it's
**in the Linux kernel**, in Android and Windows components, at AWS (Firecracker), Cloudflare, Discord, Dropbox,
and Microsoft, and it's a top choice for CLI tools, WebAssembly, and embedded. The library ecosystem for this
project is excellent (see §3.1); the main gaps are in younger niches, not anything roses touches.

**Long-term prospects.** Strong. Governance sits with the vendor-neutral **Rust Foundation** (AWS, Google,
Microsoft, Meta, and others), so it isn't dependent on one company. It has topped "most admired language"
surveys for the better part of a decade, and the industry/government push toward **memory-safe languages**
(CISA and others naming C/C++ a liability) is a structural tailwind. The realistic risk isn't disappearance —
it's that Rust stays a high-skill, deliberately-adopted language rather than a default, so hiring and ramp-up
remain costlier than for mainstream languages.

### 6.2 Go

**The pitch.** A language built at Google to make large teams productive on networked services and tooling, with
**simplicity as the explicit goal**. The bet is that a small, boring language with fast compiles, one obvious way
to do things, and great built-in tooling beats a powerful-but-complex one at scale. You can read essentially all
of Go in a weekend, and most Go code looks the same — by design.

**The feel.** Static types, but deliberately minimal:

- **Goroutines and channels** are the signature feature: lightweight concurrency (CSP-style) where you spawn
  thousands of `goroutine`s and coordinate over `channel`s. Concurrency that's awkward elsewhere is idiomatic and
  approachable here — the best-in-class story of any mainstream language.
- **Structural interfaces.** A type satisfies an interface just by having the methods — no `implements` keyword.
  This feels natural to a TS developer used to structural typing.
- **Explicit errors.** No exceptions; functions return an `error` you check with `if err != nil { ... }`. Verbose
  and repetitive, but utterly predictable — control flow is always on the page. (`panic`/`recover` exist for
  truly exceptional cases.) An attempt to add lighter error-handling syntax was explored and ultimately *dropped*
  — a window into Go's "resist features" culture.
- **Composition over inheritance.** Structs + methods + embedding; no classes, no inheritance hierarchies.
- **Generics** arrived in **1.18 (2022)** — useful, intentionally limited.

**The notable gap.** Go has **no sum types / enums and no exhaustive matching**. Modeling a "this is exactly one
of these N states" — a TUI's bread and butter — is done with interfaces, type switches, or integer constants, none
of which the compiler checks for completeness. This is precisely where Rust's `enum`/`match` shines, and it's the
clearest expressiveness cost of Go's minimalism for an app like roses. `nil` is also a real footgun (nil pointers,
nil interfaces).

**Tooling (also a real strength).** The `go` command is batteries-included: build, test, benchmark, format
(`gofmt` — non-negotiable, ends all style debates), vet, modules, a built-in **race detector**, and profiling
(`pprof`). **Compilation is famously fast**, which keeps iteration tight. `gopls` powers editor support.

**Ecosystem (2026).** Very mature and rock-stable. Released 2009, **1.0 in 2012**, now in the **1.26 era**
(early 2026), and governed by the **Go 1 compatibility promise** — code written years ago still builds, which
makes Go unusually low-maintenance over time. Go *owns* cloud-native and DevOps infrastructure: **Docker,
Kubernetes, Terraform, Prometheus, etcd, CockroachDB, Hugo** are all Go. The standard library is broad and
high-quality (its `net/http` and `encoding/json` cover the entire Feedbin client with no dependencies). The one
soft spot relevant here — mature in-process terminal-image libraries — is covered by shelling out to `chafa`.

**Long-term prospects.** Excellent and *stable* rather than *exciting*. Backed by Google, entrenched as the
default language of backend services, cloud infrastructure, and CLI tooling, with a culture that deliberately
resists churn. It won't surprise you and it won't disappear; the trade is that the things people wish Go had
(sum types, less error boilerplate) tend to arrive slowly or not at all, on purpose.

### 6.3 Which mindset fits you

- Choose **Rust** if you want maximum performance and the strongest correctness guarantees, you find the
  type system genuinely appealing (its `enum`/`match` is a near-perfect fit for TUI state and a clean undo
  stack), and you're willing to pay in learning time and compile speed. It rewards patience with software that
  is fast, small, and hard to break.
- Choose **Go** if you want to be productive *quickly*, value a tiny language and fast builds, and are happy
  to trade some expressiveness (notably the missing sum types) for simplicity and the best single-binary
  distribution story in the business. It rewards you with momentum.

Both are first-rate, actively governed, and a safe bet for the next decade. For roses specifically the earlier
recommendation stands — Rust for the stated "max performance & polish," Go if "easy to build and maintain"
quietly matters more — but at the language level, neither is a wrong answer; they're a temperament choice.

---

*Sources are linked inline throughout. Key references: [ratatui](https://ratatui.rs/) ·
[ratatui-image](https://github.com/ratatui/ratatui-image) · [Bubble Tea](https://github.com/charmbracelet/bubbletea) ·
[libvaxis](https://github.com/rockorager/libvaxis) · [notcurses](https://github.com/dankamongmen/notcurses) ·
[chafa](https://hpjansson.org/chafa/) · [Feedbin API](https://github.com/feedbin/feedbin-api) ·
[GoReleaser Homebrew](https://goreleaser.com/customization/homebrew/) ·
[cargo-dist](https://github.com/axodotdev/cargo-dist).*
