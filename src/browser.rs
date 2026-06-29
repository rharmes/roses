//! Opening article URLs in the user's chosen browser (TASK-9).
//!
//! Precedence: the config `browser` command template, then `$BROWSER`, then the
//! platform default (`open` on macOS, `xdg-open` elsewhere). A template may use
//! a `%s`/`{url}` placeholder; otherwise the URL is appended as the last
//! argument. Terminal browsers are flagged so the caller can suspend the TUI
//! around them (`ratatui` alt-screen + raw mode).

use std::process::Command;

use anyhow::{Result, bail};

use crate::config::BrowserPref;

/// A resolved browser invocation.
#[derive(Debug, PartialEq, Eq)]
pub struct Launch {
    pub program: String,
    pub args: Vec<String>,
    /// Whether this browser takes over the terminal (suspend the TUI for it).
    pub terminal: bool,
}

fn default_opener() -> &'static str {
    if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    }
}

/// Resolve how to open `url`. Pure (env is passed in) so it can be unit-tested.
pub fn resolve(pref: &BrowserPref, env_browser: Option<&str>, url: &str) -> Launch {
    if let Some(command) = pref
        .command
        .as_deref()
        .map(str::trim)
        .filter(|c| !c.is_empty())
    {
        return from_template(command, url, pref.terminal);
    }
    // `$BROWSER` is a colon-separated list of templates; use the first entry.
    if let Some(first) =
        env_browser.and_then(|env| env.split(':').map(str::trim).find(|e| !e.is_empty()))
    {
        return from_template(first, url, false);
    }
    Launch {
        program: default_opener().to_string(),
        args: vec![url.to_string()],
        terminal: false,
    }
}

/// Split a command template into argv, substituting `url` for a `%s`/`{url}`
/// placeholder or appending it when no placeholder is present.
fn from_template(template: &str, url: &str, terminal: bool) -> Launch {
    let tokens = shlex::split(template)
        .unwrap_or_else(|| template.split_whitespace().map(String::from).collect());

    let mut args = Vec::with_capacity(tokens.len() + 1);
    let mut substituted = false;
    for token in tokens {
        if token.contains("%s") || token.contains("{url}") {
            args.push(token.replace("%s", url).replace("{url}", url));
            substituted = true;
        } else {
            args.push(token);
        }
    }
    if args.is_empty() {
        // Degenerate template — fall back to the platform opener.
        return Launch {
            program: default_opener().to_string(),
            args: vec![url.to_string()],
            terminal,
        };
    }
    if !substituted {
        args.push(url.to_string());
    }
    let program = args.remove(0);
    Launch {
        program,
        args,
        terminal,
    }
}

/// Launch the browser. A terminal browser is run in the foreground and waited
/// on (the caller must have suspended the TUI); a GUI browser is spawned and
/// detached.
pub fn run(launch: &Launch) -> Result<()> {
    let mut command = Command::new(&launch.program);
    command.args(&launch.args);
    if launch.terminal {
        let status = command
            .status()
            .map_err(|e| anyhow::anyhow!("could not run browser `{}`: {e}", launch.program))?;
        if !status.success() {
            bail!("browser `{}` exited with {status}", launch.program);
        }
    } else {
        command
            .spawn()
            .map_err(|e| anyhow::anyhow!("could not launch browser `{}`: {e}", launch.program))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pref(command: Option<&str>, terminal: bool) -> BrowserPref {
        BrowserPref {
            command: command.map(str::to_string),
            terminal,
        }
    }

    #[test]
    fn default_uses_the_platform_opener() {
        let launch = resolve(&pref(None, false), None, "https://example.com/a");
        assert_eq!(launch.args, vec!["https://example.com/a".to_string()]);
        assert!(!launch.terminal);
        assert!(matches!(launch.program.as_str(), "open" | "xdg-open"));
    }

    #[test]
    fn config_template_substitutes_placeholder() {
        let launch = resolve(&pref(Some("w3m %s"), true), None, "https://example.com/a");
        assert_eq!(launch.program, "w3m");
        assert_eq!(launch.args, vec!["https://example.com/a".to_string()]);
        assert!(launch.terminal, "config-marked terminal browser");
    }

    #[test]
    fn config_template_without_placeholder_appends_url() {
        let launch = resolve(&pref(Some("firefox"), false), None, "https://example.com/a");
        assert_eq!(launch.program, "firefox");
        assert_eq!(launch.args, vec!["https://example.com/a".to_string()]);
    }

    #[test]
    fn config_template_respects_quoted_arguments() {
        let launch = resolve(
            &pref(Some("open -a 'Google Chrome' %s"), false),
            None,
            "https://example.com/a",
        );
        assert_eq!(launch.program, "open");
        assert_eq!(
            launch.args,
            vec![
                "-a".to_string(),
                "Google Chrome".to_string(),
                "https://example.com/a".to_string()
            ]
        );
    }

    #[test]
    fn brace_placeholder_also_works() {
        let launch = resolve(
            &pref(Some("links {url}"), true),
            None,
            "https://example.com/a",
        );
        assert_eq!(launch.program, "links");
        assert_eq!(launch.args, vec!["https://example.com/a".to_string()]);
    }

    #[test]
    fn env_browser_used_when_no_config() {
        let launch = resolve(
            &pref(None, false),
            Some("/usr/bin/qutebrowser %s"),
            "https://x/a",
        );
        assert_eq!(launch.program, "/usr/bin/qutebrowser");
        assert_eq!(launch.args, vec!["https://x/a".to_string()]);
        assert!(!launch.terminal, "$BROWSER is treated as a GUI launch");
    }

    #[test]
    fn config_command_takes_precedence_over_env() {
        let launch = resolve(
            &pref(Some("lynx %s"), true),
            Some("firefox %s"),
            "https://x/a",
        );
        assert_eq!(launch.program, "lynx");
    }

    #[test]
    fn env_browser_uses_first_list_entry() {
        let launch = resolve(
            &pref(None, false),
            Some("firefox %s:chromium %s"),
            "https://x/a",
        );
        assert_eq!(launch.program, "firefox");
    }
}
