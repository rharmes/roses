---
id: TASK-31
title: Group feeds into folders via Feedbin tags
status: To Do
assignee: []
created_date: '2026-07-01 14:37'
labels:
  - feature
  - feedbin-api
dependencies: []
references:
  - 'https://github.com/feedbin/feedbin-api/blob/master/content/taggings.md'
priority: medium
ordinal: 17014
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The Sources column is currently a flat list by feed name. Feedbin groups feeds with taggings (GET /taggings.json returns id, feed_id, name). Fetch taggings and render sources grouped by tag/folder (feeds may belong to multiple tags; untagged feeds go under a default group). Creating/deleting a tagging (POST/DELETE /taggings.json) can be a follow-on; the read/group path is the core deliverable.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 roses fetches taggings and displays sources grouped under their tag names, with an Untagged group for feeds without a tag
- [ ] #2 Feeds in multiple tags appear under each; unread counts remain correct per group
- [ ] #3 A taggings fetch failure degrades gracefully to the flat list
- [ ] #4 Client method covered by a mockito test; docs updated
<!-- AC:END -->
