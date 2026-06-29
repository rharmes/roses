---
id: TASK-14
title: 'Inject whimsy: color accents, an ASCII rose, and visual personality'
status: Done
assignee:
  - '@ross'
created_date: '2026-06-29 20:00'
updated_date: '2026-06-29 23:42'
labels:
  - rust
  - ui
dependencies:
  - TASK-11
documentation:
  - docs/tui_research.md
priority: medium
ordinal: 14
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The TUI is visually utilitarian. Add tasteful personality while keeping it clean and legible: a cohesive color accent (e.g. a rose/pink tone for selection/emphasis, feed names, or borders), an ASCII-art rose somewhere fitting (e.g. the empty 'all caught up' state or a startup/loading splash), and other small touches that make it more visually interesting. These are starting ideas - use your own judgment and don't feel beholden to them. Maintain the overall clean aesthetic with a hint of playfulness (no clutter, no rainbow soup).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 The UI gains a cohesive color accent (e.g. a rose/pink tone) applied tastefully and consistently to emphasis/selection or chrome.
- [x] #2 An ASCII-art rose appears somewhere appropriate (e.g. the empty/all-caught-up state or a startup/loading splash).
- [x] #3 The result stays clean and legible - changes are cohesive and restrained, not cluttered.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Per approved plan (rose accent, chrome-only; ASCII rose on all-caught-up only; gradient art + accented footer keys; no rounded borders/spinner):
1. New src/theme.rs (mod theme; in main.rs): rose palette consts (ROSE/ROSE_LIGHT/ROSE_DEEP/ROSE_DIM/LEAF) + lerp() for the petal gradient.
2. tui.rs imports: use crate::theme; add Flex to layout import.
3. column_block: focused border + title rose (Style::new().fg(ROSE).bold()); unfocused unchanged.
4. highlight: focused = fg(ROSE).reversed() (keeps REVERSED → rose bar, existing test passes).
5. reader_text title: fg(ROSE).bold().
6. footer: rebuild help Line as spans, key letters m/u/o/r/q rose+bold, rest dim; keep 'quit' substring.
7. draw(): footer always; if Ready && entries empty → draw_caught_up() centered gradient rose + 'All caught up.' caption (Flex::Center; graceful caption-only fallback on tiny terminals). Loading/Failed unchanged.
8. docs/architecture.md: Theme & whimsy note + theme module row.
Tests: keep REVERSED test; add accent/footer/all-caught-up/loading-unchanged/tiny-terminal tests. fmt+clippy+test, 10x.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented per approved plan + the 4 taste choices. New src/theme.rs (rose palette consts + lerp). Chrome-only rose accent: focused border+title (column_block), selection bar (highlight = fg(ROSE).reversed() — keeps REVERSED so the existing selection test passes), reader title, footer key letters (footer_help). All-caught-up (Ready+empty) short-circuits draw() to draw_caught_up(): vertically-centered ASCII rose, petals graded light→deep via theme::lerp, green stem, 'All caught up.' caption; footer always drawn; degrades to caption-only on tiny terminals. Loading/Failed unchanged. No rounded borders/spinner (not selected). Rendered + eyeballed the rose. 66 tests (theme lerp + 5 TUI: accent/footer/all-caught-up/loading-unchanged/tiny-fallback); fmt+clippy -D warnings clean; stable 10/10. docs/architecture.md updated (theme module row + Theme & whimsy section).

Added 'roses preview' subcommand (user request): renders the all-caught-up rose offline — no login or network — so the empty state can be eyeballed without marking everything read. tui::run_preview() seeds Status::Ready + empty entries and runs a minimal draw loop that quits on q/Esc; wired into main.rs dispatch + usage. docs/architecture.md CLI dispatch updated. fmt+clippy clean, 66 tests, stable 10/10.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Added a cohesive, restrained rose personality to the TUI. New src/theme.rs holds the rose palette (truecolor Rgb: ROSE accent, ROSE_LIGHT/ROSE_DEEP gradient ramp, LEAF) plus a lerp() for the petal gradient. Chrome-only accent (kept restrained): the focused pane's border + title (column_block), the selection bar (highlight = fg(ROSE).reversed() — renders as a rose bar and keeps the REVERSED modifier), the reader title, and the footer's keys including the arrow glyphs (footer_help); feed names, article titles, body, etc. stay neutral. When everything is read (Status::Ready + no entries), draw() short-circuits to draw_caught_up(): a vertically-centered spiral-bloom ASCII rose with petals graded light->deep over a green stem and a centered 'All caught up' caption in the default text color; the footer is always drawn and the art degrades to just the caption on tiny terminals. Loading/Failed states are unchanged. Also added a 'roses preview' subcommand (tui::run_preview) that renders the caught-up screen offline (no login/network) so it can be eyeballed without marking everything read. Verified: cargo fmt --check, clippy --all-targets -D warnings, 66 tests (theme lerp unit test + accent/footer/all-caught-up/loading-unchanged/tiny-fallback render tests; the existing REVERSED-selection and footer 'quit' tests still pass), stable 10/10, CI green. docs/architecture.md updated (theme module + Theme & whimsy section + preview in CLI dispatch). Out of scope per the chosen design: rounded borders, animated spinner, theme-config/256-color fallback.
<!-- SECTION:FINAL_SUMMARY:END -->
