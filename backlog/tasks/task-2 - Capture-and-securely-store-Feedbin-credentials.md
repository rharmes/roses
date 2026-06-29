---
id: TASK-2
title: Capture and securely store Feedbin credentials
status: Done
assignee:
  - '@claude'
created_date: '2026-06-29 00:56'
updated_date: '2026-06-29 14:05'
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
- [x] #1 First run prompts for email and password, with the password entry hidden (e.g. rpassword)
- [x] #2 Password is stored in the OS keychain via the keyring crate (macOS Keychain / Linux Secret Service)
- [x] #3 Email and non-secret settings are saved as TOML under XDG_CONFIG_HOME/roses (honoring the env var, default ~/.config)
- [x] #4 Subsequent runs load the credential from the keychain without re-prompting; a logout path can clear it
- [x] #5 No secret is written to the repo or any plaintext file; the config path is gitignored
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Add deps: keyring (OS keychain), rpassword (hidden prompt), serde+toml (config), dirs (home dir), anyhow (errors). 2. config.rs: XDG config dir honoring XDG_CONFIG_HOME (default ~/.config/roses); Settings TOML stores email only; keychain store/get/delete via keyring (service 'roses', user=email); login()/logout()/load_credentials(). 3. main.rs: load creds or prompt+store; 'roses logout' clears. 4. Defensively gitignore config.toml. 5. Verify fmt/build/clippy + a pure config unit test; user runs 'cargo run' to verify the interactive prompt + no-reprompt reload against their Feedbin account (interactive TTY needed, so that step is theirs).
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
=== TASK-2 implementation handoff (saved before a session restart) ===

STATUS: dependencies are added to Cargo.toml (anyhow, dirs, keyring w/ apple-native-keyring-store, rpassword, serde+derive, toml) and committed. The code below was DESIGNED BUT NEVER COMPILED — the Write tool was blocked by this background job's worktree-isolation guard. A fresh FOREGROUND `claude` session should recreate these files under src/, then run fmt/build/clippy/test and fix anything (especially keyring v4 method/enum names) before committing.

PoC decisions: password -> OS keychain via keyring; email + settings -> TOML under XDG ($XDG_CONFIG_HOME, default ~/.config/roses); first output is plain stdout (ratatui is TASK-6); the user has a live Feedbin account to verify against.

--- src/config.rs (DRAFT) ---
//! Configuration and credential storage.
//!
//! Non-secret settings (the logged-in Feedbin email) live in a TOML file under
//! the XDG config directory. The Feedbin password is stored separately in the
//! OS keychain via the keyring crate — never written to disk in plaintext.

use std::ffi::OsStr;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};

const APP_NAME: &str = "roses";

#[derive(Debug, Clone)]
pub struct Credentials {
    pub email: String,
    /// Sent as the HTTP Basic auth password by the Feedbin client (TASK-3).
    #[allow(dead_code)]
    pub password: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Settings {
    email: Option<String>,
}

/// Pure (testable) XDG resolution: $XDG_CONFIG_HOME/roses if set & non-empty, else ~/.config/roses.
fn config_dir_from(xdg_config_home: Option<&OsStr>, home: &Path) -> PathBuf {
    match xdg_config_home {
        Some(xdg) if !xdg.is_empty() => PathBuf::from(xdg).join(APP_NAME),
        _ => home.join(".config").join(APP_NAME),
    }
}

fn config_dir() -> Result<PathBuf> {
    let home = dirs::home_dir().context("could not determine the home directory")?;
    Ok(config_dir_from(std::env::var_os("XDG_CONFIG_HOME").as_deref(), &home))
}

fn config_path() -> Result<PathBuf> {
    Ok(config_dir()?.join("config.toml"))
}

fn load_settings() -> Result<Settings> {
    let path = config_path()?;
    if !path.exists() {
        return Ok(Settings::default());
    }
    let text = fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))
}

fn save_settings(settings: &Settings) -> Result<()> {
    let dir = config_dir()?;
    fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    let path = config_path()?;
    let text = toml::to_string_pretty(settings).context("serializing settings")?;
    fs::write(&path, text).with_context(|| format!("writing {}", path.display()))
}

fn keyring_entry(email: &str) -> Result<keyring::Entry> {
    keyring::Entry::new(APP_NAME, email).context("opening the OS keychain entry")
}

fn store_password(email: &str, password: &str) -> Result<()> {
    keyring_entry(email)?.set_password(password).context("storing the password in the OS keychain")
}

fn get_password(email: &str) -> Result<Option<String>> {
    match keyring_entry(email)?.get_password() {
        Ok(password) => Ok(Some(password)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(e).context("reading the password from the OS keychain"),
    }
}

fn delete_password(email: &str) -> Result<()> {
    match keyring_entry(email)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e).context("deleting the password from the OS keychain"),
    }
}

pub fn load_credentials() -> Result<Option<Credentials>> {
    let Some(email) = load_settings()?.email else { return Ok(None); };
    Ok(get_password(&email)?.map(|password| Credentials { email, password }))
}

pub fn login() -> Result<Credentials> {
    let email = prompt_line("Feedbin email: ")?;
    if email.is_empty() { return Err(anyhow!("email must not be empty")); }
    let password = rpassword::prompt_password("Feedbin password: ").context("reading the password")?;
    if password.is_empty() { return Err(anyhow!("password must not be empty")); }
    store_password(&email, &password)?;
    save_settings(&Settings { email: Some(email.clone()) })?;
    Ok(Credentials { email, password })
}

pub fn logout() -> Result<()> {
    if let Some(email) = load_settings()?.email { delete_password(&email)?; }
    save_settings(&Settings::default())
}

fn prompt_line(prompt: &str) -> Result<String> {
    print!("{prompt}");
    io::stdout().flush().context("flushing stdout")?;
    let mut line = String::new();
    io::stdin().read_line(&mut line).context("reading input from stdin")?;
    Ok(line.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn settings_round_trip_through_toml() {
        let settings = Settings { email: Some("reader@example.com".to_string()) };
        let text = toml::to_string_pretty(&settings).unwrap();
        let parsed: Settings = toml::from_str(&text).unwrap();
        assert_eq!(parsed.email.as_deref(), Some("reader@example.com"));
        assert!(!text.to_lowercase().contains("password"));
    }
    #[test]
    fn empty_settings_have_no_email() {
        let parsed: Settings = toml::from_str("").unwrap();
        assert_eq!(parsed.email, None);
    }
    #[test]
    fn config_dir_uses_xdg_when_set() {
        assert_eq!(config_dir_from(Some(OsStr::new("/tmp/xdg")), Path::new("/home/ross")), PathBuf::from("/tmp/xdg/roses"));
    }
    #[test]
    fn config_dir_falls_back_to_dot_config() {
        assert_eq!(config_dir_from(None, Path::new("/home/ross")), PathBuf::from("/home/ross/.config/roses"));
    }
    #[test]
    fn config_dir_ignores_empty_xdg() {
        assert_eq!(config_dir_from(Some(OsStr::new("")), Path::new("/home/ross")), PathBuf::from("/home/ross/.config/roses"));
    }
}

--- src/main.rs (DRAFT) ---
//! roses — a TUI RSS reader, backed by Feedbin.

mod config;
mod feedbin;
mod ui;

use anyhow::Result;

fn main() -> Result<()> {
    if std::env::args().nth(1).as_deref() == Some("logout") {
        config::logout()?;
        println!("Logged out — stored Feedbin credentials cleared.");
        return Ok(());
    }
    let credentials = match config::load_credentials()? {
        Some(creds) => { println!("Welcome back, {}.", creds.email); creds }
        None => {
            println!("Log in to your Feedbin account.");
            let creds = config::login()?;
            println!("Saved — email in your config file, password in the OS keychain.");
            creds
        }
    };
    println!("Ready to fetch entries for {} (next: TASK-3).", credentials.email);
    Ok(())
}

--- .gitignore: append ---
# Defensive: the real config lives in ~/.config/roses, never the repo
config.toml

--- VERIFY ---
1. source "$HOME/.cargo/env"
2. cargo fmt && cargo fmt --check
3. cargo build
4. cargo clippy --all-targets -- -D warnings
5. cargo test  (unit tests cover AC#2 keychain code paths indirectly, AC#3 XDG/TOML, AC#5 no-secret-in-TOML)
6. USER runs interactively (needs a TTY + the live account; the agent cannot drive rpassword):
   - cargo run         -> prompts email + hidden password, stores them      (AC#1)
   - cargo run         -> "Welcome back, <email>" with NO re-prompt          (AC#4)
   - cargo run logout  -> clears the keychain entry and forgets the email
7. Check ACs, write final summary, commit on rust-poc, mark TASK-2 Done, then start TASK-3.

=== Implementation landed on rust-poc (this session) ===

Recreated the drafted files directly in the checkout (bg-isolation guard disabled with user approval for this session):
- src/config.rs — full implementation: Credentials/Settings types; pure config_dir_from() XDG resolution; load/save TOML; keyring store/get/delete; load_credentials()/login()/logout(); prompt_line(); 5 unit tests.
- src/main.rs — entry point: 'logout' subcommand, load-or-prompt flow, friendly status lines.
- .gitignore — appended 'config.toml' (defensive).

keyring v4 API confirmed by compilation: Entry::new, set_password, get_password, delete_credential(), Error::NoEntry — all correct, no changes needed from the draft.

Automated verification (all pass):
- cargo fmt --check: clean
- cargo build: clean
- cargo clippy --all-targets -- -D warnings: no warnings
- cargo test: 5/5 pass (XDG default+override, empty settings, TOML round-trip asserting no 'password' field)
- Smoke test: XDG_CONFIG_HOME=<tmp> cargo run -- logout -> prints logout msg, writes an EMPTY config.toml under the temp XDG dir, leaves real ~/.config/roses untouched.

AC status: #3 (XDG/TOML) and #5 (no secret on disk, gitignored) VERIFIED and checked.
#1 (interactive email + hidden password prompt), #2 (live keychain store), #4 (no-reprompt reload + logout-clears) are IMPLEMENTED but need an interactive TTY + the live Feedbin account to verify end-to-end — that's the user's step. Once that passes, check #1/#2/#4 and mark Done, then start TASK-3.

User verified the interactive flow against their live Feedbin account (2026-06-29): AC#1 first run prompts email + hidden password; AC#2 password lands in the macOS Keychain via keyring; AC#4 second run reloads from the keychain with NO re-prompt and 'roses logout' clears the entry. All 5 ACs now pass.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Implemented Feedbin credential capture + secure storage. src/config.rs stores the email and non-secret settings as TOML under XDG_CONFIG_HOME/roses (default ~/.config/roses) and the password in the OS keychain via keyring v4; exposes login()/logout()/load_credentials() with a pure, unit-tested XDG resolver. src/main.rs does load-or-prompt with a 'logout' subcommand. config.toml is gitignored and a unit test asserts the settings TOML never carries a password. Verified: cargo fmt --check, build, clippy -D warnings, 5 unit tests, plus the user's interactive run (prompt -> keychain store -> no-reprompt reload -> logout clears). Committed on rust-poc (8cc2276).
<!-- SECTION:FINAL_SUMMARY:END -->
