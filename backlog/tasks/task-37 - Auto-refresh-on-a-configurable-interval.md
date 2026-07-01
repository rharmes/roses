---
id: TASK-37
title: Auto-refresh on a configurable interval
status: Done
assignee:
  - '@ross'
created_date: '2026-07-01 14:38'
updated_date: '2026-07-01 20:46'
labels:
  - feature
dependencies: []
priority: low
ordinal: 23014
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Add optional background auto-refresh: re-run the load on a configurable interval (a config.toml setting, default off) without blocking input, reusing spawn_fetch. Must not disrupt the current selection or a scroll position mid-read.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 A config setting controls the refresh interval and disables it when unset or zero
- [x] #2 Auto-refresh reloads in the background and preserves selection where possible
- [x] #3 The setting is documented in docs/data-model.md (Settings) and README
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. config: add refresh_interval_secs setting + pure refresh_interval_from() (None/0 = off, clamp sub-60s up to MIN_REFRESH_SECS=60) + load_refresh_interval().
2. tui: unify manual reload + auto-refresh into one gentle Msg::Loaded apply path — preserve_or_reselect() keeps focus/selection/scroll by id (reselect only if the selected article/source vanished); preserve the undo stack, pruning only re-added entries.
3. tui run_loop: track last_fetch:Instant + fetch_in_flight guard; pure should_auto_refresh(interval, elapsed, in_flight) predicate fires a SILENT background spawn_fetch (no Status::Loading) each interval; manual Reload resets the timer. 304 (unchanged) is a no-op.
4. Tests: config mapping + toml round-trip; should_auto_refresh predicate; reload preserves selection/scroll; reselects when article vanished; preserves undo but prunes re-added. All deterministic (no timing/sleeps).
5. Docs (same commit): README + data-model Settings; architecture run-loop step, undo-stack + selection notes, config list; persistence 'spawned by run' -> run_loop accuracy fix.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented as a unified gentle Msg::Loaded apply path (no new Msg variant): manual reload and background auto-refresh both preserve selection/scroll/undo by id; auto-refresh is silent (no Status::Loading) and 304s to a no-op when unchanged. Config refresh_interval_secs (seconds, 60s floor, off by default). Per interview: seconds+60s floor, footer notice on failure, preserve undo, and manual reload also made gentle. 108 tests pass (fmt+clippy clean); ran the suite 5x for flakiness — stable.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Shipped as PR #27 (merged). Opt-in refresh_interval_secs config (seconds, off by default, 60s floor) drives a silent timer-based background refresh via a pure should_auto_refresh() predicate. Manual reload + auto-refresh unified into one gentle Msg::Loaded path that preserves focus/selection/scroll and the undo stack by id (reselects only when the read article vanished; prunes re-added undo entries); a 304 is a no-op. Deterministic tests (108 pass, 5x stable); fmt+clippy clean; docs updated in-commit incl. the persistence 'spawned by run'->run_loop fix.
<!-- SECTION:FINAL_SUMMARY:END -->
