---
id: TASK-24
title: Add Linux OS keychain support (Secret Service backend)
status: To Do
assignee:
  - '@ross'
created_date: '2026-06-30 21:19'
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
- [ ] #1 On Linux, the Feedbin password is stored in and read from the OS Secret Service, so login persists across runs (service 'roses', username = email, same as macOS)
- [ ] #2 The keyring backend is cfg-gated per platform: Linux uses the Secret Service backend, macOS keeps apple-native-keyring-store — neither pulls the other's backend
- [ ] #3 The static musl Linux release build (cargo-dist targets) still links — no new C/libdbus dependency (prefer a pure-Rust zbus secret-service backend)
- [ ] #4 When no Secret Service is available at runtime, login fails with a clear message (no panic)
- [ ] #5 docs/release.md (drop/adjust the 'Linux keychain isn't wired up' limitation), docs/architecture.md and docs/data-model.md (OS keychain notes) are updated
<!-- AC:END -->
