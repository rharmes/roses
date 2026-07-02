//! roses — a TUI RSS reader, backed by Feedbin.
//!
//! Entry point and command dispatch:
//!   - `roses`             — launch the full-screen TUI (`tui`)
//!   - `roses list`        — print unread entries to stdout (`ui`)
//!   - `roses export [FILE]` — export subscriptions as OPML (`opml`)
//!   - `roses import FILE` — import subscriptions from an OPML file (`feedbin`)
//!   - `roses logout`      — clear stored credentials (`config`)
//!
//! Modules: `config` (credentials), `feedbin` (API client), `ui` (stdout
//! rendering), `tui` (ratatui app), `opml` (OPML export).

mod browser;
mod config;
mod feedbin;
mod images;
mod opml;
mod store;
mod text;
mod theme;
mod tui;
mod ui;

use std::collections::HashMap;
use std::time::Duration;

use anyhow::{Context, Result, anyhow};

use feedbin::Client;

/// How many of the newest unread entries `roses list` prints.
const DISPLAY_LIMIT: usize = 20;

fn main() -> Result<()> {
    match std::env::args().nth(1).as_deref() {
        None => run_tui(),
        Some("list") => run_list(),
        // OPML export/import for bulk feed migration (TASK-38). Export takes an
        // optional path (stdout when omitted); import requires the OPML file.
        Some("export") => run_export(std::env::args().nth(2)),
        Some("import") => run_import(std::env::args().nth(2)),
        Some("logout") => {
            config::logout()?;
            println!("Logged out — stored Feedbin credentials cleared.");
            Ok(())
        }
        // Render the "all caught up" screen offline (no login/network) so the
        // empty state can be previewed without marking everything read.
        Some("preview") => tui::run_preview(),
        Some(other) => Err(anyhow!(
            "unknown command {other:?}. Usage: roses [list | export [FILE] | import FILE | logout | preview]"
        )),
    }
}

/// Load stored credentials (or prompt for them on first run) and return a
/// Feedbin client. The credentials are **not** validated here: the TUI paints
/// from its offline cache first and reconciles in the background (TASK-41), so a
/// bad-password 401 or an offline box surfaces as an in-app notice rather than
/// blocking startup. (`roses list` still hits the network immediately, so it
/// reports auth errors on its first request.)
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
    Client::new(&credentials)
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

/// Export the current Feedbin subscriptions as an OPML document (TASK-38). Writes
/// to `path`, or to stdout when it's `None` — status lines go to stderr so a
/// piped OPML document on stdout stays clean.
fn run_export(path: Option<String>) -> Result<()> {
    let client = connect()?;
    let subscriptions = client.subscriptions()?;
    let total = subscriptions.len();
    // A subscription without a feed URL can't be a valid OPML outline, so skip
    // it; fall back to the feed URL as the label when a feed has no title.
    let mut feeds: Vec<opml::OpmlFeed> = subscriptions
        .into_iter()
        .filter_map(|s| {
            let xml_url = s.feed_url?;
            let text = s
                .title
                .filter(|t| !t.trim().is_empty())
                .unwrap_or_else(|| xml_url.clone());
            Some(opml::OpmlFeed {
                text,
                xml_url,
                html_url: s.site_url,
            })
        })
        .collect();
    // Deterministic, human-friendly order (case-insensitive by label).
    feeds.sort_by_key(|a| a.text.to_lowercase());
    let skipped = total - feeds.len();
    let document = opml::to_opml("roses subscriptions", &feeds);
    let plural = if feeds.len() == 1 { "" } else { "s" };

    match path {
        Some(path) => {
            std::fs::write(&path, &document).with_context(|| format!("writing OPML to {path}"))?;
            eprintln!("Exported {} subscription{plural} to {path}.", feeds.len());
        }
        None => {
            print!("{document}");
            eprintln!("Exported {} subscription{plural}.", feeds.len());
        }
    }
    if skipped > 0 {
        eprintln!("Skipped {skipped} feed(s) with no feed URL.");
    }
    Ok(())
}

/// How often to poll a running import, and how long to wait before giving up on
/// the wait (the import keeps running server-side). Feedbin imports typically
/// finish within seconds to a couple of minutes.
const IMPORT_POLL_INTERVAL: Duration = Duration::from_secs(2);
const IMPORT_POLL_MAX: u32 = 150; // ~5 minutes at a 2s cadence.

/// Import subscriptions from an OPML file via Feedbin, polling to completion and
/// printing a summary of complete/failed feeds (TASK-38).
fn run_import(path: Option<String>) -> Result<()> {
    let path = path.ok_or_else(|| anyhow!("Usage: roses import FILE.opml"))?;
    let opml = std::fs::read(&path).with_context(|| format!("reading {path}"))?;
    if opml.iter().all(u8::is_ascii_whitespace) {
        return Err(anyhow!("{path} is empty — nothing to import."));
    }
    let client = connect()?;
    eprintln!("Uploading {path}…");
    let mut import = client.create_import(&opml)?;
    let count = import.import_items.len();
    if count > 0 {
        let plural = if count == 1 { "" } else { "s" };
        eprintln!("Importing {count} feed{plural}…");
    }

    // Feedbin processes the import asynchronously; poll until it reports complete
    // (or we hit the cap, in which case it's still running server-side).
    let mut polls = 0;
    while !import.complete && polls < IMPORT_POLL_MAX {
        std::thread::sleep(IMPORT_POLL_INTERVAL);
        import = client.import_status(import.id)?;
        polls += 1;
    }

    let tally = import.tally();
    if import.complete {
        println!(
            "Done: {} complete, {} failed.",
            tally.complete, tally.failed
        );
    } else {
        let waited = IMPORT_POLL_MAX as u64 * IMPORT_POLL_INTERVAL.as_secs();
        println!(
            "Still processing after {waited}s: {} complete, {} pending, {} failed. \
             Re-run later or check Feedbin (import #{}).",
            tally.complete, tally.pending, tally.failed, import.id
        );
    }
    for url in &tally.failed_urls {
        println!("  failed: {url}");
    }
    Ok(())
}
