# roses

A TUI RSS reader, backed by Feedbin.

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
