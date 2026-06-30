//! User-facing output.
//!
//! Starts as plain stdout rendering of fetched entries (TASK-4) and grows into
//! the full-screen ratatui interface (TASK-6). Rendering is kept as a pure
//! `String`-producing function so it can be unit-tested without a terminal.

use std::collections::HashMap;
use std::fmt::Write;

use crate::feedbin::Entry;

const NO_TITLE: &str = "(untitled)";
const NO_FEED: &str = "(unknown feed)";

/// Render a readable plain-text list of unread entries.
///
/// `entries` is the (already trimmed/sorted) batch to show; `feed_titles` maps
/// `feed_id` to a feed name; `total_unread` is the full unread count so the
/// header can say "showing X of Y". When nothing is unread, returns a friendly
/// all-caught-up message instead of an empty list (AC #3).
pub fn format_unread(
    entries: &[Entry],
    feed_titles: &HashMap<i64, String>,
    total_unread: usize,
) -> String {
    if total_unread == 0 {
        return "You're all caught up — no unread entries.\n".to_string();
    }

    let mut out = String::new();
    let _ = writeln!(
        out,
        "Unread entries (showing {} of {}):\n",
        entries.len(),
        total_unread
    );

    for (i, entry) in entries.iter().enumerate() {
        let title = entry.title.as_deref().unwrap_or(NO_TITLE);
        let feed = feed_titles
            .get(&entry.feed_id)
            .map(String::as_str)
            .unwrap_or(NO_FEED);
        let _ = writeln!(out, "{}. {title}", i + 1);
        match entry.url.as_deref() {
            Some(url) => {
                let _ = writeln!(out, "   {feed} · {url}");
            }
            None => {
                let _ = writeln!(out, "   {feed}");
            }
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(feed_id: i64, title: Option<&str>, url: Option<&str>) -> Entry {
        Entry {
            id: 1,
            feed_id,
            title: title.map(str::to_string),
            url: url.map(str::to_string),
            author: None,
            published: None,
            summary: None,
            content: None,
        }
    }

    #[test]
    fn empty_unread_is_friendly() {
        let out = format_unread(&[], &HashMap::new(), 0);
        assert!(
            out.to_lowercase().contains("caught up"),
            "expected a friendly empty message, got: {out}"
        );
    }

    #[test]
    fn lists_titles_with_feed_names_and_a_count() {
        let mut feeds = HashMap::new();
        feeds.insert(7, "Rust Blog".to_string());
        let entries = vec![
            entry(7, Some("Releasing 1.0"), Some("https://example.com/1")),
            entry(99, None, None), // unknown feed + missing title -> placeholders
        ];

        let out = format_unread(&entries, &feeds, 42);

        assert!(out.contains("showing 2 of 42"), "{out}");
        assert!(out.contains("Releasing 1.0"), "{out}");
        assert!(out.contains("Rust Blog"), "{out}");
        assert!(out.contains("https://example.com/1"), "{out}");
        assert!(out.contains(NO_TITLE), "{out}");
        assert!(out.contains(NO_FEED), "{out}");
    }

    #[test]
    fn entry_without_url_omits_the_separator() {
        let mut feeds = HashMap::new();
        feeds.insert(7, "Rust Blog".to_string());
        let out = format_unread(&[entry(7, Some("No link here"), None)], &feeds, 1);
        assert!(out.contains("No link here"), "{out}");
        assert!(out.contains("Rust Blog"), "{out}");
        assert!(!out.contains('·'), "no URL means no ' · ' separator: {out}");
    }
}
