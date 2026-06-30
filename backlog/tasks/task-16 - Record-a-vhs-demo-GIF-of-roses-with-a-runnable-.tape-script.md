---
id: TASK-16
title: Record a vhs demo GIF of roses with a runnable .tape script
status: To Do
assignee: []
created_date: '2026-06-30 01:51'
labels:
  - docs
dependencies: []
ordinal: 2014
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Produce an animated GIF that demos roses for the README using vhs (https://github.com/charmbracelet/vhs). Provide a committed .tape script the maintainer runs locally in their terminal against their own live Feedbin account, walking through the core usage: launching roses, moving across the three columns (sources -> articles -> reader), scrolling an article, and key actions (mark read/undo, open in browser), ideally ending on the all-caught-up rose. No credentials in the script (login is assumed/handled out of band). Embed the resulting GIF in the README.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A committed vhs .tape script (e.g. demo/roses.tape) records a GIF of roses when run with vhs
- [ ] #2 The demo walks through the core flow: launching roses, moving across the three columns, scrolling an article, and at least one key action (mark read / undo / open in browser)
- [ ] #3 The maintainer can run it locally against their own live Feedbin account; no secrets are committed (login handled out of band)
- [ ] #4 The generated demo GIF is embedded in the README
<!-- AC:END -->
