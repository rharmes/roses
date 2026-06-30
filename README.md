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

> **Linux note:** the Feedbin password is stored in the OS keychain, which is wired
> up for macOS only — on Linux the login isn't persisted yet. See
> [`docs/release.md`](docs/release.md).

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
```

## Keyboard shortcuts

| Key(s) | Action |
| --- | --- |
| `↑`/`k`, `↓`/`j` | Move within the focused column (in the reader, scroll) |
| `←`/`h`, `→`/`l` | Move focus across columns (sources → articles → reader) |
| `g`/`Home`, `G`/`End` | First / last item (or top / bottom of the reader) |
| `PgUp`/`PgDn` | Page the reader |
| `m` / `u` | Mark the selected article read / undo the last mark |
| `o` | Open the selected article in the browser |
| `r` | Reload |
| `q`/`Esc` | Quit |
