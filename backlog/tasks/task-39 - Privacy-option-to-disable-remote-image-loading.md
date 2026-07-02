---
id: TASK-39
title: 'Privacy: option to disable remote image loading'
status: Done
assignee:
  - '@ross'
created_date: '2026-07-01 14:38'
updated_date: '2026-07-02 17:15'
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
- [x] #1 A config setting disables remote image fetching; with it off no image HTTP request is issued and a placeholder is shown
- [x] #2 Default behavior is documented; the setting lives in Settings (docs/data-model.md) and README
- [x] #3 A test verifies the image queue is not filled when disabled
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. config.rs: add Settings.load_remote_images: Option<bool>; add pub fn load_remote_images() -> Result<bool> returning the value.unwrap_or(true). TOML round-trip test.
2. tui.rs: App gains load_remote_images: bool (App::new() default true). refill_image_queue early-returns (no enqueue, image_urls stays empty) when disabled — the single choke point for image fetches. reader_text + push_image gain a remote_images: bool param; when false push_image renders dim '[remote images off: <url>]' for both inline and lead images, ignoring the cache. Call site passes self.load_remote_images.
3. Plumbing: UiConfig gains load_remote_images: bool; run_loop sets app.load_remote_images; run() resolves via config::load_remote_images().unwrap_or(true). Mirrors base_accent (constant per process, so not in reader_cache key).
4. Tests: AC#3 — refill_image_queue leaves image_queue + image_urls empty (and no Loading in images map) when disabled; reader render shows the placeholder and enqueues nothing; config round-trip.
5. Docs (same commit): README config block, data-model Settings table + config.toml example, architecture (Image pre-fetch section + config settings list).
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented on branch task-39-disable-remote-images. Config-only (no runtime toggle, per interview). Settings.load_remote_images: Option<bool> + config::load_remote_images() -> bool (default true). App.load_remote_images (default true), set from UiConfig in run_loop; run() resolves via config::load_remote_images().unwrap_or(true). refill_image_queue() — the sole image-enqueue choke point — early-returns when off, so no image HTTP request is issued and image_urls stays empty (no 'N of M' indicator). reader_text/push_image take a remote_images: bool; when off push_image renders dim '[remote images off: <url>]' for both inline and lead images, short-circuiting the cache lookup. Flag is constant per process (like base_accent), so not in the reader_cache key. Tests: refill enqueues nothing + image_progress None when off (AC#3), reader shows the placeholder not 'loading', config round-trip + default-true. 130 pass, fmt+clippy clean, 5x stable. Docs in-commit: README config block, data-model Settings table+example, architecture Image pre-fetch section + config list.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Shipped as PR #32 (merged to main). config.toml load_remote_images (bool, default true) is a privacy switch: false blocks every third-party image fetch — no image HTTP request is issued, so the reader's IP never reaches trackers — and the reader shows a dim '[remote images off: <url>]' placeholder. Gated at the single image-enqueue choke point: refill_image_queue() early-returns when off (nothing queued, image_urls empty so no 'N of M' indicator), and push_image() short-circuits ahead of the cache for inline + lead images. App.load_remote_images resolved once at startup (like base_accent) via config::load_remote_images(), threaded through UiConfig; constant per process so not in the reader_cache key. Config-only, no runtime toggle (per interview). Tests: refill enqueues nothing + image_progress None when off (AC#3), reader renders placeholder not 'loading', config round-trip + default-true. 130 pass, fmt+clippy clean, 5x stable; docs updated in-commit (README, data-model, architecture).
<!-- SECTION:FINAL_SUMMARY:END -->
