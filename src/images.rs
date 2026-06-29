//! Inline image approximations (TASK-8): fetch entry images and render them as
//! Unicode half-block art that works on any terminal (no Sixel/Kitty protocol).
//!
//! Each character cell is `▀` (upper half block): its foreground colour is the
//! top pixel and its background colour is the bottom pixel, so one text row
//! shows two pixel rows. The result is a `Vec<Line>` that drops straight into
//! the reader's scrollable `Paragraph`.

use std::time::Duration;

use anyhow::{Context, Result, bail};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

/// Refuse images larger than this (decoding is memory-proportional).
const MAX_BYTES: usize = 16 * 1024 * 1024;
const FETCH_TIMEOUT: Duration = Duration::from_secs(10);
const USER_AGENT: &str = concat!("roses/", env!("CARGO_PKG_VERSION"));
/// Cap the rendered height so a tall image can't fill the whole reader.
const MAX_ROWS: u32 = 40;

const UPPER_HALF_BLOCK: char = '▀';

/// Fetch an image and render it to half-block lines `max_cols` wide.
///
/// Images live on arbitrary third-party hosts, so this uses a plain client with
/// **no Feedbin auth** — never replay the user's credentials off-site.
pub fn fetch_and_render(url: &str, max_cols: u16) -> Result<Vec<Line<'static>>> {
    let client = reqwest::blocking::Client::builder()
        .timeout(FETCH_TIMEOUT)
        .user_agent(USER_AGENT)
        .build()
        .context("building the image HTTP client")?;
    let response = client
        .get(url)
        .send()
        .with_context(|| format!("fetching image {url}"))?;
    if !response.status().is_success() {
        bail!("image fetch failed: HTTP {}", response.status());
    }
    let bytes = response.bytes().context("reading image bytes")?;
    if bytes.len() > MAX_BYTES {
        bail!("image too large ({} bytes)", bytes.len());
    }
    let image = image::load_from_memory(&bytes).context("decoding image")?;
    Ok(render(&image, max_cols))
}

/// Render a decoded image to half-block lines (pure; unit-tested).
pub fn render(image: &image::DynamicImage, max_cols: u16) -> Vec<Line<'static>> {
    let cols = u32::from(max_cols).clamp(1, 80);
    let (width, height) = (image.width().max(1), image.height().max(1));
    // Terminal cells are ~twice as tall as wide, so halve the height in cells to
    // keep the picture's proportions.
    let rows = ((cols * height) / width / 2).clamp(1, MAX_ROWS);

    let resized = image
        .resize_exact(cols, rows * 2, image::imageops::FilterType::Triangle)
        .to_rgb8();

    let mut lines = Vec::with_capacity(rows as usize);
    for row in 0..rows {
        // Coalesce runs of same-coloured cells into one span to keep span counts
        // (and the diff/redraw cost) reasonable.
        let mut spans: Vec<Span> = Vec::new();
        let mut run = String::new();
        let mut run_style: Option<Style> = None;
        for col in 0..cols {
            let top = resized.get_pixel(col, row * 2).0;
            let bottom = resized.get_pixel(col, row * 2 + 1).0;
            let style = Style::default()
                .fg(Color::Rgb(top[0], top[1], top[2]))
                .bg(Color::Rgb(bottom[0], bottom[1], bottom[2]));
            if run_style == Some(style) {
                run.push(UPPER_HALF_BLOCK);
            } else {
                if let Some(previous) = run_style.take() {
                    spans.push(Span::styled(std::mem::take(&mut run), previous));
                }
                run.push(UPPER_HALF_BLOCK);
                run_style = Some(style);
            }
        }
        if let Some(previous) = run_style {
            spans.push(Span::styled(run, previous));
        }
        lines.push(Line::from(spans));
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{DynamicImage, Rgb, RgbImage};

    #[test]
    fn renders_half_blocks_with_top_and_bottom_pixel_colours() {
        // 1px wide, 2px tall: a red pixel over a blue one -> one cell, one row.
        let mut buffer = RgbImage::new(1, 2);
        buffer.put_pixel(0, 0, Rgb([255, 0, 0]));
        buffer.put_pixel(0, 1, Rgb([0, 0, 255]));
        let image = DynamicImage::ImageRgb8(buffer);

        let lines = render(&image, 1);
        assert_eq!(lines.len(), 1, "two pixel rows collapse to one text row");
        let spans = &lines[0].spans;
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].content, "▀");
        assert_eq!(
            spans[0].style.fg,
            Some(Color::Rgb(255, 0, 0)),
            "fg = top pixel"
        );
        assert_eq!(
            spans[0].style.bg,
            Some(Color::Rgb(0, 0, 255)),
            "bg = bottom pixel"
        );
    }

    #[test]
    fn render_caps_height_and_width() {
        let image = DynamicImage::ImageRgb8(RgbImage::new(1000, 4000));
        let lines = render(&image, 200);
        assert!(lines.len() as u32 <= MAX_ROWS, "height capped at MAX_ROWS");
        // Width is clamped to 80 cells; each line is at most 80 blocks wide.
        let width: usize = lines[0]
            .spans
            .iter()
            .map(|s| s.content.chars().count())
            .sum();
        assert!(width <= 80, "width capped at 80 cells, got {width}");
    }
}
