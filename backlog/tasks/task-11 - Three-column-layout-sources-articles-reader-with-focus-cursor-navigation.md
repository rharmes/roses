---
id: TASK-11
title: Three-column layout (sources / articles / reader) with focus-cursor navigation
status: In Progress
assignee:
  - '@claude'
created_date: '2026-06-29 16:22'
updated_date: '2026-06-29 16:33'
labels:
  - rust
  - ui
dependencies:
  - TASK-6
  - TASK-7
references:
  - 'https://github.com/feedbin/feedbin-api'
documentation:
  - docs/tui_research.md
priority: medium
ordinal: 8
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Redesign the TUI from two panes (entries list + reader) into a three-column Miller-columns layout driven by a single moving focus 'cursor' instead of separate per-pane scroll keys. This realizes the full feeds/entries/reader vision named in TASK-6 (which shipped as two panes) and relocates the TASK-7 mark/undo into the articles column.

Columns:
1. Sources - each subscribed feed (blog) with unread items, showing a count of unread items for that source.
2. Articles - the unread articles for the currently-selected source; mark-read (m) and undo (u) act here.
3. Reader - the body of the currently-selected article.

Navigation is a single focus 'cursor' shown via reversed text (light-on-dark or dark-on-light depending on the terminal), replacing the current per-pane scroll keys:
- Focus starts on the first source in column 1.
- Up/Down (or k/j) move the cursor within the focused column only.
- The selected source drives column 2; with focus on a source, column 3 is empty.
- Right (or l) moves focus to column 2 (first article); Up/Down move through articles and the selected article renders in column 3.
- Right again moves focus to column 3 (reader); Up/Down scroll the article body.
- Left (or h) moves focus to the previous column on the left, preserving each column's cursor position; the reversed-text indicator follows the focused column.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The TUI renders three columns side by side: sources (each with a count of its unread items), articles for the selected source, and the reader for the selected article.
- [ ] #2 A single focus cursor marks the active column and item using reversed text (legible whether the terminal is light or dark); it starts on the first source.
- [ ] #3 Up/Down (and k/j) move the cursor within the focused column only; no pane scrolls independently of the cursor.
- [ ] #4 Right/Left (and l/h) move focus across columns (sources -> articles -> reader and back), preserving each column's cursor position.
- [ ] #5 Selecting a source populates the articles column with that source's unread items; while focus is on a source, the reader column is empty.
- [ ] #6 With focus on the articles column, the selected article renders in the reader column, and mark-read (m) and undo (u) operate on that article.
- [ ] #7 With focus on the reader column, Up/Down scroll the article body.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Per-source unread counts: Feedbin's unread_entries is a flat id list with no per-feed count endpoint, and roses currently hydrates only a sample (newest ~50). Initial implementation may show counts over the loaded window; true per-feed totals (hydrate enough to count, or group all unread ids by feed_id) can be scoped here or as a follow-up. This task supersedes the TASK-6 two-pane layout and the TASK-7 per-pane scroll-key scheme (reader scroll moves from space/b to focus-based Up/Down once focus is on the reader).
<!-- SECTION:NOTES:END -->
