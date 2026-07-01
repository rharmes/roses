//! Centralized rose palette (TASK-14): one cohesive accent plus a small gradient
//! ramp for the "all caught up" rose. Colors are truecolor `Rgb` (already used by
//! `images.rs`); non-truecolor terminals downsample to the nearest pink, and the
//! `bold`/`dim`/`reversed` modifiers at the call sites still carry focus and
//! selection, so the UI degrades gracefully.

use ratatui::style::Color;

/// Primary accent — focused chrome, selection, reader title, footer keys.
pub const ROSE: Color = Color::Rgb(0xE0, 0x6C, 0x9A);
/// Muted neutral grey the focus/selection accent recedes to while the help
/// overlay is open, so the overlay itself draws the eye (TASK-46).
pub const MUTED: Color = Color::Rgb(0x80, 0x80, 0x80);
/// Top of the petal gradient (lightest).
pub const ROSE_LIGHT: Color = Color::Rgb(0xF2, 0xA9, 0xC4);
/// Bottom of the petal gradient (deepest).
pub const ROSE_DEEP: Color = Color::Rgb(0xB0, 0x39, 0x5B);
/// Green stem/leaf in the rose art.
pub const LEAF: Color = Color::Rgb(0x6B, 0x9E, 0x78);

/// Parse a hex color into an `Rgb`, for the configurable accent (TASK-45).
/// Accepts `#rrggbb`/`rrggbb` and the 3-digit shorthand `#rgb`/`rgb` (each digit
/// doubled, so `#f00` → `#ff0000`), case-insensitive, with an optional leading
/// `#` and surrounding whitespace. Returns `None` for anything malformed (wrong
/// length, non-hex digits) so callers can fall back to the [`ROSE`] default.
pub fn parse_hex(s: &str) -> Option<Color> {
    let s = s.trim();
    let s = s.strip_prefix('#').unwrap_or(s);
    if !s.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let hex = |slice: &str| u8::from_str_radix(slice, 16).ok();
    let (r, g, b) = match s.len() {
        6 => (hex(&s[0..2])?, hex(&s[2..4])?, hex(&s[4..6])?),
        // Shorthand: a single digit `d` expands to `dd` = d*17 (0xF → 0xFF).
        3 => {
            let d = |i: usize| hex(&s[i..i + 1]).map(|v| v * 17);
            (d(0)?, d(1)?, d(2)?)
        }
        _ => return None,
    };
    Some(Color::Rgb(r, g, b))
}

/// Component-wise linear interpolation between two `Rgb` colors; `t` is clamped to
/// `0.0..=1.0`. Non-`Rgb` inputs fall back to `a` (the palette only feeds `Rgb`).
pub fn lerp(a: Color, b: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    match (a, b) {
        (Color::Rgb(ar, ag, ab), Color::Rgb(br, bg, bb)) => {
            let mix =
                |x: u8, y: u8| (f32::from(x) + (f32::from(y) - f32::from(x)) * t).round() as u8;
            Color::Rgb(mix(ar, br), mix(ag, bg), mix(ab, bb))
        }
        _ => a,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_hex_accepts_six_and_three_digit_forms() {
        let rose = Color::Rgb(0xE0, 0x6C, 0x9A);
        assert_eq!(parse_hex("#e06c9a"), Some(rose), "with leading #");
        assert_eq!(parse_hex("e06c9a"), Some(rose), "without #");
        assert_eq!(parse_hex("E06C9A"), Some(rose), "case-insensitive");
        assert_eq!(
            parse_hex("  #E06c9A  "),
            Some(rose),
            "surrounding whitespace"
        );
        // 3-digit shorthand doubles each digit.
        assert_eq!(parse_hex("#f00"), Some(Color::Rgb(0xFF, 0x00, 0x00)));
        assert_eq!(parse_hex("0f0"), Some(Color::Rgb(0x00, 0xFF, 0x00)));
        assert_eq!(parse_hex("#abc"), Some(Color::Rgb(0xAA, 0xBB, 0xCC)));
    }

    #[test]
    fn parse_hex_rejects_malformed_input() {
        for bad in [
            "",
            "#",
            "12345",
            "1234567",
            "#12",
            "#gg0000",
            "xyzxyz",
            "#e06c9",
            "rgb(1,2,3)",
        ] {
            assert_eq!(parse_hex(bad), None, "should reject {bad:?}");
        }
    }

    #[test]
    fn lerp_hits_both_ends_and_the_midpoint() {
        assert_eq!(lerp(ROSE_LIGHT, ROSE_DEEP, 0.0), ROSE_LIGHT);
        assert_eq!(lerp(ROSE_LIGHT, ROSE_DEEP, 1.0), ROSE_DEEP);
        // Out-of-range t clamps rather than extrapolating.
        assert_eq!(lerp(ROSE_LIGHT, ROSE_DEEP, 2.0), ROSE_DEEP);
        assert_eq!(lerp(ROSE_LIGHT, ROSE_DEEP, -1.0), ROSE_LIGHT);
        // Midpoint is the component-wise average.
        let Color::Rgb(r, g, b) = lerp(Color::Rgb(0, 0, 0), Color::Rgb(10, 20, 30), 0.5) else {
            panic!("expected rgb");
        };
        assert_eq!((r, g, b), (5, 10, 15));
    }
}
