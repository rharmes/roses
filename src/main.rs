//! roses — a TUI RSS reader, backed by Feedbin.
//!
//! Entry point and command dispatch:
//!   - `roses`         — launch the full-screen TUI (`tui`)
//!   - `roses list`    — print unread entries to stdout (`ui`)
//!   - `roses logout`  — clear stored credentials (`config`)
//!
//! Modules: `config` (credentials), `feedbin` (API client), `ui` (stdout
//! rendering), `tui` (ratatui app).

mod config;
mod feedbin;
mod tui;
mod ui;

use std::collections::HashMap;

use anyhow::{Result, anyhow};

use feedbin::Client;

/// How many of the newest unread entries `roses list` prints.
const DISPLAY_LIMIT: usize = 20;

fn main() -> Result<()> {
    match std::env::args().nth(1).as_deref() {
        None => run_tui(),
        Some("list") => run_list(),
        Some("logout") => {
            config::logout()?;
            println!("Logged out — stored Feedbin credentials cleared.");
            Ok(())
        }
        Some(other) => Err(anyhow!(
            "unknown command {other:?}. Usage: roses [list | logout]"
        )),
    }
}

/// Load stored credentials (or prompt for them on first run) and return an
/// authenticated Feedbin client.
fn connect() -> Result<Client> {
    let credentials = match config::load_credentials()? {
        Some(creds) => creds,
        None => {
            println!("Log in to your Feedbin account.");
            let creds = config::login()?;
            println!("Saved — email in your config file, password in the OS keychain.");
            creds
        }
    };
    let client = Client::new(&credentials)?;
    client.authenticate()?;
    Ok(client)
}

/// Launch the full-screen ratatui interface (TASK-6).
fn run_tui() -> Result<()> {
    let client = connect()?;
    tui::run(client)
}

/// Print a plain-text list of the newest unread entries to stdout — the headless
/// fallback to the TUI (TASK-4).
fn run_list() -> Result<()> {
    let client = connect()?;
    let mut unread = client.unread_entry_ids()?;
    let total_unread = unread.len();

    let (entries, feed_titles) = if unread.is_empty() {
        (Vec::new(), HashMap::new())
    } else {
        // Feedbin entry IDs grow over time, so the largest IDs are the newest.
        unread.sort_unstable_by(|a, b| b.cmp(a));
        let sample: Vec<i64> = unread.into_iter().take(DISPLAY_LIMIT).collect();
        let feed_titles = client.feed_titles()?;
        let mut entries = client.entries(&sample)?;
        entries.sort_by(|a, b| b.published.cmp(&a.published));
        (entries, feed_titles)
    };

    print!(
        "{}",
        ui::format_unread(&entries, &feed_titles, total_unread)
    );
    Ok(())
}
