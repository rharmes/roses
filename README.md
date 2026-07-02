# roses

A TUI RSS reader, backed by Feedbin. 100% Rust and lightning fast. Opens feed items in your browser of choice (graphical or CLI).

![roses demo](demo/roses.gif)

## Install

Pre-built static binaries for **macOS** and **Linux** (x86_64 + aarch64) are
attached to every [GitHub Release](https://github.com/rharmes/roses/releases).

```sh
# Homebrew (macOS / Linux)
brew install rharmes/tap/roses

# Cargo (builds from source from crates.io)
cargo install roses

# Shell installer (downloads the right pre-built binary)
curl --proto '=https' --tlsv1.2 -LsSf \
  https://github.com/rharmes/roses/releases/latest/download/roses-installer.sh | sh
```

> **Linux note:** the Feedbin password is kept in the OS keychain — the native
> Keychain on macOS, the Secret Service (GNOME Keyring, KWallet, …) on Linux. On
> Linux a keyring daemon must be running and unlocked for the login to persist
> across runs. See [`docs/release.md`](docs/release.md).

## Build & run

`roses` is built in Rust. Install a toolchain via [rustup](https://rustup.rs);
the version is pinned in `rust-toolchain.toml`.

```sh
cargo run        # build and run
cargo build      # compile (add --release for an optimized build)
cargo test       # run tests
cargo fmt        # format
cargo clippy     # lint
```

## Config

Non-secret settings live in a TOML file at `$XDG_CONFIG_HOME/roses/config.toml`
(or `~/.config/roses/config.toml`). Your Feedbin **password is never written
there** — it's kept in the OS keychain. `roses` writes `email` on login; the
browser is yours to set:

```toml
email = "you@example.com"

# Command that `o` runs to open an article's URL. `%s` (or `{url}`) is replaced
# with the URL; otherwise the URL is appended. Omit `browser` to fall back to
# $BROWSER or the system opener (`open` on macOS, `xdg-open` elsewhere).
browser = "w3m %s"

# true for a terminal browser like w3m: roses suspends the TUI, runs it, then
# restores. false (the default) launches a GUI browser in the background.
browser_terminal = true

# Background auto-refresh interval, in seconds. Omit (or set 0) to disable it
# (the default). roses re-fetches quietly on this cadence without disturbing
# your place — an unchanged unread set is a cheap conditional request. Values
# below 60 are clamped up to 60s to stay polite to Feedbin.
refresh_interval_secs = 300

# Accent color for the UI chrome — the focused column, the selection bar, the
# reader title, the footer keys, and the help overlay. A hex string: #rrggbb or
# the shorthand #rgb (the leading # is optional). Omit or set an invalid value
# to keep the default rose. The "all caught up" rose art always stays rose.
highlight_color = "#e06c9a"

# Privacy: whether to fetch inline and lead images from the third-party hosts a
# feed names. Set false to block every image request — roses never contacts
# those hosts (so your IP isn't leaked to trackers) and shows a placeholder in
# the reader instead. Omit or set true (the default) to load images normally.
load_remote_images = true
```

## Keyboard shortcuts

| Key(s) | Action |
| --- | --- |
| `↑`/`k`, `↓`/`j` | Move within the focused column (in the reader, scroll) |
| `←`/`h`, `→`/`l` | Move focus across columns (sources → articles → reader) |
| `g`/`Home`, `G`/`End` | First / last item (or top / bottom of the reader) |
| `PgUp`/`PgDn` | Page the reader |
| `m` / `u` | Mark the selected article read / undo the last mark (undo restores a whole bulk mark too) |
| `M` | Mark every loaded article in the selected source read |
| `A` | Mark the whole loaded window read (asks `y`/`n` first) |
| `o` | Open in the browser — a podcast enclosure, else a link-blog's external link, else the article URL |
| `r` | Reload |
| `?` | Toggle a help overlay listing every keybinding (any key closes it) |
| `q`/`Esc` | Quit |
