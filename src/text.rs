//! Shared text hardening for display fields.
//!
//! A hostile feed can embed ANSI/terminal escape sequences in fields we render
//! verbatim — titles, authors, URLs, feed names — to smuggle cursor moves,
//! color, or clipboard writes onto the user's terminal. The reader *body* is
//! already defused by `tui::sanitize`, which keeps newlines/tabs for multi-line
//! layout. `strip_control_chars` is the single-line counterpart for header and
//! list fields: it drops every control character (there is no legitimate one in
//! a title or URL).

/// Remove all Unicode control characters from `s`. `char::is_control` covers C0
/// (0x00–0x1F, including ESC 0x1B — the lead byte of terminal escape
/// sequences), DEL (0x7F), and C1 (0x80–0x9F). Newline and tab are control
/// characters too and are dropped, since these are single-line display fields.
pub fn strip_control_chars(s: &str) -> String {
    s.chars().filter(|c| !c.is_control()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_esc_bel_and_newlines_keeping_visible_text() {
        // An ANSI colour sequence plus a BEL and a trailing newline: the ESC and
        // BEL bytes vanish, so the sequence renders as inert visible text.
        let hostile = "red\x1b[31mtext\x07\n";
        assert_eq!(strip_control_chars(hostile), "red[31mtext");
    }

    #[test]
    fn leaves_plain_unicode_untouched() {
        assert_eq!(
            strip_control_chars("Plain — title, 42% ✓"),
            "Plain — title, 42% ✓"
        );
    }
}
