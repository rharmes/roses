//! roses — a TUI RSS reader, backed by Feedbin.
//!
//! Application entry point. The proof-of-concept tasks build it out:
//!   - `config`  — capture and store Feedbin credentials (TASK-2)
//!   - `feedbin` — query the Feedbin API (TASK-3)
//!   - `ui`      — display fetched entries (TASK-4)

mod config;
mod feedbin;
mod ui;

use std::collections::HashMap;

use anyhow::Result;

/// How many of the newest unread entries to render in the plain-text list.
const DISPLAY_LIMIT: usize = 20;

fn main() -> Result<()> {
    if std::env::args().nth(1).as_deref() == Some("logout") {
        config::logout()?;
        println!("Logged out — stored Feedbin credentials cleared.");
        return Ok(());
    }

    let credentials = match config::load_credentials()? {
        Some(creds) => {
            println!("Welcome back, {}.", creds.email);
            creds
        }
        None => {
            println!("Log in to your Feedbin account.");
            let creds = config::login()?;
            println!("Saved — email in your config file, password in the OS keychain.");
            creds
        }
    };

    let client = feedbin::Client::new(&credentials)?;
    client.authenticate()?;
    println!("Authenticated with Feedbin as {}.\n", credentials.email);

    let mut unread = client.unread_entry_ids()?;
    let total_unread = unread.len();

    let (entries, feed_titles) = if unread.is_empty() {
        (Vec::new(), HashMap::new())
    } else {
        // Feedbin entry IDs grow over time, so the largest IDs are the newest;
        // show a readable sample of those.
        unread.sort_unstable_by(|a, b| b.cmp(a));
        let sample: Vec<i64> = unread.into_iter().take(DISPLAY_LIMIT).collect();

        let feed_titles = client.feed_titles()?;
        let mut entries = client.entries(&sample)?;
        // entries.json need not preserve the requested order; show newest first.
        entries.sort_by(|a, b| b.published.cmp(&a.published));
        (entries, feed_titles)
    };

    print!(
        "{}",
        ui::format_unread(&entries, &feed_titles, total_unread)
    );
    Ok(())
}
