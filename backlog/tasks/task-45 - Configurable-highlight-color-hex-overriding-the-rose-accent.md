---
id: TASK-45
title: Configurable highlight color (hex) overriding the rose accent
status: To Do
assignee: []
created_date: '2026-07-01 22:20'
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
- [ ] #1 A config.toml setting (e.g. highlight_color) accepts a hex color and, when set, replaces the rose accent everywhere it is used (focused border/title, selection bar, reader title, footer keys)
- [ ] #2 Unset or invalid hex falls back to the built-in rose default with no panic (invalid values are ignored)
- [ ] #3 The hex-to-Color parser is unit-tested: accepts #rrggbb and rrggbb, rejects malformed input
- [ ] #4 The setting is documented in README (Config), docs/data-model.md (Settings), and docs/architecture.md (theme accent)
<!-- AC:END -->
