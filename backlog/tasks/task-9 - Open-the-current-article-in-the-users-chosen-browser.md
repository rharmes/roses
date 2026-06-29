---
id: TASK-9
title: Open the current article in the user's chosen browser
status: In Progress
assignee:
  - '@claude'
created_date: '2026-06-29 00:56'
updated_date: '2026-06-29 17:09'
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

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. config.rs: add browser (command template) + browser_terminal (bool) to Settings; BrowserPref { command, terminal } + load_browser_pref(). 2. src/browser.rs: pure resolve(pref, $BROWSER, url) -> Launch{program,args,terminal} (precedence config command > BROWSER > platform open/xdg-open; shlex-split the template; substitute %s/{url} or append the url) + run(&Launch) (status+wait if terminal, else spawn); unit tests for each precedence/placeholder case. 3. main.rs: mod browser. 4. tui.rs: 'o' -> Action::Open opening selected_url(); for a terminal browser suspend the TUI (disable raw mode + LeaveAlternateScreen), run+wait, restore (enable raw mode + EnterAlternateScreen + clear) (AC#3); GUI browsers just spawn (AC#1); footer adds 'o open'; red notice on failure/no-URL. dep: shlex. 5. fmt/clippy -D warnings/test +10x.
<!-- SECTION:PLAN:END -->
