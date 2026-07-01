---
id: TASK-45
title: Configurable highlight color (hex) overriding the rose accent
status: In Progress
assignee:
  - '@ross'
created_date: '2026-07-01 22:20'
updated_date: '2026-07-01 23:40'
labels:
  - feature
  - ux
dependencies: []
priority: low
ordinal: 30014
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The UI accent color is hard-coded to theme::ROSE (TASK-14) and applied to chrome only: the focused pane's border + title, the selection bar, the reader title, and the footer key glyphs. Add a config.toml setting that lets the user specify their own accent as a hex color string (e.g. "#af5f87" or "af5f87"), overriding the rose default. Resolve + validate the hex once at config load and thread the resolved ratatui Color through the TUI so every accent call site uses it; keep theme::ROSE as the fallback. This builds on the config/settings plumbing (config.rs Settings) and the theme module.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 A config.toml setting (e.g. highlight_color) accepts a hex color and, when set, replaces the rose accent everywhere it is used (focused border/title, selection bar, reader title, footer keys)
- [x] #2 Unset or invalid hex falls back to the built-in rose default with no panic (invalid values are ignored)
- [x] #3 The hex-to-Color parser is unit-tested: accepts #rrggbb and rrggbb, rejects malformed input
- [x] #4 The setting is documented in README (Config), docs/data-model.md (Settings), and docs/architecture.md (theme accent)
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented on branch task-45-highlight-color. Added Settings.highlight_color (Option<String>) + config::load_highlight_color(); theme::parse_hex() accepts #rrggbb/rrggbb + 3-digit #rgb (case-insensitive, optional #, trims ws), rejects malformed -> None. run() resolves once: load_highlight_color -> parse_hex -> unwrap_or(ROSE), stored on App.base_accent. accent() returns MUTED under help else base_accent; column_block/highlight use accent(); footer_help/help_lines/draw_help_overlay/reader_text + the confirm-prompt take the accent (base_accent, un-muted). Chrome only — the caught-up rose mascot keeps its own gradient (per interview). Bundled browser_pref/refresh_interval/accent into UiConfig to stay under clippy's 7-arg limit (repo had no existing allows). Tests: parse_hex accepts/rejects (theme), highlight_color toml round-trip (config), render test asserting focused border+selection+reader title+footer keys all take a configured blue with NO rose leaking (AC #1), and the mascot-stays-rose render test. 127 pass, fmt+clippy clean, 5x stable. Docs: README config block, data-model Settings table+example, architecture Theme section + config list.
<!-- SECTION:NOTES:END -->
