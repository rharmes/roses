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
    /// Sent as the HTTP Basic auth password by the Feedbin client.
    pub password: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Settings {
    email: Option<String>,
    /// Command template to open article URLs (`%s`/`{url}` placeholder, else the
    /// URL is appended). Falls back to `$BROWSER`, then the platform opener.
    browser: Option<String>,
    /// Whether `browser` is a terminal browser (roses suspends the TUI for it).
    browser_terminal: Option<bool>,
}

/// The user's browser preference from config. `$BROWSER` and the platform
/// default opener are applied by the `browser` module, not here.
#[derive(Debug, Default, Clone)]
pub struct BrowserPref {
    /// User-configured command template, if any.
    pub command: Option<String>,
    /// Whether the configured browser runs inside the terminal.
    pub terminal: bool,
}

/// Pure (testable) XDG resolution: `$XDG_CONFIG_HOME/roses` if set and
/// non-empty, otherwise `~/.config/roses`.
fn config_dir_from(xdg_config_home: Option<&OsStr>, home: &Path) -> PathBuf {
    match xdg_config_home {
        Some(xdg) if !xdg.is_empty() => PathBuf::from(xdg).join(APP_NAME),
        _ => home.join(".config").join(APP_NAME),
    }
}

fn config_dir() -> Result<PathBuf> {
    let home = dirs::home_dir().context("could not determine the home directory")?;
    Ok(config_dir_from(
        std::env::var_os("XDG_CONFIG_HOME").as_deref(),
        &home,
    ))
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

/// Hint attached when a keychain operation fails because the store itself is
/// unavailable — most commonly on Linux when no Secret Service is reachable.
const KEYCHAIN_UNAVAILABLE_HINT: &str = "the OS keychain is unavailable — on Linux the Feedbin password is stored in the Secret \
     Service, so a keyring daemon (GNOME Keyring, KWallet, …) must be running and unlocked";

/// True when the failure is the keychain backend being absent or locked (rather
/// than a specific entry simply not existing), so we can attach an actionable
/// hint. keyring returns `NoDefaultStore` on Linux when no Secret Service could
/// be reached; `NoStorageAccess` covers a locked/blocked store.
fn keychain_unavailable(err: &keyring::Error) -> bool {
    matches!(
        err,
        keyring::Error::NoDefaultStore | keyring::Error::NoStorageAccess(_)
    )
}

/// Wrap a keyring failure with the operation context, plus the "keychain
/// unavailable" hint when that is the underlying cause. Keeps every keychain
/// path fallible (no panic) with a clear message.
fn keyring_error(op: &'static str, err: keyring::Error) -> anyhow::Error {
    let unavailable = keychain_unavailable(&err);
    let err = anyhow::Error::new(err).context(op);
    if unavailable {
        err.context(KEYCHAIN_UNAVAILABLE_HINT)
    } else {
        err
    }
}

fn keyring_entry(email: &str) -> Result<keyring::Entry> {
    keyring::Entry::new(APP_NAME, email)
        .map_err(|e| keyring_error("opening the OS keychain entry", e))
}

fn store_password(email: &str, password: &str) -> Result<()> {
    keyring_entry(email)?
        .set_password(password)
        .map_err(|e| keyring_error("storing the password in the OS keychain", e))
}

fn get_password(email: &str) -> Result<Option<String>> {
    match keyring_entry(email)?.get_password() {
        Ok(password) => Ok(Some(password)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(keyring_error(
            "reading the password from the OS keychain",
            e,
        )),
    }
}

fn delete_password(email: &str) -> Result<()> {
    match keyring_entry(email)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(keyring_error(
            "deleting the password from the OS keychain",
            e,
        )),
    }
}

/// Load stored credentials, if the user has logged in before. Returns `None`
/// when no email is recorded or the keychain has no matching entry.
pub fn load_credentials() -> Result<Option<Credentials>> {
    let Some(email) = load_settings()?.email else {
        return Ok(None);
    };
    Ok(get_password(&email)?.map(|password| Credentials { email, password }))
}

/// Load the user's browser preference from the config file (empty if unset).
pub fn load_browser_pref() -> Result<BrowserPref> {
    let settings = load_settings()?;
    Ok(BrowserPref {
        command: settings.browser,
        terminal: settings.browser_terminal.unwrap_or(false),
    })
}

/// Interactively prompt for an email and (hidden) password, store the password
/// in the OS keychain and the email in the TOML config, and return them.
pub fn login() -> Result<Credentials> {
    let email = prompt_line("Feedbin email: ")?;
    if email.is_empty() {
        return Err(anyhow!("email must not be empty"));
    }
    let password =
        rpassword::prompt_password("Feedbin password: ").context("reading the password")?;
    if password.is_empty() {
        return Err(anyhow!("password must not be empty"));
    }
    store_password(&email, &password)?;
    // Merge into existing settings so non-secret prefs (e.g. browser) survive.
    let mut settings = load_settings().unwrap_or_default();
    settings.email = Some(email.clone());
    save_settings(&settings)?;
    Ok(Credentials { email, password })
}

/// Clear the stored password from the keychain and forget the email, keeping
/// other settings (e.g. the browser preference) intact.
pub fn logout() -> Result<()> {
    let mut settings = load_settings().unwrap_or_default();
    if let Some(email) = settings.email.take() {
        delete_password(&email)?;
    }
    save_settings(&settings)
}

fn prompt_line(prompt: &str) -> Result<String> {
    print!("{prompt}");
    io::stdout().flush().context("flushing stdout")?;
    let mut line = String::new();
    io::stdin()
        .read_line(&mut line)
        .context("reading input from stdin")?;
    Ok(line.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_round_trip_through_toml() {
        let settings = Settings {
            email: Some("reader@example.com".to_string()),
            ..Default::default()
        };
        let text = toml::to_string_pretty(&settings).unwrap();
        let parsed: Settings = toml::from_str(&text).unwrap();
        assert_eq!(parsed.email.as_deref(), Some("reader@example.com"));
        // The serialized settings must never contain a password field.
        assert!(!text.to_lowercase().contains("password"));
    }

    #[test]
    fn empty_settings_have_no_email() {
        let parsed: Settings = toml::from_str("").unwrap();
        assert_eq!(parsed.email, None);
    }

    #[test]
    fn config_dir_uses_xdg_when_set() {
        assert_eq!(
            config_dir_from(Some(OsStr::new("/tmp/xdg")), Path::new("/home/ross")),
            PathBuf::from("/tmp/xdg/roses")
        );
    }

    #[test]
    fn config_dir_falls_back_to_dot_config() {
        assert_eq!(
            config_dir_from(None, Path::new("/home/ross")),
            PathBuf::from("/home/ross/.config/roses")
        );
    }

    #[test]
    fn config_dir_ignores_empty_xdg() {
        assert_eq!(
            config_dir_from(Some(OsStr::new("")), Path::new("/home/ross")),
            PathBuf::from("/home/ross/.config/roses")
        );
    }
}
