---
id: TASK-27
title: Sanitize all feed-derived display strings against terminal-escape injection
status: To Do
assignee: []
created_date: '2026-07-01 14:37'
labels:
  - security
  - hardening
dependencies: []
priority: medium
ordinal: 13014
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
sanitize() in src/tui.rs strips control characters (defusing escape-sequence injection from a hostile feed) but is applied only to reader body text. Titles, author, url, external_url, permalink, and feed names are rendered directly (bold/underlined/raw spans) without stripping. Route every feed-derived string through control-character stripping before it is placed in a Line/Span: reader header, articles list (wrap_title), and sources list.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A feed entry whose title/author/url/feed name contains ESC (0x1B) or other C0 control characters renders with those bytes removed
- [ ] #2 Reader header (title, author, url, external_url, permalink), article titles, and source names are all sanitized
- [ ] #3 A unit test asserts a control-char-laden title and author are stripped
<!-- AC:END -->
