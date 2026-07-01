# Continuous integration

`roses` runs a single GitHub Actions workflow, [`.github/workflows/ci.yml`](../.github/workflows/ci.yml),
on **every push and every pull request**. It is the automated enforcement of the project's
"very low tolerance for flaky tests" rule: formatting, linting, and tests must all be green.

## What runs, and why

The `lint-and-test` job (on `ubuntu-latest`) does three checks, in order — the same three you
should run locally before pushing:

| Step | Command | Why |
| --- | --- | --- |
| Formatting | `cargo fmt --all --check` | Fails if any file isn't `rustfmt`-clean. Keeps diffs free of formatting churn. `--check` only reports; it never rewrites. |
| Lint | `cargo clippy --all-targets -- -D warnings` | Clippy over the bin, tests, and examples. `-D warnings` promotes **every** warning to an error, so lint debt can't accumulate. |
| Tests | `cargo test --locked` | Runs the unit tests. `--locked` fails if `Cargo.lock` is stale, guaranteeing CI builds the exact dependency versions that are committed. |

Run the same locally:

```sh
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test --locked
```

## Linux keychain integration (`linux-keychain` job)

A second job on `ubuntu-latest` exercises the Linux keychain path **end-to-end** against a real
Secret Service — the one thing `cargo test` can't cover on a headless box or a macOS dev machine.
The round-trip test (`config::tests::keychain_round_trip_via_secret_service`) is `#[ignore]`d and
Linux-only, so it never runs in the core job or on a developer's macOS; this job provisions a
keyring and runs just it:

```sh
sudo apt-get install -y gnome-keyring dbus
dbus-run-session -- bash -euo pipefail -c '
  echo -n "roses-ci" | gnome-keyring-daemon --unlock --components=secrets
  cargo test --locked -- --ignored keychain_round_trip_via_secret_service
'
```

`dbus-run-session` starts a private session bus; `gnome-keyring-daemon --unlock --components=secrets`
creates/unlocks the login keyring and serves the Secret Service API that the zbus backend
(`zbus-secret-service-keyring-store`, chosen for Linux in `Cargo.toml`) talks to. The test stores,
reads back, and deletes a unique-per-run password, proving login actually persists on Linux.

It's a **separate job** on purpose: a keyring/D-Bus hiccup surfaces as a red check without blocking
the core `lint-and-test` gate. It is intentionally **not** (yet) a required status check for that
reason — promote it once it has proven stable (add `"linux-keychain"` to the `contexts` list below).

## Toolchain pinning

The job installs the toolchain pinned in [`rust-toolchain.toml`](../rust-toolchain.toml) —
`channel = "stable"` with the `rustfmt` and `clippy` components — via:

```sh
rustup toolchain install stable --profile minimal --component rustfmt --component clippy
rustup default stable
```

Pinning means CI and contributors build `roses` with the **same** compiler and lints, so "works
on my machine" and "passes CI" stay in sync. If the pin ever changes to a fixed version (e.g.
`channel = "1.96.0"`), update the `channel` in the install step above to match.

## Caching

[`Swatinem/rust-cache@v2`](https://github.com/Swatinem/rust-cache) caches the cargo registry, the
git index, and the `target/` directory, keyed on the toolchain version and `Cargo.lock`. The first
run is cold (it compiles the full dependency tree — `reqwest`, `rustls`, `tokio`, etc.); subsequent
runs restore the cache and only recompile what changed. The cache step runs **after** the toolchain
install so the key includes the right `rustc` version.

## Other workflow details

- **`permissions: contents: read`** — least privilege; CI never needs write access.
- **`concurrency` with `cancel-in-progress`** — a newer push to the same ref cancels the older run,
  so we don't burn minutes on superseded commits.
- **`CARGO_TERM_COLOR: always`** — readable, colorized logs in the Actions UI.

## Requiring a green run before merge

The workflow reports a status check named **`lint-and-test`**. To make it mandatory before a PR can
merge into `main`, enable branch protection (a one-time, repo-admin action). The check must have run
at least once for GitHub to know it exists.

Via the UI: **Settings → Branches → Add branch ruleset (or classic protection) for `main` →
require status checks to pass → select `lint-and-test`**.

Or with the `gh` CLI (classic branch protection):

```sh
gh api --method PUT repos/rharmes/roses/branches/main/protection \
  -H "Accept: application/vnd.github+json" \
  --input - <<'JSON'
{
  "required_status_checks": { "strict": true, "contexts": ["lint-and-test"] },
  "enforce_admins": false,
  "required_pull_request_reviews": null,
  "restrictions": null
}
JSON
```

`strict: true` also requires the branch to be up to date with `main` before merging.

## Release pipeline (separate from `lint-and-test`)

Distribution (TASK-10) is **not** part of this `lint-and-test` workflow. Tagged
releases are handled by two other workflows — `.github/workflows/release.yml`
([cargo-dist], generated from `dist-workspace.toml`) and
`.github/workflows/publish-crates.yml` — triggered by a `vX.Y.Z` tag. On pull
requests, `release.yml` also runs a lightweight **`plan`** job that validates the
release config without releasing. Full details: [`release.md`](release.md).

[cargo-dist]: https://github.com/axodotdev/cargo-dist
