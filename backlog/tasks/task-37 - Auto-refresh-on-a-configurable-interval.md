---
id: TASK-37
title: Auto-refresh on a configurable interval
status: To Do
assignee: []
created_date: '2026-07-01 14:38'
labels:
  - feature
dependencies: []
priority: low
ordinal: 23014
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Add optional background auto-refresh: re-run the load on a configurable interval (a config.toml setting, default off) without blocking input, reusing spawn_fetch. Must not disrupt the current selection or a scroll position mid-read.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A config setting controls the refresh interval and disables it when unset or zero
- [ ] #2 Auto-refresh reloads in the background and preserves selection where possible
- [ ] #3 The setting is documented in docs/data-model.md (Settings) and README
<!-- AC:END -->
