//! Centralized rose palette (TASK-14): one cohesive accent plus a small gradient
//! ramp for the "all caught up" rose. Colors are truecolor `Rgb` (already used by
//! `images.rs`); non-truecolor terminals downsample to the nearest pink, and the
//! `bold`/`dim`/`reversed` modifiers at the call sites still carry focus and
//! selection, so the UI degrades gracefully.

use ratatui::style::Color;

/// Primary accent — focused chrome, selection, reader title, footer keys.
pub const ROSE: Color = Color::Rgb(0xE0, 0x6C, 0x9A);
/// Top of the petal gradient (lightest).
pub const ROSE_LIGHT: Color = Color::Rgb(0xF2, 0xA9, 0xC4);
/// Bottom of the petal gradient (deepest).
pub const ROSE_DEEP: Color = Color::Rgb(0xB0, 0x39, 0x5B);
/// Muted rose for captions / secondary text.
pub const ROSE_DIM: Color = Color::Rgb(0x9E, 0x4B, 0x6C);
/// Green stem/leaf in the rose art.
pub const LEAF: Color = Color::Rgb(0x6B, 0x9E, 0x78);

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
