---
id: TASK-13
title: Wrap article titles in the articles pane and space items apart
status: In Progress
assignee:
  - '@ross'
created_date: '2026-06-29 20:00'
updated_date: '2026-06-29 21:10'
labels:
  - rust
  - ui
dependencies:
  - TASK-11
documentation:
  - docs/tui_research.md
priority: medium
ordinal: 13
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
In the articles pane each item is a single line, so long titles are truncated. Render each article's title wrapped across as many lines as needed so the full title is visible, and add a small gap (e.g. a blank line) below each item to separate them. Selection and navigation must still operate per-article, not per-line: up/down moves between whole articles and the highlight covers the entire wrapped item. Wrap to the pane's current inner width and recompute on resize.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Long article titles wrap onto multiple lines so the full title is visible (no truncation) in the articles pane.
- [x] #2 Each article item is separated from the next by spacing (e.g. a blank line).
- [x] #3 Up/down navigation moves between whole articles, and the selection highlight covers the entire wrapped item.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Render each article as a multi-line ListItem instead of a single Line: wrap_title() word-wraps the full title to the pane inner width (block.inner(area).width, recomputed each draw → resize reflows), hard-breaking overlong words; widths via unicode-width so wrapped lines don't overflow/truncate (AC#1).
2. Append a trailing blank Line to each item as the inter-item gap (AC#2).
3. Keep navigation per-article: a whole article is one ListItem, so ratatui List steps by item and applies highlight_style across the full item height — selection covers the entire wrapped item, up/down move article-by-article (AC#3). No new App state (List handles scroll).
4. Promote unicode-width (already transitive) to a direct dep.
5. Tests: wrap_title unit tests (short/wrap/hard-break/empty+zero-width) + a render test asserting multi-line wrap, blank-gap separation, and REVERSED highlight across all wrapped rows. fmt+clippy+test, 10x; update docs/architecture.md.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented in draw_articles: each article is a multi-line ListItem = wrap_title(title, block.inner(area).width) lines + a trailing blank gap line. wrap_title word-wraps on whitespace and hard-breaks words wider than the line, measuring display width via unicode-width (promoted from transitive to a direct dep). Navigation unchanged (move_article/article_index are id/index based); ratatui List highlights the full item height, so the selection covers the whole wrapped article and up/down step per-article. Width recomputed each draw, so resizes reflow. New tests: 4 wrap_title unit tests + render test article_titles_wrap_space_apart_and_highlight_whole_item (asserts >=2 wrapped rows, full title visible, blank gap, next item after gap, REVERSED on every wrapped row of the selected item). fmt + clippy -D warnings clean; 60 tests pass, stable 10/10. Note: the selected item's trailing blank is part of the item so it's also highlighted (a 'selected card' look) — acceptable; flagged for the user. docs/architecture.md updated (layout + deps).
<!-- SECTION:NOTES:END -->
