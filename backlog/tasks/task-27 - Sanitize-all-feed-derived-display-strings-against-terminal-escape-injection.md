---
id: TASK-27
title: Sanitize all feed-derived display strings against terminal-escape injection
status: In Progress
assignee:
  - '@claude'
created_date: '2026-07-01 14:37'
updated_date: '2026-07-01 15:00'
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
- [x] #1 A feed entry whose title/author/url/feed name contains ESC (0x1B) or other C0 control characters renders with those bytes removed
- [x] #2 Reader header (title, author, url, external_url, permalink), article titles, and source names are all sanitized
- [x] #3 A unit test asserts a control-char-laden title and author are stripped
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Add src/text.rs with pure strip_control_chars() removing ALL control chars (C0/DEL/C1, incl newline/tab) for single-line display fields; unit-tested. 2. Apply in tui.rs: reader header (title/author/url/external_url/permalink), article titles (into wrap_title), source names, and selected_url(). 3. Apply in ui.rs format_unread (title, feed name, url) for the roses list stdout path. 4. Tests: control-char-laden title+author stripped. 5. fmt/clippy/test.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Added src/text.rs::strip_control_chars (drops all C0/DEL/C1 controls incl newline/tab for single-line fields) with unit tests. Applied in tui.rs reader header (title, author, podcast_indicator, external_url, permalink, url), draw_articles titles, draw_sources feed names, and selected_url() (browser arg). Also hardened the roses list stdout path (ui.rs format_unread: title, feed, url) per the agreed scope extension. New tui test reader_header_strips_control_characters_from_title_author_and_url. 87 tests pass, clippy clean.
<!-- SECTION:NOTES:END -->
