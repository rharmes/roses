//! roses — a TUI RSS reader, backed by Feedbin.
//!
//! Application entry point. This is currently a scaffold; the proof-of-concept
//! tasks build it out:
//!   - `config`  — capture and store Feedbin credentials (TASK-2)
//!   - `feedbin` — query the Feedbin API (TASK-3)
//!   - `ui`      — display fetched entries (TASK-4)

mod config;
mod feedbin;
mod ui;

fn main() {
    println!("roses — a TUI RSS reader, backed by Feedbin.");
    println!("Scaffold only — see backlog tasks TASK-2..4 for the proof-of-concept.");
}
