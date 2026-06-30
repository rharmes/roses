# roses

A TUI RSS reader, backed by Feedbin.

## Demo

![roses demo](demo/roses.gif)

The GIF is recorded with [vhs](https://github.com/charmbracelet/vhs) from
[`demo/roses.tape`](demo/roses.tape). To (re)generate it, make sure `roses` is on
your `PATH` and you've signed in to Feedbin once (run `roses` and log in), then:

```sh
vhs demo/roses.tape    # writes demo/roses.gif
```

The script drives an already-authenticated `roses` — no credentials live in it.

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

See [`docs/tui_research.md`](docs/tui_research.md) for the language/stack
rationale and the build-out roadmap.
