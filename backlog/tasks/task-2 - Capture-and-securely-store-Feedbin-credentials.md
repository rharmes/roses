---
id: TASK-2
title: Capture and securely store Feedbin credentials
status: To Do
assignee: []
created_date: '2026-06-29 00:56'
labels:
  - poc
  - rust
dependencies:
  - TASK-1
documentation:
  - docs/tui_research.md
priority: high
ordinal: 2
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Prompt for the user's Feedbin email and password on first run and store them safely: the password in the OS keychain via the keyring crate, the email and non-secret settings in a TOML file under the XDG config dir. Feedbin uses HTTP Basic auth (email + password) on every request, so the password must be retrievable locally but never written to plaintext.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 First run prompts for email and password, with the password entry hidden (e.g. rpassword)
- [ ] #2 Password is stored in the OS keychain via the keyring crate (macOS Keychain / Linux Secret Service)
- [ ] #3 Email and non-secret settings are saved as TOML under XDG_CONFIG_HOME/roses (honoring the env var, default ~/.config)
- [ ] #4 Subsequent runs load the credential from the keychain without re-prompting; a logout path can clear it
- [ ] #5 No secret is written to the repo or any plaintext file; the config path is gitignored
<!-- AC:END -->
