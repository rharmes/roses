---
id: TASK-39
title: 'Privacy: option to disable remote image loading'
status: To Do
assignee: []
created_date: '2026-07-01 14:38'
labels:
  - feature
  - privacy
  - security
dependencies: []
priority: low
ordinal: 25014
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
roses auto-fetches inline and lead images from arbitrary third-party hosts on load (refill_image_queue), which leaks the reader IP to trackers and issues requests to whatever host a feed names. Add a config setting (and/or runtime toggle) to disable remote image fetching; when off, images render as a placeholder and no network request is made.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A config setting disables remote image fetching; with it off no image HTTP request is issued and a placeholder is shown
- [ ] #2 Default behavior is documented; the setting lives in Settings (docs/data-model.md) and README
- [ ] #3 A test verifies the image queue is not filled when disabled
<!-- AC:END -->
