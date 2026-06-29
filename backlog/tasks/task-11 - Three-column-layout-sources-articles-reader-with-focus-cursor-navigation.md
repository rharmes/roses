---
id: TASK-11
title: Three-column layout (sources / articles / reader) with focus-cursor navigation
status: In Progress
assignee:
  - '@claude'
created_date: '2026-06-29 16:22'
updated_date: '2026-06-29 16:47'
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
- [x] #1 The TUI renders three columns side by side: sources (each with a count of its unread items), articles for the selected source, and the reader for the selected article.
- [ ] #2 A single focus cursor marks the active column and item using reversed text (legible whether the terminal is light or dark); it starts on the first source.
- [x] #3 Up/Down (and k/j) move the cursor within the focused column only; no pane scrolls independently of the cursor.
- [x] #4 Right/Left (and l/h) move focus across columns (sources -> articles -> reader and back), preserving each column's cursor position.
- [x] #5 Selecting a source populates the articles column with that source's unread items; while focus is on a source, the reader column is empty.
- [x] #6 With focus on the articles column, the selected article renders in the reader column, and mark-read (m) and undo (u) operate on that article.
- [x] #7 With focus on the reader column, Up/Down scroll the article body.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Per-source unread counts: Feedbin's unread_entries is a flat id list with no per-feed count endpoint, and roses currently hydrates only a sample (newest ~50). Initial implementation may show counts over the loaded window; true per-feed totals (hydrate enough to count, or group all unread ids by feed_id) can be scoped here or as a follow-up. This task supersedes the TASK-6 two-pane layout and the TASK-7 per-pane scroll-key scheme (reader scroll moves from space/b to focus-based Up/Down once focus is on the reader).

Rewrote src/tui.rs into a three-column Miller layout (sources | articles | reader) with a single focus cursor. Selection tracked by id (selected_source feed_id, selected_article entry id) so it survives mark/undo edits; sources() groups loaded entries by feed_id with counts, ordered by feed name. Focus enum {Sources, Articles, Reader}: up/down (arrows or k/j) move within the focused column (sources: change source + reset article to first; articles: change article; reader: scroll), left/right (arrows or h/l) move focus across columns preserving each column's cursor; g/G jump to edges; PgUp/PgDn page the reader. Reversed-text highlight on the focused column's selected row, bold on the remembered selection in unfocused columns; focused column gets a bold border (others dim). Reader shows the selected article only when focus is Articles/Reader (empty while a source is focused, per spec). m/u relocated here: act on the selected article when focus is Articles/Reader (no-op in Sources); the TASK-7 optimistic+rollback model adapted to id-based selection, with sensible reselection after removal (stay near index, or drop focus to Sources if a source empties). Footer: '↑↓ move · ←→ focus · m read · u undo · r reload · q quit'; red notice line on write failures. Verified: fmt, clippy --all-targets -- -D warnings, 32 tests (13 TUI: grouping/counts, focus transitions + cursor preservation, in-column movement, reader scroll, source-empty drops focus, mark/undo round-trip + rollback, TestBackend three-column render, reader-empty-on-source), stable across 10 runs. AC#1 and #3-#7 are test-backed and checked; AC#2's reversed-text cursor needs a live look. Per-source counts are over the loaded ~50-entry window (true per-feed totals noted as a possible follow-up).

Bugfix (found in live review): reader pane didn't scroll with up/down while focused. Root cause — scroll was clamped against text.lines.len() (the unwrapped line count); a single long paragraph is one line that word-wraps to many rows, so max_scroll was 0 and each redraw snapped reader_scroll back to 0. Fix: clamp against the wrapped height via Paragraph::line_count(inner_width) on a block-less measurement paragraph (enabled ratatui feature unstable-rendered-line-info). Added a regression test (reader_scrolls_long_wrapped_content, reproduced red then green). 33 tests, stable 10x.
<!-- SECTION:NOTES:END -->
