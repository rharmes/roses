---
id: TASK-38
title: OPML import and export
status: Done
assignee:
  - '@ross'
created_date: '2026-07-01 14:38'
updated_date: '2026-07-02 17:43'
labels:
  - feature
  - feedbin-api
dependencies: []
references:
  - 'https://github.com/feedbin/feedbin-api/blob/master/content/imports.md'
priority: low
ordinal: 24014
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Support bulk feed migration. Import via Feedbin POST /imports.json (OPML upload) with GET /imports/{id}.json to poll status. Export by generating OPML from GET /subscriptions.json (Feedbin has no export endpoint). Likely exposed as roses subcommands (e.g. roses import FILE / roses export FILE).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 roses can export current subscriptions to a valid OPML file
- [x] #2 roses can import an OPML file via the Feedbin imports endpoint and report completion/status
- [x] #3 Import/export paths covered by tests (mockito for the API, a golden OPML for export)
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. New src/opml.rs (hand-rolled, no XML dep): pub struct OpmlFeed { text, xml_url, html_url: Option }; pub fn to_opml(title, &[OpmlFeed]) -> String (OPML 2.0, flat, escaped attrs); fn xml_escape. Golden unit test (inline expected string, incl. special-char escaping + optional htmlUrl).
2. feedbin.rs: make Subscription pub with pub fields + add feed_url/site_url: Option<String>; add pub fn subscriptions() -> Result<Vec<Subscription>>; refactor feed_titles() to reuse it. Add pub Import { id, complete, import_items: Vec<ImportItem> } + ImportItem { title, feed_url, status } (Deserialize, serde(default) on items); pub fn create_import(&[u8]) -> Import (POST imports.json, Content-Type text/xml, raw body); pub fn import_status(id) -> Import (GET imports/{id}.json). impl Import { pub fn tally() -> ImportTally { complete, pending, failed, failed_urls } } (pure, tested). mockito tests for create_import + import_status; tally unit test.
3. main.rs: mod opml; dispatch export/import; update usage + module doc. run_export(path: Option<String>): connect, subscriptions(), map->OpmlFeed (skip missing feed_url, count skipped), sort by text (case-insensitive), to_opml; write FILE or stdout (status msgs to stderr so piped OPML stays clean). run_import(path): require FILE, read bytes (err if empty), create_import, poll import_status every ~2s until complete or cap (~5min) printing progress, then print tally summary (+ failed urls; note id if still pending).
4. Docs same commit: architecture (opml module row, CLI dispatch bullet, import/export section, imports endpoints in network table), data-model (Subscription now pub +fields, Import/ImportItem/OpmlFeed types, imports API rows), README (Import & export section).
5. cargo fmt + clippy -D warnings + test; run suite 5x for stability.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented on branch task-38-opml-import-export. Design per interview: export takes FILE or stdout (status to stderr so piped OPML stays clean); flat OPML sorted by title (folders deferred to TASK-31); import polls to completion + prints tally; hand-rolled writer, no XML crate (import needs no parser — Feedbin parses server-side). New src/opml.rs: OpmlFeed + to_opml (OPML 2.0, xml_escape) + golden test + empty-doc test. feedbin.rs: Subscription now pub with feed_url/site_url; subscriptions() (feed_titles() refactored to reuse it); Import/ImportItem + Import::tally()->ImportTally (pure); create_import (POST imports.json text/xml raw) + import_status (GET imports/{id}.json); mockito tests for subscriptions/create_import/import_status + tally unit tests. main.rs: mod opml; export/import dispatch + usage; run_export (skip feeds w/o feed_url, sort by title, write file/stdout); run_import (require FILE, err on empty, poll every 2s cap ~5min, print tally + failed urls). 136 tests pass, fmt+clippy clean, 5x stable. Docs in-commit: README Import & export section, architecture (opml module row, CLI dispatch, network table imports rows, OPML section), data-model (Subscription pub, Import/ImportItem/ImportTally/OpmlFeed, imports API rows).
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Shipped as PR #33 (merged to main). Two headless subcommands: 'roses export [FILE]' builds a flat, XML-escaped OPML 2.0 document client-side (Feedbin has no export endpoint) from subscriptions() — sorted by title, feeds without feed_url skipped, status to stderr so piped OPML stays clean; 'roses import FILE' POSTs raw OPML as text/xml (no parser — Feedbin parses server-side) then polls the async import every 2s (cap ~5min) to completion, printing an Import::tally() summary of complete/pending/failed + failed URLs. New dependency-free src/opml.rs (to_opml + xml_escape). feedbin.rs: Subscription now pub with feed_url/site_url (feed_titles() reuses new subscriptions()); Import/ImportItem + pure Import::tally()->ImportTally; create_import + import_status. Folder/tag grouping deferred to TASK-31 (flat for now). Tests: golden OPML + empty-doc, mockito for subscriptions/create_import/import_status, tally units. 136 pass, fmt+clippy clean, 5x stable; docs updated in-commit (README, architecture, data-model).
<!-- SECTION:FINAL_SUMMARY:END -->
