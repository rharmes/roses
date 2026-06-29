//! roses — a TUI RSS reader, backed by Feedbin.
//!
//! Application entry point. The proof-of-concept tasks build it out:
//!   - `config`  — capture and store Feedbin credentials (TASK-2)
//!   - `feedbin` — query the Feedbin API (TASK-3)
//!   - `ui`      — display fetched entries (TASK-4)

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
    println!("Authenticated with Feedbin as {}.", credentials.email);

    let unread = client.unread_entry_ids()?;
    println!("{} unread entries.", unread.len());

    // Hydrate a small batch as a smoke test; rich rendering arrives in TASK-4.
    let batch: Vec<i64> = unread.iter().copied().take(20).collect();
    let entries = client.entries(&batch)?;
    println!(
        "Fetched {} entries (display lands in TASK-4).",
        entries.len()
    );
    Ok(())
}
