# roses — local persistence & offline cache (TASK-41)

roses keeps a local **SQLite** cache of fetched feeds/entries and their read
state so the TUI paints instantly on launch and stays readable offline. Feedbin
remains the **source of truth** for read/unread; the cache is reconciled after
every successful network load. This is the decision record for the schema and
sync strategy — the implementation is [`src/store.rs`](../src/store.rs), wired
into the UI loop in `tui.rs`.

## Why SQLite (`rusqlite`, bundled)

- **Blocking API** fits the app's synchronous UI loop + `spawn_blocking` model —
  no async runtime needed (unlike `sqlx`).
- **Real SQL** for querying/sorting the unread view and future filters.
- `features = ["bundled"]` compiles SQLite from source, so there is **no system
  `libsqlite3` dependency** — `cargo install roses` and the release build work
  on a bare box.
- **Trade-off (musl):** bundled SQLite is C compiled via `cc`. This reintroduces
  a C-compiled dependency into the otherwise-C-free static-musl build (the
  keyring backend was deliberately kept pure-Rust via zbus). `rusqlite`+`bundled`
  links cleanly under `*-unknown-linux-musl`, and CI's **`musl-build`** job
  (TASK-43) builds `x86_64-unknown-linux-musl` on every push/PR and asserts the
  binary is statically linked — see [`ci.md`](ci.md) / [`release.md`](release.md).
- **Alternatives considered:** async `sqlx` (needs a runtime — mismatch);
  pure-Rust `redb`/`native_db` (keeps musl C-free but isn't SQL and deviates from
  the task's "SQLite").

## Location

`$XDG_DATA_HOME/roses/roses.db` if `XDG_DATA_HOME` is set & non-empty, else
`~/.local/share/roses/roses.db` (honored on macOS too, matching `config.rs`'s XDG
style). WAL journal mode.

## Schema (v1)

```sql
meta(key TEXT PRIMARY KEY, value TEXT NOT NULL)   -- schema_version; unread.etag / unread.last_modified (TASK-42)
feeds(feed_id INTEGER PRIMARY KEY, title TEXT)
entries(
    id        INTEGER PRIMARY KEY,          -- Feedbin entry id
    feed_id   INTEGER NOT NULL,
    published TEXT,                          -- ISO-8601, drives the newest-first sort
    unread    INTEGER NOT NULL DEFAULT 1,
    starred   INTEGER NOT NULL DEFAULT 0,    -- present from v1; wired by TASK-29
    json      TEXT NOT NULL                  -- serialized feedbin::Entry (full hydration)
)
INDEX idx_entries_unread ON entries(unread)
```

Scalar columns (`feed_id`, `published`, `unread`, `starred`) drive
querying/sorting; the full `Entry` rides along as a **JSON blob** so the reader
can hydrate everything (title, body HTML, the extended-mode objects) and the
schema tolerates Feedbin adding fields. `schema_version` in `meta` gates future
migrations. `starred` exists now but only read/unread is wired — **TASK-29** fills
it in (read + write-through) without a migration.

## Sync strategy

Feedbin is authoritative for read state; the cache mirrors it.

- **Reconcile (`replace_snapshot`)** — after a successful network load: upsert
  feeds + the hydrated entries (as unread), then mark every cached-unread entry
  **not** in the fresh unread set (hydrated ids ∪ pending ids) as read. Only the
  bounded set of cached-unread rows is scanned; the full server id list isn't
  materialized.
- **Lazy load-more (`upsert_entries`)** — later-hydrated batches (TASK-40) are
  upserted so they're cached for offline reading too.
- **Write-through (`set_unread`)** — a *successful* mark-read/undo mirrors into
  the cache; on failure the existing optimistic rollback leaves both sides
  unchanged. An offline write *queue* (mutating while disconnected) is **out of
  scope** here — writes still require the network.
- **Delta sync (`get_validators`/`set_validators`, TASK-42)** — the `unread`
  endpoint's `ETag`/`Last-Modified` are stored in `meta` and replayed as
  `If-None-Match` / `If-Modified-Since`. A **`304 Not Modified`** short-circuits
  the whole reload (no subscriptions/entries fetch) and keeps the current view —
  so the common "nothing new" refresh costs one cheap request. Validators persist
  across sessions, so even the first load of a session can 304. Incremental
  hydration on a *changed* set + `updated_entries` content refresh are the
  **TASK-44** follow-up.

All cache writes happen on the **main thread** in the message-drain
(`persist_msg`), so the `Connection` never crosses threads; network work stays on
the `spawn_blocking` pool and only *results* touch the store.

## Startup (offline-first)

`connect()` no longer authenticates before the TUI. `run_loop` opens the store,
seeds `App` from `load_unread(DISPLAY_LIMIT)` for an instant first paint, then the
background fetch (spawned by `run`) reconciles. A failed refresh **with cached
entries present** keeps the cached view and shows a footer notice instead of a
`Failed` screen; with nothing cached it's still `Failed`. A store that won't open
is non-fatal — roses runs without persistence.

## Testing

`store.rs` unit tests use an in-memory / temp-file DB: round-trip,
reconcile-marks-reads, write-through, load-more upsert, reopen-persists, and the
validators round-trip. The App-level offline fallback
(`failed_refresh_keeps_cached_view`) and the 304 behavior
(`not_modified_keeps_the_current_view`) live in `tui.rs`; the client's
conditional 200-then-304 flow is in `feedbin.rs`.

## Future

- **TASK-44** (delta sync part 2): incremental hydration on a changed set +
  `updated_entries` content refresh (builds on TASK-42's validators).
- **TASK-29** wires `starred` (read + write-through) on the existing column.
- An offline write queue (mutate-while-disconnected) could build on this.
