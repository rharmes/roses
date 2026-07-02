# roses — data model

The types `roses` holds in memory, persists, and exchanges with Feedbin. For how they flow through the
app, see [`architecture.md`](architecture.md).

## Feedbin domain types

### `feedbin::Entry` (`pub`)

A hydrated unread entry. Feedbin sends most fields nullable, so they're `Option` to avoid panicking on
real-world data.

| Field | Type | Notes |
| --- | --- | --- |
| `id` | `i64` | Feedbin entry id. **IDs grow over time** — a larger id is newer (used to pick the newest unread, and as the read/unread write key). |
| `feed_id` | `i64` | Which subscription/feed this belongs to; joined to `feed_titles`. |
| `title` | `Option<String>` | Falls back to `(untitled)`. |
| `url` | `Option<String>` | Article URL (opened by `o`). |
| `author` | `Option<String>` | Author display name (nullable in Feedbin); shown in the reader header in place of the feed name (TASK-18). |
| `published` | `Option<String>` | ISO-8601 string. Sorted on as a raw string compare (= chronological); also humanized to local time for the reader header via `format_published()` (TASK-17). |
| `summary` | `Option<String>` | Short text; reader body fallback when `content` is absent. |
| `content` | `Option<String>` | Full body **as HTML**; the reader renders it to text + half-block images. |
| `images` | `Option<Box<EntryImages>>` | Extended-mode lead image (`size_1.cdn_url`); shown as a hero when the body has no inline `<img>` (TASK-21). |
| `enclosure` | `Option<Box<Enclosure>>` | Extended-mode podcast/media (`enclosure_url` + type + `itunes_duration`); reader shows a "kind · duration" line and `o` opens it (TASK-22). |
| `json_feed` | `Option<Box<JsonFeed>>` | Extended-mode JSON Feed extras; `external_url` is the link-blog target `o` opens (TASK-23). |

`Deserialize` ignores unknown JSON fields. Derived `Clone` (entries are cloned into background tasks and
the undo stack). `#[allow(dead_code)]` on the struct (`id` is read by the read/undo sync but not every code path).
The three extended-mode objects (`images`/`enclosure`/`json_feed`) are only present with `mode=extended`
and are **boxed** so they don't bloat `Entry` (which travels through the message channel, the entries Vec,
and the undo stack). Accessors `lead_image_url()`, `enclosure_url()`, `external_url()` dig through the
nested `Option`s. `EntryImages { size_1: Option<ImageSize{ cdn_url }> }`,
`Enclosure { enclosure_url, enclosure_type, itunes_duration }`, `JsonFeed { external_url }` — all fields
`Option`, tolerant of absence.

### `feedbin::Subscription` (`pub`)

`feed_id: i64` + `title/feed_url/site_url: Option<String>` (Feedbin's `id`/`created_at` are dropped).
`feed_titles()` builds the `feed_id → title` map from it (null-titled feeds dropped, rendering as
`(unknown feed)`); the OPML export (TASK-38) uses `feed_url` (→ `xmlUrl`) and `site_url` (→ `htmlUrl`).

### `feedbin::Import` / `ImportItem` / `ImportTally` (`pub`, TASK-38)

The OPML-import job. `Import { id: i64, complete: bool, import_items: Vec<ImportItem> }` (`import_items`
`#[serde(default)]` so a payload without it still parses); `ImportItem { feed_url: Option<String>, status:
String }` (`status` is `"pending"`/`"complete"`/`"failed"` — kept as a string since Feedbin owns the
vocabulary; Feedbin's `title` isn't modelled). `Import::tally()` is a pure fold to `ImportTally { complete,
pending, failed: usize, failed_urls: Vec<String> }` (unknown statuses count as pending) — the data
`roses import` prints, testable without the network.

### `opml::OpmlFeed` (`pub`, TASK-38)

`{ text: String, xml_url: String, html_url: Option<String> }` — one feed to serialize as an OPML
`<outline>`. `opml::to_opml(title, &[OpmlFeed]) -> String` writes a flat, XML-escaped OPML 2.0 document;
`run_export` maps `Subscription`s to these (skipping any without a `feed_url`, since an outline needs an
`xmlUrl`; a blank title falls back to the URL) and sorts by `text`.

### Unread state

Feedbin's `unread_entries` endpoint is the **source of truth**: a flat `Vec<i64>` of *all* unread entry
ids (unpaginated). roses hydrates the newest `DISPLAY_LIMIT` (50) into `Entry`s immediately and keeps the
rest as `App.pending_ids`, hydrating the next `LOAD_MORE_BATCH` (100) on demand as the reader nears the
end (TASK-40) — so every unread entry is reachable without a separate paging endpoint.

## Ordering invariants

- **`App.entries`** is stored **newest-first** (`load()` sorts by `published` descending).
- **Sources** (`App.sources()`) are ordered by **feed name** (then `feed_id` as a tiebreak).
- **Articles within a source** (`App.articles()`) are **oldest-first** — `articles()` reverses the
  newest-first `entries`. This is the on-screen order and the image pre-fetch order.

## TUI state — `tui::App` (the single source of UI truth, main thread only)

| Field | Type | Meaning |
| --- | --- | --- |
| `status` | `Status` | `Loading` / `Ready` / `Failed(String)`. |
| `entries` | `Vec<Entry>` | All loaded unread entries, newest-first. |
| `feed_titles` | `HashMap<i64, String>` | `feed_id` → display name. |
| `total_unread` | `usize` | Server's full unread count (for "showing X of Y"); adjusted on mark/undo. Invariant: `total_unread == entries.len() + pending_ids.len()` (plus any in-flight batch). |
| `pending_ids` | `Vec<i64>` | Unread ids not yet hydrated, newest-first; the next `LOAD_MORE_BATCH` is drained on demand (TASK-40). |
| `in_flight_more` | `Option<Vec<i64>>` | The batch a background load-more is hydrating — concurrency guard + retry buffer (restored to `pending_ids` on failure). |
| `focus` | `Focus` | Which column the cursor is in. |
| `selected_source` | `Option<i64>` | Selected **feed_id** (selection by id, not index). |
| `selected_article` | `Option<i64>` | Selected **entry id**. |
| `reader_scroll` | `u16` | Reader vertical scroll offset (clamped to wrapped height each draw). |
| `images` | `HashMap<String, ImageState>` | Per-URL image cache. |
| `image_queue` | `VecDeque<String>` | URLs awaiting a fetch slot, in priority order. |
| `image_urls` | `Vec<String>` | Every distinct image URL of the current load, in on-screen order; drives the `Loading N of M` count. |
| `spinner_tick` | `usize` | Loading-spinner animation frame, advanced once per UI tick. |
| `reader_width` | `u16` | Reader inner width from the last draw; sizes pre-fetched art. |
| `reader_cache` | `Option<ReaderCache>` | Memoized reader render, keyed by `(entry id, width, image_generation)`; rebuilt only on a key miss (TASK-28). |
| `image_generation` | `u64` | Bumped whenever an image resolves, so the reader cache invalidates when a visible image finishes (TASK-28). |
| `undo_stack` | `Vec<Undone>` | Marked-read **batches** that can be restored (most recent last); a single `m` is a batch of one (TASK-30). |
| `notice` | `Option<String>` | Transient footer message (e.g. a write failure); cleared on next key. |
| `pending_confirm` | `Option<Confirm>` | A pending footer `y`/`n` confirmation; the next key answers it instead of its normal binding (only `A`/`MarkWindowRead` arms it — TASK-30). |
| `show_help` | `bool` | Whether the `?` keybinding help overlay is open; while set, any key closes it (TASK-32). Pure chrome — doesn't affect loads or selection. |
| `should_quit` | `bool` | Set by `q`/`Esc`. |

**Why selection-by-id:** mark/undo insert and remove `entries`, which would invalidate stored indices.
Tracking `selected_source`/`selected_article` by id keeps the cursor stable; `ListState` indices are
recomputed from the ids each frame (`source_index`, `article_index`).

### Enums & small structs (`tui.rs`)

- `Focus { Sources, Articles, Reader }` — the single cursor's column.
- `Status { Loading, Ready, Failed(String) }`.
- `Loaded { entries, feed_titles, total_unread, pending_ids }` — payload of a successful fetch (TASK-40).
- `ReaderCache { key: (i64, u16, u64), text: Text, wrapped: u16 }` — memoized reader render (TASK-28).
- `Msg` — background-worker → UI-loop messages:
  - `Loaded(Result<Loaded, String>)`
  - `Write { op: WriteOp, batch: Vec<(Entry, usize)>, result: Result<(), String> }` — a mark/undo write over one or more entries (a single `m`/`u` is a one-element batch; bulk marks carry the whole set — TASK-30).
  - `Image { url: String, result: Result<Vec<Line<'static>>, String> }`
  - `LoadedMore(Result<Vec<Entry>, String>)` — a lazily-hydrated older batch to append (TASK-40).
  - `NotModified` — the conditional unread fetch 304'd; keep the current view (TASK-42).
  - `Validators(feedbin::Validators)` — fresh `ETag`/`Last-Modified` to persist; no UI effect (TASK-42).
- `LoadOutcome { NotModified, Fresh(Loaded, Validators) }` — what `load()` returns to `spawn_fetch` (TASK-42).
- `feedbin::Validators { etag: Option<String>, last_modified: Option<String> }` + `Conditional<T> { NotModified, Modified { data, validators } }` — HTTP-caching types; stored in the cache's `meta` table under `unread.etag` / `unread.last_modified`.
- `WriteOp { MarkRead, Undo }` — which unread-state write a `spawn_write` performed.
- `Undone { batch: Vec<(Entry, usize)> }` — an undoable mark-read: the entries + their positions in `entries`, restored as a unit so one `u` reverses a bulk mark (TASK-30).
- `Action { None, Reload, MarkRead, MarkSourceRead, MarkWindowRead, Undo, OpenInBrowser }` — what a keypress asks the loop to do (`MarkSourceRead`=`M`, `MarkWindowRead`=`A`; TASK-30).
- `Confirm { MarkWindowRead }` — a pending footer `y`/`n` confirmation intercepting the next key (TASK-30).
- `Segment { Text(String), Image(String) }` — one piece of reader content in document order.
- `ImageState { Loading, Ready(Vec<Line<'static>>), Failed }` — image cache entry.

> Errors crossing the channel are `String` (not `anyhow::Error`) because they only need to be displayed,
> and `String` is trivially `Send`.

### Image cache & queue lifecycle

A URL's lifecycle: enqueued by `refill_image_queue` (inserted as `Loading` in `images`, pushed to
`image_queue`) → popped by `next_queued_image` and fetched on the blocking pool → `Msg::Image` sets it
`Ready(art)` or `Failed`. The cache is keyed by URL and reused across reloads. `prioritize_selected_images`
reorders only URLs still in `image_queue` (already-fetched/in-flight URLs aren't in the queue). Render
states: `Ready` → the art lines; `Failed` → `[image unavailable: <url>]`; otherwise → `[image loading… <url>]`.

## Persisted & external storage

### `store` — SQLite offline cache (TASK-41)

A local SQLite DB at `$XDG_DATA_HOME/roses/roses.db` (or `~/.local/share/roses/roses.db`) caches feeds,
entries, and read state so the TUI paints instantly and reads offline. Tables: `meta(key, value)` (schema
version + the TASK-42 HTTP validators `unread.etag`/`unread.last_modified`), `feeds(feed_id, title)`, and `entries(id, feed_id, published, unread,
starred, json)` — scalar columns for the unread query/sort plus a serialized-`Entry` JSON blob for full
hydration. `starred` exists from v1 but is wired by TASK-29. `store::CachedSnapshot { entries, feed_titles,
total_unread }` is the initial-paint payload. Feedbin stays the source of truth for read state; full schema
+ sync strategy are in [`persistence.md`](persistence.md).

### `config::Settings` → `config.toml`

Serialized (TOML) under the config dir (`$XDG_CONFIG_HOME/roses/config.toml` or `~/.config/roses/config.toml`).
`None` fields are omitted by the `toml` serializer.

```toml
# email is written by `roses` on login; browser + refresh settings are user-edited.
email = "reader@example.com"
browser = "w3m %s"          # command template: %s / {url} placeholder, else URL appended
browser_terminal = true     # roses suspends/restores the TUI around a terminal browser
refresh_interval_secs = 300 # background auto-refresh cadence; omit/0 = off (TASK-37)
highlight_color = "#e06c9a" # UI accent override; hex #rrggbb / #rgb; invalid/unset = rose (TASK-45)
load_remote_images = true   # fetch third-party images; false = block all + placeholder (TASK-39)
```

| Field | Type | Notes |
| --- | --- | --- |
| `email` | `Option<String>` | The logged-in Feedbin email; also the keychain username. |
| `browser` | `Option<String>` | Browser command template. |
| `browser_terminal` | `Option<bool>` | Whether `browser` runs in the terminal (default false). |
| `refresh_interval_secs` | `Option<u64>` | Background auto-refresh interval in seconds; `None`/`0` disables it (default). Sub-60 s values are clamped up to a 60 s politeness floor (`MIN_REFRESH_SECS`) by `load_refresh_interval()` (TASK-37). |
| `highlight_color` | `Option<String>` | UI accent color as a hex string (`#rrggbb`/`rrggbb` or 3-digit `#rgb`). Resolved to a `Color` by `theme::parse_hex`; unset or unparseable falls back to `theme::ROSE`. Recolors chrome only — the "all caught up" rose mascot keeps its own palette (TASK-45). |
| `load_remote_images` | `Option<bool>` | Whether to fetch inline/lead images from third-party hosts. `false` blocks every image request (no IP leak to trackers) and the reader shows a `[remote images off: <url>]` placeholder; unset/`true` loads images as usual (default). Read by `load_remote_images()` (TASK-39). |

**No password is ever written here** (a unit test asserts the serialized settings never contain
"password"). The `config.toml` filename is `.gitignore`d defensively.

### `config::Credentials` (`pub`, in-memory only)

`{ email: String, password: String }`. Returned by `load_credentials()`/`login()`; the `password` is
read from the OS keychain and sent as HTTP Basic auth. Never serialized.

### OS keychain

The Feedbin password is stored via `keyring` — service `"roses"`, username = email. The backend is
cfg-gated per platform: macOS uses the native Keychain (`apple-native-keyring-store`); Linux uses the
Secret Service (GNOME Keyring / KWallet) via the pure-Rust zbus backend (`zbus-secret-service-keyring-store`),
which needs a running keyring daemon at runtime (else a keychain op fails with a clear error, no panic).
`roses logout` deletes the keychain entry and clears `email` (keeping other settings).

### `config::BrowserPref` (`pub`)

`{ command: Option<String>, terminal: bool }` — the config browser preference handed to `browser::resolve`
(which layers `$BROWSER` and the platform opener on top).

## `browser::Launch` (`pub`)

A resolved browser invocation: `{ program: String, args: Vec<String>, terminal: bool }`. `terminal`
tells the TUI whether to suspend/restore the alternate screen + raw mode around the child process.

## Feedbin API reference (v2)

Base `https://api.feedbin.com/v2/`, HTTPS only, all paths end in `.json`, **HTTP Basic auth (email +
password) on every request** — no tokens. Full spec: <https://github.com/feedbin/feedbin-api>.

| Endpoint | roses use | Request / response |
| --- | --- | --- |
| `GET /authentication.json` | validate login | 200 valid / 401 invalid. |
| `GET /unread_entries.json` | unread ids | → `[i64, …]` (source of truth). |
| `GET /entries.json?ids=…&mode=extended` | hydrate entries | ≤100 ids/request; `mode=extended` adds `images`/`enclosure`/`json_feed`; → `[Entry-shaped objects]`. |
| `GET /subscriptions.json` | feed names + export | → objects with `feed_id`, `title`, `feed_url`, `site_url`. |
| `DELETE /unread_entries.json` | mark read | body `{"unread_entries":[…]}`, ≤1000 ids; → changed ids. |
| `POST /unread_entries.json` | mark unread (undo) | same shape as DELETE. |
| `POST /imports.json` | import OPML | body = raw OPML, `Content-Type: text/xml`; → `Import` `{id, complete, import_items}` (TASK-38). |
| `GET /imports/{id}.json` | poll import | → the same `Import` shape; `import_items[].status` ∈ `pending`/`complete`/`failed` (TASK-38). |

Write requests send `Content-Type: application/json; charset=utf-8`. Constants: `MAX_IDS_PER_REQUEST = 100`
(entries), `MAX_UNREAD_IDS_PER_REQUEST = 1000` (unread writes). Entry images are fetched separately, with
**no auth** (third-party hosts) — see `images::fetch_and_render` (10 s timeout, 16 MB cap).
