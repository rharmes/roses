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

    println!(
        "Ready to fetch entries for {} (next: TASK-3).",
        credentials.email
    );
    Ok(())
}
