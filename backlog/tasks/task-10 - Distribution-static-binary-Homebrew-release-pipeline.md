---
id: TASK-10
title: 'Distribution: static binary + Homebrew release pipeline'
status: In Progress
assignee:
  - '@ross'
created_date: '2026-06-29 00:56'
updated_date: '2026-06-30 01:04'
labels:
  - rust
  - release
dependencies:
  - TASK-4
documentation:
  - docs/tui_research.md
priority: low
ordinal: 10
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Make roses installable as a single static binary. Build musl static Linux targets and macOS binaries, automate releases (e.g. cargo-dist) to GitHub Releases with a generated Homebrew formula, and publish to crates.io. See docs/tui_research.md sections 3.1 and 4.5.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Tagged releases produce static Linux (musl) and macOS binaries for x86_64 and aarch64
- [x] #2 A Homebrew tap formula installs the binary via brew install
- [x] #3 The release process is automated from a git tag (e.g. cargo-dist / GitHub Actions)
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
cargo-dist pipeline (decided with user). 1. dist-workspace.toml [dist]: cargo-dist-version 0.32.0, targets = x86_64/aarch64 -unknown-linux-musl + x86_64/aarch64 -apple-darwin (no Windows), installers [shell, homebrew], tap rharmes/homebrew-tap, publish-jobs [homebrew], pr-run-mode plan. 2. dist generate → .github/workflows/release.yml (tag vX.Y.Z* → build matrix + checksums + GitHub Release + push formula to tap via HOMEBREW_TAP_TOKEN; PRs run plan-only). 3. Hand-written .github/workflows/publish-crates.yml (final vX.Y.Z tag → cargo publish, CARGO_REGISTRY_TOKEN). 4. Cargo.toml homepage + [profile.dist]. 5. README install section + docs/release.md + CLAUDE/architecture/ci doc links. 6. User one-time prereqs (tap repo + 2 secrets + crates.io name), then cut v0.1.0.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented via cargo-dist 0.32.0. dist plan validates: announces v0.1.0 with 4 binary archives (musl x86_64/aarch64 + darwin x86_64/aarch64), roses-installer.sh, roses.rb (Homebrew), checksums. dist generate --check passes (release.yml in sync with dist-workspace.toml). release.yml: tags '**[0-9]+.[0-9]+.[0-9]+*' + PR plan job; publish-homebrew-formula checks out rharmes/homebrew-tap via HOMEBREW_TAP_TOKEN. publish-crates.yml: vX.Y.Z (final) → cargo publish --locked. cargo build --release ok; fmt/clippy/test green (70). ACs structurally satisfied by the pipeline + validated by dist plan; LIVE end-to-end verification (binaries built, brew install, cargo install) happens when v0.1.0 is tagged (after user sets up the tap repo + HOMEBREW_TAP_TOKEN + CARGO_REGISTRY_TOKEN secrets + confirms crates.io name). Known limitations documented (unsigned macOS → Gatekeeper; Linux keychain not wired; no Windows).
<!-- SECTION:NOTES:END -->
