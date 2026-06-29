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

fn keyring_entry(email: &str) -> Result<keyring::Entry> {
    keyring::Entry::new(APP_NAME, email).context("opening the OS keychain entry")
}

fn store_password(email: &str, password: &str) -> Result<()> {
    keyring_entry(email)?
        .set_password(password)
        .context("storing the password in the OS keychain")
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

/// Load stored credentials, if the user has logged in before. Returns `None`
/// when no email is recorded or the keychain has no matching entry.
pub fn load_credentials() -> Result<Option<Credentials>> {
    let Some(email) = load_settings()?.email else {
        return Ok(None);
    };
    Ok(get_password(&email)?.map(|password| Credentials { email, password }))
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
    save_settings(&Settings {
        email: Some(email.clone()),
    })?;
    Ok(Credentials { email, password })
}

/// Clear the stored password from the keychain and forget the email.
pub fn logout() -> Result<()> {
    if let Some(email) = load_settings()?.email {
        delete_password(&email)?;
    }
    save_settings(&Settings::default())
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
