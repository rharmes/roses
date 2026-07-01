---
id: TASK-24
title: Add Linux OS keychain support (Secret Service backend)
status: Done
assignee:
  - '@ross'
created_date: '2026-06-30 21:19'
updated_date: '2026-07-01 13:33'
labels:
  - rust
dependencies: []
documentation:
  - docs/release.md
  - docs/architecture.md
  - docs/data-model.md
priority: medium
ordinal: 10014
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
On Linux the keyring crate has no backend enabled, so a Feedbin login does not persist between runs (documented as a known limitation in docs/release.md). Wire up a persistent OS keychain backend for Linux so login survives restarts, matching the macOS experience.

Use a cfg(target_os = linux) keyring dependency with a Secret Service backend (GNOME Keyring / KWallet over D-Bus). Prefer the pure-Rust zbus-based secret-service backend (e.g. keyring's sync-secret-service feature) so the static musl release build still links without a C libdbus dependency. macOS keeps using apple-native-keyring-store (cfg-gated, unchanged).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 On Linux, the Feedbin password is stored in and read from the OS Secret Service, so login persists across runs (service 'roses', username = email, same as macOS)
- [x] #2 The keyring backend is cfg-gated per platform: Linux uses the Secret Service backend, macOS keeps apple-native-keyring-store — neither pulls the other's backend
- [x] #3 The static musl Linux release build (cargo-dist targets) still links — no new C/libdbus dependency (prefer a pure-Rust zbus secret-service backend)
- [x] #4 When no Secret Service is available at runtime, login fails with a clear message (no panic)
- [x] #5 docs/release.md (drop/adjust the 'Linux keychain isn't wired up' limitation), docs/architecture.md and docs/data-model.md (OS keychain notes) are updated
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Finding: keyring 4.1.2's default `v1` feature already auto-registers the pure-Rust zbus Secret Service store on Linux and apple-native on macOS; backend crates are target-gated inside keyring (verified via cargo tree --target linux-musl → zbus/secret-service, no dbus/libdbus; --target macos → apple only). No new backend crate needed.
2. Cargo.toml: replace the misleading unconditional `keyring = { features=[apple-native-keyring-store] }` with explicit per-target deps — macOS→apple-native, Linux→zbus-secret-service — documenting the zbus/musl rationale inline (AC #2, #3).
3. config.rs: map keyring failures so an unavailable/locked keychain (NoDefaultStore / NoStorageAccess — e.g. no Secret Service on Linux) yields a clear, actionable error; guaranteed no panic via the Result flow (AC #4). keyring::Error is #[non_exhaustive] so it can't be constructed in a unit test — covered by compile + review.
4. Docs (AC #5): drop the 'Linux keychain isn't wired up' limitation in docs/release.md; update README.md Linux note; update docs/architecture.md + docs/data-model.md keychain notes to Linux=Secret Service (zbus).
5. cargo fmt + clippy -D warnings + test (run suite repeatedly); confirm Cargo.lock unchanged. Commit + push; ask before PR.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented on branch task-24-linux-keychain.

Key finding: keyring 4.1.2 (already in the tree) supersedes the task's 'no Linux backend' premise. Its default `v1` feature auto-registers the platform store on first `Entry::new` (apple-native on macOS, pure-Rust zbus Secret Service on Linux), and the backend crates are target-gated inside keyring's own manifest — so no new backend crate was needed.

Changes:
- Cargo.toml: replaced the misleading unconditional `keyring = { features=[apple-native-keyring-store] }` with per-target deps — macOS→apple-native-keyring-store, Linux→zbus-secret-service-keyring-store, plus a `not(any(macos,linux))` fallback (`keyring = 4.1.2`) so other platforms (e.g. cargo install on Windows) still build via keyring's default backend.
- src/config.rs: added `keyring_error()` mapping NoDefaultStore/NoStorageAccess (no reachable/locked store — e.g. no Secret Service on Linux) to a clear, actionable hint; all keychain ops stay Result-based (no panic).
- Docs: dropped the 'Linux keychain isn't wired up' limitation in docs/release.md (now a runtime Secret-Service requirement); updated README.md Linux note, docs/architecture.md + docs/data-model.md keychain sections.

Verification (via cargo tree --target, since this host is macOS):
- Linux x86_64-unknown-linux-musl → keyring → zbus-secret-service-keyring-store only (no dbus/libdbus, no apple) → AC #1/#2/#3.
- macOS aarch64-apple-darwin → keyring → apple-native-keyring-store only → AC #2.
- Windows fallback → windows-native-keyring-store (no regression).
- cargo fmt/clippy -D warnings clean; 83 tests pass, stable x3; Cargo.lock unchanged.

AC #4 note: keyring::Error is #[non_exhaustive], so the error variant can't be constructed in a unit test, and CI (headless ubuntu) has no Secret Service to exercise the live path; the clear-message/no-panic behavior is covered by the Result-based flow + review rather than a bespoke test. Linux runtime persistence itself is not exercisable on this macOS host.

Added a real Linux runtime test (approved add-on, strengthens AC #1 verification): config::tests::keychain_round_trip_via_secret_service — #[ignore]d + #[cfg(target_os=linux)], stores/reads/deletes a unique-per-run password. New CI job 'linux-keychain' provisions gnome-keyring under dbus-run-session and runs it via 'cargo test -- --ignored'. Separate job (not a required check yet) so a keyring/D-Bus hiccup can't block lint-and-test; docs/ci.md documents promoting it once stable. Can't dry-run locally (no Docker on this macOS host + ring cross-compile blocks cargo check --target linux) — verifying via the CI run. docs/ci.md + architecture.md CI notes updated.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Wired up persistent Linux login via the OS Secret Service. keyring 4.1.2's default v1 feature already auto-selects the platform store, so the fix is per-target cfg-gating in Cargo.toml (macOS→apple-native-keyring-store, Linux→pure-Rust zbus-secret-service-keyring-store — no C libdbus for musl — plus a default-backend fallback for other platforms), a clear no-panic error when no keychain/Secret Service is reachable (config.rs keyring_error), and doc updates (release.md/README/architecture/data-model). Verified per-target with cargo tree (Linux=zbus only, macOS=apple only, Windows=windows-native fallback); fmt/clippy clean; 83 tests pass; Cargo.lock unchanged.
<!-- SECTION:FINAL_SUMMARY:END -->
