---
id: TASK-36
title: Add and remove subscriptions in-app
status: To Do
assignee: []
created_date: '2026-07-01 14:38'
labels:
  - feature
  - feedbin-api
dependencies: []
references:
  - 'https://github.com/feedbin/feedbin-api/blob/master/content/subscriptions.md'
priority: medium
ordinal: 22014
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Let the user subscribe (POST /subscriptions.json with a feed_url) and unsubscribe (DELETE /subscriptions/{id}.json). Handle the 300 Multiple Choices response (a list of feed_url/title candidates) by prompting the user to choose, and the 404 no-feed-found case. Rename (PATCH title) is optional.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The user can add a feed by URL; on a 300 Multiple Choices response roses presents the candidates and subscribes to the chosen one
- [ ] #2 The user can unsubscribe from the selected source
- [ ] #3 A 404/no-feed-found surfaces a clear message; client methods covered by mockito tests including the 300 branch
<!-- AC:END -->
