---
id: TASK-9
title: Open the current article in the user's chosen browser
status: To Do
assignee: []
created_date: '2026-06-29 00:56'
labels:
  - rust
  - feature
dependencies:
  - TASK-6
documentation:
  - docs/tui_research.md
priority: low
ordinal: 9
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Let the user open the full article in a browser of their choice: a GUI browser (Chrome/Firefox/Safari) or a CLI browser (Carbonyl/w3m/lynx). Honor the BROWSER env var and a config setting; use open/xdg-open as the default. See docs/tui_research.md section 4.3.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A keybinding opens the selected entry URL in the default browser (macOS open / Linux xdg-open)
- [ ] #2 A config setting and the BROWSER env var allow choosing a specific GUI or CLI browser
- [ ] #3 Launching a terminal browser suspends and restores the ratatui screen and raw mode cleanly
<!-- AC:END -->
