---
id: TASK-33
title: Next-unread navigation across sources
status: To Do
assignee: []
created_date: '2026-07-01 14:38'
labels:
  - feature
  - ux
dependencies: []
priority: medium
ordinal: 19014
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Add a key (e.g. n / N) that jumps to the next/previous unread article across sources in on-screen order (sources by name, articles oldest-first), wrapping at the ends. Purely client-side over the loaded entries; sets focus to the articles/reader column.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The next-unread key moves selection to the next unread entry across source boundaries in display order; the previous-unread key reverses it
- [ ] #2 Navigation wraps at the first/last unread
- [ ] #3 Behavior covered by App-level unit tests over a seeded entry set
<!-- AC:END -->
