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

`Deserialize` ignores unknown JSON fields. Derived `Clone` (entries are cloned into background tasks and
the undo stack). `#[allow(dead_code)]` on the struct (`id` is read by the read/undo sync but not every code path).

### `feedbin::Subscription` (private)

Only `feed_id: i64` + `title: Option<String>` are deserialized; used solely to build the
`feed_titles: HashMap<i64, String>` map (null-titled feeds are dropped and render as `(unknown feed)`).

### Unread state

Feedbin's `unread_entries` endpoint is the **source of truth**: a flat `Vec<i64>` of unread entry ids.
roses loads the newest `DISPLAY_LIMIT` (50) of them and hydrates those into `Entry`s.

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
| `total_unread` | `usize` | Server's full unread count (for "showing X of Y"); adjusted on mark/undo. |
| `focus` | `Focus` | Which column the cursor is in. |
| `selected_source` | `Option<i64>` | Selected **feed_id** (selection by id, not index). |
| `selected_article` | `Option<i64>` | Selected **entry id**. |
| `reader_scroll` | `u16` | Reader vertical scroll offset (clamped to wrapped height each draw). |
| `images` | `HashMap<String, ImageState>` | Per-URL image cache. |
| `image_queue` | `VecDeque<String>` | URLs awaiting a fetch slot, in priority order. |
| `image_urls` | `Vec<String>` | Every distinct image URL of the current load, in on-screen order; drives the `Loading N of M` count. |
| `spinner_tick` | `usize` | Loading-spinner animation frame, advanced once per UI tick. |
| `reader_width` | `u16` | Reader inner width from the last draw; sizes pre-fetched art. |
| `undo_stack` | `Vec<Undone>` | Marked-read entries that can be restored (most recent last). |
| `notice` | `Option<String>` | Transient footer message (e.g. a write failure); cleared on next key. |
| `should_quit` | `bool` | Set by `q`/`Esc`. |

**Why selection-by-id:** mark/undo insert and remove `entries`, which would invalidate stored indices.
Tracking `selected_source`/`selected_article` by id keeps the cursor stable; `ListState` indices are
recomputed from the ids each frame (`source_index`, `article_index`).

### Enums & small structs (`tui.rs`)

- `Focus { Sources, Articles, Reader }` — the single cursor's column.
- `Status { Loading, Ready, Failed(String) }`.
- `Loaded { entries, feed_titles, total_unread }` — payload of a successful fetch.
- `Msg` — background-worker → UI-loop messages:
  - `Loaded(Result<Loaded, String>)`
  - `Write { op: WriteOp, entry: Entry, index: usize, result: Result<(), String> }`
  - `Image { url: String, result: Result<Vec<Line<'static>>, String> }`
- `WriteOp { MarkRead, Undo }` — which unread-state write a `spawn_write` performed.
- `Undone { entry: Entry, index: usize }` — an undoable mark-read (entry + its position in `entries`).
- `Action { None, Reload, MarkRead, Undo, OpenInBrowser }` — what a keypress asks the loop to do.
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

### `config::Settings` → `config.toml`

Serialized (TOML) under the config dir (`$XDG_CONFIG_HOME/roses/config.toml` or `~/.config/roses/config.toml`).
`None` fields are omitted by the `toml` serializer.

```toml
# email is written by `roses` on login; browser settings are user-edited.
email = "reader@example.com"
browser = "w3m %s"        # command template: %s / {url} placeholder, else URL appended
browser_terminal = true   # roses suspends/restores the TUI around a terminal browser
```

| Field | Type | Notes |
| --- | --- | --- |
| `email` | `Option<String>` | The logged-in Feedbin email; also the keychain username. |
| `browser` | `Option<String>` | Browser command template. |
| `browser_terminal` | `Option<bool>` | Whether `browser` runs in the terminal (default false). |

**No password is ever written here** (a unit test asserts the serialized settings never contain
"password"). The `config.toml` filename is `.gitignore`d defensively.

### `config::Credentials` (`pub`, in-memory only)

`{ email: String, password: String }`. Returned by `load_credentials()`/`login()`; the `password` is
read from the OS keychain and sent as HTTP Basic auth. Never serialized.

### OS keychain

The Feedbin password is stored via `keyring` — service `"roses"`, username = email. macOS uses the
native Keychain (`apple-native-keyring-store`); Linux would use the Secret Service. `roses logout`
deletes the keychain entry and clears `email` (keeping other settings).

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
| `GET /entries.json?ids=…` | hydrate entries | ≤100 ids/request; → `[Entry-shaped objects]`. |
| `GET /subscriptions.json` | feed names | → objects with `feed_id` + `title`. |
| `DELETE /unread_entries.json` | mark read | body `{"unread_entries":[…]}`, ≤1000 ids; → changed ids. |
| `POST /unread_entries.json` | mark unread (undo) | same shape as DELETE. |

Write requests send `Content-Type: application/json; charset=utf-8`. Constants: `MAX_IDS_PER_REQUEST = 100`
(entries), `MAX_UNREAD_IDS_PER_REQUEST = 1000` (unread writes). Entry images are fetched separately, with
**no auth** (third-party hosts) — see `images::fetch_and_render` (10 s timeout, 16 MB cap).
