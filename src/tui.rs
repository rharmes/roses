//! Full-screen terminal UI (TASK-6, redesigned in TASK-11).
//!
//! A three-column Miller-columns layout on the crossterm backend: **sources**
//! (feeds with unread counts) | **articles** for the selected source | a
//! scrollable **reader**. A single focus "cursor" (reversed text) moves with the
//! arrow/`hjkl` keys — up/down within the focused column, left/right between
//! columns. Feedbin is queried on a background `tokio::spawn_blocking` task so
//! input never blocks; results arrive over a channel drained each tick.

use std::collections::{HashMap, HashSet, VecDeque};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use chrono::{DateTime, Local};
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::layout::{Alignment, Constraint, Flex, Layout, Margin, Rect};
use ratatui::style::{Color, Style, Stylize};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{
    Block, Clear, List, ListItem, ListState, Padding, Paragraph, Scrollbar, ScrollbarOrientation,
    ScrollbarState, Wrap,
};
use ratatui::{DefaultTerminal, Frame};
use tokio::runtime::Handle;
use tokio::sync::mpsc::{self, UnboundedSender};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::browser;
use crate::config::BrowserPref;
use crate::feedbin::{Client, Conditional, Enclosure, Entry, Validators};
use crate::store::Store;
use crate::text::strip_control_chars;
use crate::theme;

/// How many of the newest unread entries to hydrate on the initial load.
const DISPLAY_LIMIT: usize = 50;
/// How many more unread entries to hydrate per lazy load-more batch — one
/// `entries` request (TASK-40).
const LOAD_MORE_BATCH: usize = 100;
/// Begin hydrating the next batch once the selection is within this many entries
/// of the oldest loaded one, so more arrive before the user reaches the end
/// (TASK-40).
const LOAD_MORE_THRESHOLD: usize = 15;
/// Lines the reader scrolls per PageUp/PageDown.
const READER_PAGE: u16 = 10;
/// Lines the reader scrolls per ↑/↓ (a few at a time, so holding the key moves at
/// a useful pace rather than one line per key-repeat).
const READER_SCROLL_STEP: u16 = 3;
/// How long to wait for input before redrawing (also bounds how quickly a
/// finished background task shows up).
const TICK: Duration = Duration::from_millis(100);
/// Cap on simultaneous image fetches so pre-fetching stays polite to hosts.
const MAX_IMAGE_FETCHES: usize = 6;

/// A fully-loaded snapshot from Feedbin.
struct Loaded {
    entries: Vec<Entry>,
    feed_titles: HashMap<i64, String>,
    total_unread: usize,
    /// Unread ids beyond the initially-hydrated window, newest-first — hydrated
    /// on demand as the user reads toward the end (TASK-40).
    pending_ids: Vec<i64>,
}

/// Which unread-state write a background task performed.
#[derive(Clone, Copy)]
enum WriteOp {
    MarkRead,
    Undo,
}

/// A group of entries marked read together and restorable as a unit, each
/// remembering its position in `entries` so undo can put the batch back in
/// published order. A single `m` is a batch of one; `M`/`A` are larger batches
/// (TASK-30), so one undo (`u`) reverses a bulk mark in a single step.
struct Undone {
    batch: Vec<(Entry, usize)>,
}

/// Message from a background worker to the UI loop.
enum Msg {
    Loaded(Result<Loaded, String>),
    /// Result of a mark-read / undo write, carrying the batch of entries + their
    /// indices so the UI can finalize on success or roll back on failure (TASK-7
    /// AC #4). A single mark is a one-element batch; bulk marks (TASK-30) carry
    /// the whole set so one undo restores it.
    Write {
        op: WriteOp,
        batch: Vec<(Entry, usize)>,
        result: Result<(), String>,
    },
    /// Rendered half-block art for an entry image, keyed by its source URL.
    Image {
        url: String,
        result: Result<Vec<Line<'static>>, String>,
    },
    /// A lazily-hydrated batch of older unread entries to append (TASK-40).
    LoadedMore(Result<Vec<Entry>, String>),
    /// The conditional unread fetch returned `304 Not Modified` — nothing
    /// changed, so keep the current view (TASK-42).
    NotModified,
    /// Fresh HTTP validators from a `200` unread fetch, to persist for the next
    /// conditional request (TASK-42). No UI effect.
    Validators(Validators),
}

/// Outcome of the conditional load (TASK-42): unchanged, or a fresh snapshot
/// plus the validators to store.
enum LoadOutcome {
    NotModified,
    Fresh(Loaded, Validators),
}

/// Cache state for one entry image (TASK-8).
enum ImageState {
    Loading,
    Ready(Vec<Line<'static>>),
    Failed,
}

/// A piece of reader content in document order: a run of text, or an image.
enum Segment {
    Text(String),
    Image(String),
}

/// What a keypress asks the run loop to do beyond mutating `App`.
enum Action {
    None,
    Reload,
    MarkRead,
    /// Mark every loaded article in the selected source read (`M`, TASK-30).
    MarkSourceRead,
    /// Mark every loaded article read — the whole window (`A`, TASK-30).
    MarkWindowRead,
    Undo,
    OpenInBrowser,
}

/// A pending y/n confirmation shown in the footer, intercepting the next key.
/// Only the whole-window mark is gated (TASK-30); the source mark is instant.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Confirm {
    MarkWindowRead,
}

/// Which column the cursor is in.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Focus {
    Sources,
    Articles,
    Reader,
}

enum Status {
    Loading,
    Ready,
    Failed(String),
}

/// A memoized reader render (TASK-28), valid while its `key` is unchanged — so a
/// static article isn't re-parsed from HTML on every ~100ms frame or scroll.
struct ReaderCache {
    /// `(selected entry id, reader inner width, image generation)`.
    key: (i64, u16, u64),
    text: Text<'static>,
    /// Wrapped height at `key.1` width, for the scroll clamp + scrollbar.
    wrapped: u16,
}

struct App {
    status: Status,
    /// All loaded unread entries, newest first.
    entries: Vec<Entry>,
    feed_titles: HashMap<i64, String>,
    total_unread: usize,
    /// Unread ids not yet hydrated, newest-first; drained a batch at a time as
    /// the user reads toward the end (TASK-40).
    pending_ids: Vec<i64>,
    /// The batch currently being hydrated by a background load-more, if any —
    /// both a concurrency guard and a retry buffer (restored on failure).
    in_flight_more: Option<Vec<i64>>,
    focus: Focus,
    /// Selection is tracked by id, not index, so it survives mark/undo edits.
    selected_source: Option<i64>,
    selected_article: Option<i64>,
    reader_scroll: u16,
    /// Rendered/loading entry images, keyed by source URL (TASK-8).
    images: HashMap<String, ImageState>,
    /// Image URLs awaiting a fetch slot, in priority order (pre-fetch queue).
    image_queue: VecDeque<String>,
    /// Every distinct image URL in the current load, in on-screen order; drives
    /// the "Loading N of M images" progress count (TASK-19).
    image_urls: Vec<String>,
    /// Animation frame for the loading spinner, advanced once per UI tick.
    spinner_tick: usize,
    /// Inner width of the reader pane from the last draw, used to size images.
    reader_width: u16,
    /// Memoized reader render (TASK-28); rebuilt only when its key changes.
    reader_cache: Option<ReaderCache>,
    /// Bumped whenever an image resolves, so the reader cache invalidates when a
    /// visible image finishes loading (keys the cache on image state cheaply).
    image_generation: u64,
    /// Marked-read entries that can be restored, most recent last.
    undo_stack: Vec<Undone>,
    /// Transient status line (e.g. a write failure); cleared on the next key.
    notice: Option<String>,
    /// A pending y/n confirmation shown in the footer; the next key answers it
    /// (`y` proceeds, anything else cancels) instead of its normal binding.
    pending_confirm: Option<Confirm>,
    /// Whether the keybinding help overlay is open (`?` toggles it — TASK-32).
    show_help: bool,
    /// The resolved accent color (rose by default, or the user's `highlight_color`
    /// — TASK-45). Set once from config; `accent()` layers the help-overlay mute
    /// on top. Chrome only — the "all caught up" rose mascot keeps its own palette.
    base_accent: Color,
    should_quit: bool,
}

impl App {
    fn new() -> Self {
        Self {
            status: Status::Loading,
            entries: Vec::new(),
            feed_titles: HashMap::new(),
            total_unread: 0,
            pending_ids: Vec::new(),
            in_flight_more: None,
            focus: Focus::Sources,
            selected_source: None,
            selected_article: None,
            reader_scroll: 0,
            images: HashMap::new(),
            image_queue: VecDeque::new(),
            image_urls: Vec::new(),
            spinner_tick: 0,
            reader_width: 0,
            reader_cache: None,
            image_generation: 0,
            undo_stack: Vec::new(),
            notice: None,
            pending_confirm: None,
            show_help: false,
            base_accent: theme::ROSE,
            should_quit: false,
        }
    }

    // --- derived views -----------------------------------------------------

    fn feed_name(&self, feed_id: i64) -> &str {
        self.feed_titles
            .get(&feed_id)
            .map(String::as_str)
            .unwrap_or("(unknown feed)")
    }

    /// Distinct sources (feed_id, unread-in-window count), ordered by name.
    fn sources(&self) -> Vec<(i64, usize)> {
        let mut counts: HashMap<i64, usize> = HashMap::new();
        for entry in &self.entries {
            *counts.entry(entry.feed_id).or_default() += 1;
        }
        let mut rows: Vec<(i64, usize)> = counts.into_iter().collect();
        rows.sort_by(|a, b| {
            self.feed_name(a.0)
                .cmp(self.feed_name(b.0))
                .then(a.0.cmp(&b.0))
        });
        rows
    }

    /// Loaded articles for one source, **oldest first**. Entries are stored
    /// newest-first, and the articles column reverses that so the oldest unread
    /// item sits at the top (TASK-11).
    fn articles(&self, feed_id: i64) -> Vec<&Entry> {
        let mut articles: Vec<&Entry> = self
            .entries
            .iter()
            .filter(|e| e.feed_id == feed_id)
            .collect();
        articles.reverse();
        articles
    }

    /// Article ids for one source, oldest first (matching the displayed order).
    fn article_ids(&self, feed_id: i64) -> Vec<i64> {
        self.articles(feed_id).iter().map(|e| e.id).collect()
    }

    fn selected_article_entry(&self) -> Option<&Entry> {
        let id = self.selected_article?;
        self.entries.iter().find(|e| e.id == id)
    }

    /// The URL of the selected article, if any (for opening in a browser).
    /// The URL that `o` opens for the selected entry. Precedence: a podcast
    /// enclosure (TASK-22) → a link-blog `external_url` (TASK-23) → the permalink.
    fn selected_url(&self) -> Option<String> {
        self.selected_article_entry().and_then(|e| {
            e.enclosure_url()
                .or_else(|| e.external_url())
                .map(str::to_string)
                .or_else(|| e.url.clone())
                // Strip control chars so a hostile url can't inject terminal
                // escapes into the spawned browser command's arguments.
                .map(|url| strip_control_chars(&url))
        })
    }

    /// Image URLs in one article's body, in document order.
    fn article_image_urls(&self, entry: &Entry) -> Vec<String> {
        let body = entry
            .content
            .as_deref()
            .or(entry.summary.as_deref())
            .unwrap_or("");
        let inline: Vec<String> = content_blocks(body)
            .into_iter()
            .filter_map(|block| match block {
                Segment::Image(url) => Some(url),
                Segment::Text(_) => None,
            })
            .collect();
        // Fall back to the extended-mode lead image only when the body has no
        // inline image — matching the reader, which shows the lead image only
        // then (TASK-21), so the pre-fetch and the "N of M" count stay in sync.
        if inline.is_empty()
            && let Some(url) = entry.lead_image_url()
        {
            return vec![url.to_string()];
        }
        inline
    }

    /// Enqueue every not-yet-seen image for background pre-fetch in on-screen
    /// order — sources top-to-bottom (by name), and within each source articles
    /// top-to-bottom (oldest first) — marking each `Loading` so it is fetched
    /// once.
    fn refill_image_queue(&mut self) {
        let source_ids: Vec<i64> = self.sources().into_iter().map(|(id, _)| id).collect();
        // Record every distinct image URL of the current load (in on-screen
        // order) for the progress count, queueing the ones not yet cached.
        let mut urls = Vec::new();
        let mut seen = HashSet::new();
        for feed_id in source_ids {
            for article_id in self.article_ids(feed_id) {
                let Some(entry) = self.entries.iter().find(|e| e.id == article_id) else {
                    continue;
                };
                for url in self.article_image_urls(entry) {
                    if !seen.insert(url.clone()) {
                        continue;
                    }
                    urls.push(url.clone());
                    if !self.images.contains_key(&url) {
                        self.images.insert(url.clone(), ImageState::Loading);
                        self.image_queue.push_back(url);
                    }
                }
            }
        }
        self.image_urls = urls;
    }

    /// Progress of the current load's image fetches as `(done, total)`, or `None`
    /// when there's nothing to show — no images, or all have resolved. `done`
    /// counts images that reached `Ready` or `Failed`; `total` is the whole load.
    /// Drives the footer loading indicator (TASK-19).
    fn image_progress(&self) -> Option<(usize, usize)> {
        let total = self.image_urls.len();
        if total == 0 {
            return None;
        }
        let done = self
            .image_urls
            .iter()
            .filter(|u| {
                matches!(
                    self.images.get(*u),
                    Some(ImageState::Ready(_)) | Some(ImageState::Failed)
                )
            })
            .count();
        (done < total).then_some((done, total))
    }

    /// Pop the next image URL to fetch (already marked `Loading`).
    fn next_queued_image(&mut self) -> Option<String> {
        self.image_queue.pop_front()
    }

    /// Hybrid pre-fetch: keep the top-to-bottom base order, but if the focused
    /// article still has images waiting in the queue (i.e. not yet fetched),
    /// move just those to the front so an explicit jump pulls them forward.
    /// Already-fetched / in-flight images aren't in the queue, so they're left
    /// alone and sequential reading stays top-to-bottom.
    fn prioritize_selected_images(&mut self) {
        if self.image_queue.is_empty() {
            return;
        }
        let Some(entry) = self.selected_article_entry() else {
            return;
        };
        let wanted: HashSet<String> = self.article_image_urls(entry).into_iter().collect();
        if wanted.is_empty() {
            return;
        }
        let mut front = VecDeque::with_capacity(self.image_queue.len());
        let mut rest = Vec::new();
        for url in self.image_queue.drain(..) {
            if wanted.contains(&url) {
                front.push_back(url);
            } else {
                rest.push(url);
            }
        }
        front.extend(rest);
        self.image_queue = front;
    }

    fn source_index(&self) -> Option<usize> {
        let sel = self.selected_source?;
        self.sources().iter().position(|s| s.0 == sel)
    }

    fn article_index(&self) -> Option<usize> {
        let (feed_id, article) = (self.selected_source?, self.selected_article?);
        self.article_ids(feed_id)
            .iter()
            .position(|&id| id == article)
    }

    /// True when the selection is within `LOAD_MORE_THRESHOLD` of the oldest
    /// loaded entry (in the newest-first `entries` order) and there are still
    /// un-hydrated unread ids — the cue to fetch the next batch (TASK-40). With
    /// no selection (e.g. everything nearby was read), a near-empty list also
    /// qualifies so reading never dead-ends while more remain.
    fn near_tail(&self) -> bool {
        if self.pending_ids.is_empty() {
            return false;
        }
        match self
            .selected_article
            .and_then(|id| self.entries.iter().position(|e| e.id == id))
        {
            Some(idx) => idx + LOAD_MORE_THRESHOLD >= self.entries.len(),
            None => self.entries.len() <= LOAD_MORE_THRESHOLD,
        }
    }

    /// If a load-more is warranted and none is in flight, drain the next batch of
    /// pending ids into `in_flight_more` and return them for hydration (TASK-40).
    fn maybe_begin_load_more(&mut self) -> Option<Vec<i64>> {
        if self.in_flight_more.is_some() || !self.near_tail() {
            return None;
        }
        let n = self.pending_ids.len().min(LOAD_MORE_BATCH);
        let batch: Vec<i64> = self.pending_ids.drain(..n).collect();
        self.in_flight_more = Some(batch.clone());
        Some(batch)
    }

    // --- state updates -----------------------------------------------------

    fn apply(&mut self, msg: Msg) {
        match msg {
            Msg::Loaded(Ok(loaded)) => {
                self.entries = loaded.entries;
                self.feed_titles = loaded.feed_titles;
                self.total_unread = loaded.total_unread;
                self.pending_ids = loaded.pending_ids;
                self.in_flight_more = None;
                self.status = Status::Ready;
                self.notice = None;
                // A reload / background auto-refresh keeps the reader's place:
                // preserve focus, selection, and scroll by id when the selected
                // source & article survived; reselect only when they've gone
                // (TASK-37). Selection is by id, so it "just works" with the new
                // entries; an initial load (no prior selection) reselects.
                self.preserve_or_reselect();
                // Keep the undo stack across a load so a silent auto-refresh
                // doesn't wipe a recent mark-read's undo (TASK-37); drop only
                // entries the fresh set re-added, so an undo can't duplicate a
                // now-present row.
                let present: HashSet<i64> = self.entries.iter().map(|e| e.id).collect();
                self.undo_stack
                    .retain(|u| !u.batch.iter().any(|(e, _)| present.contains(&e.id)));
                self.refill_image_queue();
            }
            Msg::Loaded(Err(err)) => {
                if self.entries.is_empty() {
                    self.status = Status::Failed(err);
                } else {
                    // Offline-first: a failed refresh keeps the cached view
                    // usable — surface a notice instead of blanking it (TASK-41).
                    self.notice = Some(format!("Couldn't reach Feedbin — showing cached: {err}"));
                }
            }

            Msg::Write {
                op: WriteOp::MarkRead,
                batch,
                result,
            } => match result {
                Ok(()) => self.undo_stack.push(Undone { batch }),
                Err(err) => {
                    // Roll the whole batch back into view and offer a retry hint.
                    let n = batch.len();
                    self.reinsert_batch(&batch);
                    self.notice = Some(match n {
                        1 => format!("Mark read failed (restored): {err}"),
                        n => format!("Mark {n} read failed (restored): {err}"),
                    });
                }
            },

            Msg::Write {
                op: WriteOp::Undo,
                batch,
                result,
            } => {
                if let Err(err) = result {
                    // The optimistic re-insert didn't stick server-side; take the
                    // batch back out, keeping it undoable for a retry.
                    for (entry, _) in &batch {
                        if let Some(pos) = self.entries.iter().position(|e| e.id == entry.id) {
                            self.entries.remove(pos);
                            self.total_unread = self.total_unread.saturating_sub(1);
                        }
                    }
                    self.preserve_or_reselect();
                    self.undo_stack.push(Undone { batch });
                    self.notice = Some(format!("Undo failed (kept read): {err}"));
                }
            }

            Msg::Image { url, result } => {
                let state = match result {
                    Ok(lines) => ImageState::Ready(lines),
                    Err(_) => ImageState::Failed,
                };
                self.images.insert(url, state);
                // Invalidate the reader cache: the currently-shown article may
                // include this image, so its render can change (TASK-28).
                self.image_generation = self.image_generation.wrapping_add(1);
            }

            Msg::LoadedMore(Ok(mut more)) => {
                // Append the older batch and re-sort so the newest-first
                // invariant holds; selection is by id, so it survives (TASK-40).
                self.entries.append(&mut more);
                self.entries.sort_by(|a, b| b.published.cmp(&a.published));
                self.in_flight_more = None;
                self.refill_image_queue();
            }
            Msg::LoadedMore(Err(err)) => {
                // Restore the in-flight batch to the front of pending_ids (in
                // original order) so it can be retried, and surface a notice.
                if let Some(mut batch) = self.in_flight_more.take() {
                    batch.append(&mut self.pending_ids);
                    self.pending_ids = batch;
                }
                self.notice = Some(format!("Load more failed (will retry): {err}"));
            }

            Msg::NotModified => {
                // 304: the unread set is unchanged, so keep the current view and
                // just settle a pending Loading state (TASK-42).
                if matches!(self.status, Status::Loading) {
                    self.status = Status::Ready;
                }
                self.notice = None;
            }
            // Validators are persisted by `persist_msg`; no UI effect.
            Msg::Validators(_) => {}
        }
    }

    /// Focus the first source and its first article (after a fresh load).
    fn reset_selection(&mut self) {
        self.focus = Focus::Sources;
        self.reader_scroll = 0;
        let first_source = self.sources().first().map(|s| s.0);
        self.selected_source = first_source;
        self.selected_article = first_source.and_then(|fid| self.article_ids(fid).first().copied());
    }

    /// After a reload / auto-refresh, keep the cursor where it was when the
    /// selected source *and* article still exist — leaving focus and scroll
    /// untouched so a mid-read isn't disturbed (TASK-37). When the source
    /// survived but its article didn't (e.g. marked read on another client),
    /// keep the source, pick its first article, and reset the reader. When the
    /// source is gone — or nothing was selected yet (initial load) — fall back
    /// to [`reset_selection`](Self::reset_selection).
    fn preserve_or_reselect(&mut self) {
        let source_present = self
            .selected_source
            .is_some_and(|fid| self.sources().iter().any(|(id, _)| *id == fid));
        if !source_present {
            self.reset_selection();
            return;
        }
        let feed_id = self.selected_source.expect("source is present");
        let article_present = self
            .selected_article
            .is_some_and(|aid| self.article_ids(feed_id).contains(&aid));
        if !article_present {
            self.selected_article = self.article_ids(feed_id).first().copied();
            self.reader_scroll = 0;
            if self.selected_article.is_none() {
                self.reset_selection();
            }
        }
        // else: the selected article survived — leave focus, selection, and
        // reader scroll exactly as they were.
    }

    /// Move the cursor up (`-1`) or down (`+1`) within the focused column.
    fn move_cursor(&mut self, delta: i32) {
        match self.focus {
            Focus::Sources => self.move_source(delta),
            Focus::Articles => self.move_article(delta),
            Focus::Reader => {
                self.reader_scroll = if delta > 0 {
                    self.reader_scroll.saturating_add(READER_SCROLL_STEP)
                } else {
                    self.reader_scroll.saturating_sub(READER_SCROLL_STEP)
                };
            }
        }
    }

    fn move_source(&mut self, delta: i32) {
        let sources = self.sources();
        if sources.is_empty() {
            return;
        }
        let current = self.source_index().unwrap_or(0) as i32;
        let next = (current + delta).clamp(0, sources.len() as i32 - 1) as usize;
        let feed_id = sources[next].0;
        self.selected_source = Some(feed_id);
        self.selected_article = self.article_ids(feed_id).first().copied();
        self.reader_scroll = 0;
    }

    fn move_article(&mut self, delta: i32) {
        let Some(feed_id) = self.selected_source else {
            return;
        };
        let ids = self.article_ids(feed_id);
        if ids.is_empty() {
            return;
        }
        let current = self.article_index().unwrap_or(0) as i32;
        let next = (current + delta).clamp(0, ids.len() as i32 - 1) as usize;
        self.selected_article = Some(ids[next]);
        self.reader_scroll = 0;
    }

    /// Jump to the first (`true`) or last (`false`) item in the focused column.
    fn move_to_edge(&mut self, first: bool) {
        match self.focus {
            Focus::Sources => {
                let sources = self.sources();
                let pick = if first {
                    sources.first()
                } else {
                    sources.last()
                };
                if let Some(&(feed_id, _)) = pick {
                    self.selected_source = Some(feed_id);
                    self.selected_article = self.article_ids(feed_id).first().copied();
                    self.reader_scroll = 0;
                }
            }
            Focus::Articles => {
                if let Some(feed_id) = self.selected_source {
                    let ids = self.article_ids(feed_id);
                    let pick = if first { ids.first() } else { ids.last() };
                    if let Some(&id) = pick {
                        self.selected_article = Some(id);
                        self.reader_scroll = 0;
                    }
                }
            }
            Focus::Reader => self.reader_scroll = if first { 0 } else { u16::MAX },
        }
    }

    /// Move focus one column to the right (sources → articles → reader).
    fn focus_right(&mut self) {
        match self.focus {
            Focus::Sources => {
                if self.selected_article.is_none()
                    && let Some(feed_id) = self.selected_source
                {
                    self.selected_article = self.article_ids(feed_id).first().copied();
                }
                if self.selected_article.is_some() {
                    self.focus = Focus::Articles;
                    self.reader_scroll = 0;
                }
            }
            Focus::Articles => {
                self.focus = Focus::Reader;
                self.reader_scroll = 0;
            }
            Focus::Reader => {}
        }
    }

    /// Move focus one column to the left (reader → articles → sources).
    fn focus_left(&mut self) {
        self.focus = match self.focus {
            Focus::Reader => Focus::Articles,
            Focus::Articles | Focus::Sources => Focus::Sources,
        };
        self.reader_scroll = 0;
    }

    /// Optimistically mark the selected article read (only when an article is
    /// the active target). Returns the removed entry + its index as a one-element
    /// batch for the write (TASK-30 unified the single and bulk paths).
    fn begin_mark_read(&mut self) -> Option<Vec<(Entry, usize)>> {
        if self.focus == Focus::Sources {
            return None;
        }
        let article = self.selected_article?;
        let hint = self.article_index().unwrap_or(0);
        let index = self.entries.iter().position(|e| e.id == article)?;
        let entry = self.entries.remove(index);
        self.total_unread = self.total_unread.saturating_sub(1);
        self.reselect_after_removal(entry.feed_id, hint);
        Some(vec![(entry, index)])
    }

    /// Optimistically mark every loaded article in the selected source read
    /// (`M`, TASK-30). Un-hydrated `pending_ids` for the source stay unread.
    /// Returns the removed batch for one batched write.
    fn begin_mark_source_read(&mut self) -> Option<Vec<(Entry, usize)>> {
        let feed_id = self.selected_source?;
        let indices: Vec<usize> = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, e)| e.feed_id == feed_id)
            .map(|(i, _)| i)
            .collect();
        self.remove_batch(indices)
    }

    /// Optimistically mark every loaded article read — the whole window (`A`,
    /// TASK-30). Scoped to loaded entries; `pending_ids` stay unread and the
    /// next batch auto-hydrates as usual (`near_tail`).
    fn begin_mark_window_read(&mut self) -> Option<Vec<(Entry, usize)>> {
        self.remove_batch((0..self.entries.len()).collect())
    }

    /// Remove the entries at `indices` as one batch, decrement the unread count,
    /// and reselect. Returns the `(entry, original index)` pairs in ascending-
    /// index order (so [`reinsert_batch`](Self::reinsert_batch) can restore them
    /// exactly), or `None` if the set was empty.
    fn remove_batch(&mut self, mut indices: Vec<usize>) -> Option<Vec<(Entry, usize)>> {
        if indices.is_empty() {
            return None;
        }
        indices.sort_unstable();
        // The source we're clearing, for reselection (arbitrary but harmless for
        // a whole-window mark, where every source empties anyway).
        let feed_id = self.entries[indices[0]].feed_id;
        // Remove back-to-front so earlier indices stay valid, then restore
        // ascending order for a clean undo.
        let mut batch: Vec<(Entry, usize)> = indices
            .iter()
            .rev()
            .map(|&i| (self.entries.remove(i), i))
            .collect();
        batch.reverse();
        self.total_unread = self.total_unread.saturating_sub(batch.len());
        self.reselect_after_removal(feed_id, 0);
        Some(batch)
    }

    /// Optimistically restore the most recent undo batch as a unit (`u`). A bulk
    /// mark restores in one step (TASK-30). Returns the batch for the write.
    fn begin_undo(&mut self) -> Option<Vec<(Entry, usize)>> {
        let batch = self.undo_stack.pop()?.batch;
        self.reinsert_batch(&batch);
        Some(batch)
    }

    /// Re-insert a removed batch at its original indices (ascending, so each
    /// lands correctly as the others fill in), bump the unread count, and focus
    /// the first restored entry. Shared by undo and mark-read rollback.
    fn reinsert_batch(&mut self, batch: &[(Entry, usize)]) {
        for (entry, index) in batch {
            let at = (*index).min(self.entries.len());
            self.entries.insert(at, entry.clone());
            self.total_unread = self.total_unread.saturating_add(1);
        }
        if let Some((entry, _)) = batch.first() {
            self.selected_source = Some(entry.feed_id);
            self.selected_article = Some(entry.id);
            self.focus = Focus::Articles;
            self.reader_scroll = 0;
        }
    }

    /// After removing an article from `feed_id`, pick the next sensible
    /// selection: stay near `hint` in the same source, or — if it emptied — drop
    /// focus to the sources column and pick the first remaining source.
    fn reselect_after_removal(&mut self, feed_id: i64, hint: usize) {
        let ids = self.article_ids(feed_id);
        if !ids.is_empty() {
            self.selected_source = Some(feed_id);
            self.selected_article = Some(ids[hint.min(ids.len() - 1)]);
        } else {
            let first_source = self.sources().first().map(|s| s.0);
            self.selected_source = first_source;
            self.selected_article =
                first_source.and_then(|fid| self.article_ids(fid).first().copied());
            self.focus = Focus::Sources;
        }
        self.reader_scroll = 0;
    }

    /// Handle one key press; returns an [`Action`] the run loop must drive.
    fn handle_key(&mut self, code: KeyCode) -> Action {
        // The help overlay is modal: while it's open, any key just dismisses it
        // (TASK-32) — `?`/`Esc`/`q` included — and does nothing else.
        if self.show_help {
            self.show_help = false;
            return Action::None;
        }
        // A pending confirmation swallows the next key (TASK-30): `y`/`Y`
        // proceeds, anything else (incl. `n`/Esc/navigation) cancels.
        if let Some(confirm) = self.pending_confirm.take() {
            return match (confirm, code) {
                (Confirm::MarkWindowRead, KeyCode::Char('y' | 'Y')) => Action::MarkWindowRead,
                _ => Action::None,
            };
        }
        self.notice = None;
        match code {
            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
            KeyCode::Down | KeyCode::Char('j') => self.move_cursor(1),
            KeyCode::Up | KeyCode::Char('k') => self.move_cursor(-1),
            KeyCode::Right | KeyCode::Char('l') => self.focus_right(),
            KeyCode::Left | KeyCode::Char('h') => self.focus_left(),
            KeyCode::Char('g') | KeyCode::Home => self.move_to_edge(true),
            KeyCode::Char('G') | KeyCode::End => self.move_to_edge(false),
            KeyCode::PageDown if self.focus == Focus::Reader => {
                self.reader_scroll = self.reader_scroll.saturating_add(READER_PAGE);
            }
            KeyCode::PageUp if self.focus == Focus::Reader => {
                self.reader_scroll = self.reader_scroll.saturating_sub(READER_PAGE);
            }
            KeyCode::Char('m') => return Action::MarkRead,
            KeyCode::Char('M') => return Action::MarkSourceRead,
            // The big one asks first; no-op if there's nothing loaded to mark.
            KeyCode::Char('A') => {
                if !self.entries.is_empty() {
                    self.pending_confirm = Some(Confirm::MarkWindowRead);
                }
            }
            KeyCode::Char('u') => return Action::Undo,
            KeyCode::Char('o') => return Action::OpenInBrowser,
            KeyCode::Char('r') => return Action::Reload,
            KeyCode::Char('?') => self.show_help = true,
            _ => {}
        }
        Action::None
    }

    /// The footer confirmation prompt, if a confirmation is pending (TASK-30).
    fn confirm_prompt(&self) -> Option<String> {
        self.pending_confirm.map(|confirm| match confirm {
            Confirm::MarkWindowRead => format!(
                "Mark all {} loaded articles read?  y / n",
                self.entries.len()
            ),
        })
    }

    // --- rendering ---------------------------------------------------------

    fn draw(&mut self, frame: &mut Frame) {
        let [main, footer] =
            Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(frame.area());

        // When everything is read (Ready with nothing unread), show the rose in
        // place of empty columns (TASK-14). Loading and failure states keep the
        // three-column view (their status text lives in the Sources pane).
        if matches!(self.status, Status::Ready) && self.entries.is_empty() {
            draw_caught_up(frame, main);
        } else {
            let [sources, articles, reader] = Layout::horizontal([
                Constraint::Percentage(25),
                Constraint::Percentage(35),
                Constraint::Percentage(40),
            ])
            .areas(main);

            // Record the reader's content width every frame so background image
            // pre-fetches size their art to fit even before the reader is opened.
            // Mirror draw_reader's inner rect (border + padding) so the art width
            // matches the real content area.
            self.reader_width = self.column_block("Reader", false).inner(reader).width;

            self.draw_sources(frame, sources);
            self.draw_articles(frame, articles);
            self.draw_reader(frame, reader);
        }

        // A pending confirmation (TASK-30) takes over the footer — accented, not
        // red — over any notice or the normal help.
        let footer_line = if let Some(prompt) = self.confirm_prompt() {
            Line::from(format!(" {prompt} "))
                .fg(self.base_accent)
                .bold()
        } else {
            match &self.notice {
                Some(text) => Line::from(format!(" {text} ")).red(),
                None => footer_help(self.base_accent),
            }
        };
        // Right-aligned footer slot: the image loading indicator while images are
        // still resolving, otherwise a "showing X of Y unread" hint whenever more
        // unread entries remain un-hydrated (TASK-40). Suppressed while a
        // confirmation prompt owns the footer so it can't crowd the prompt.
        let right_indicator = if self.pending_confirm.is_some() {
            None
        } else {
            match self.image_progress() {
                Some((done, total)) => Some(loading_indicator(done, total, self.spinner_tick)),
                None if !self.pending_ids.is_empty() => Some(format!(
                    "↓ {} of {} unread",
                    self.entries.len(),
                    self.total_unread
                )),
                None => None,
            }
        };
        match right_indicator {
            // Reserve the right columns so the indicator never overlaps the help;
            // the help side flexes and truncates if narrow.
            Some(indicator) => {
                let width = indicator.chars().count() as u16 + 1;
                let [help_area, indicator_area] =
                    Layout::horizontal([Constraint::Min(0), Constraint::Length(width)])
                        .areas(footer);
                frame.render_widget(footer_line, help_area);
                frame.render_widget(
                    Paragraph::new(Line::from(format!(" {indicator}")).dim())
                        .alignment(Alignment::Right),
                    indicator_area,
                );
            }
            None => frame.render_widget(footer_line, footer),
        }

        // The help overlay floats above everything else (TASK-32); it's pure
        // chrome, so the underlying App state (selection, loads) is untouched.
        if self.show_help {
            draw_help_overlay(frame, main, self.base_accent);
        }
    }

    /// The accent color for focused chrome + the selection bar: the configured
    /// accent (`base_accent`, rose by default — TASK-45), muted to grey while the
    /// help overlay is open so the overlay draws the eye (TASK-46). Purely a
    /// display choice — focus/selection state is untouched, and the
    /// overlay/footer/reader-title read `base_accent` directly (never muted).
    fn accent(&self) -> Color {
        if self.show_help {
            theme::MUTED
        } else {
            self.base_accent
        }
    }

    fn column_block(&self, title: &'static str, focused: bool) -> Block<'static> {
        // The focused pane lights up in the accent (border + title); unfocused panes
        // stay neutral (dim border, plain title) so only the active column draws the
        // eye (TASK-14, chrome-only accent). The accent mutes to grey under the help
        // overlay (TASK-46).
        let (border, title_line) = if focused {
            let accent = self.accent();
            (
                Style::new().fg(accent).bold(),
                Line::from(Span::styled(title, Style::new().fg(accent).bold())),
            )
        } else {
            (Style::new().dim(), Line::from(title))
        };
        // Inset the content from the border for breathing room (TASK-12): one cell
        // of horizontal padding, no top/bottom inset, applied to every pane via this
        // shared block so the padding stays consistent. `draw_reader` derives its
        // scroll bounds from `block.inner(area)`, so this padding is accounted for
        // there automatically.
        Block::bordered()
            .title(title_line)
            .border_style(border)
            .padding(Padding::horizontal(1))
    }

    /// A rose-tinted reversed bar marks the active cursor; a bold row marks the
    /// remembered selection in an unfocused column. `reversed()` swaps the rose
    /// foreground onto the background, so the selection reads as a rose bar
    /// (TASK-14) while keeping the `REVERSED` modifier.
    fn highlight(&self, focused: bool) -> Style {
        if focused {
            Style::new().fg(self.accent()).reversed()
        } else {
            Style::new().bold()
        }
    }

    fn draw_sources(&self, frame: &mut Frame, area: Rect) {
        let focused = self.focus == Focus::Sources;
        let block = self.column_block("Sources", focused);
        let sources = self.sources();
        if sources.is_empty() {
            let message = match &self.status {
                Status::Loading => "Loading…".to_string(),
                Status::Failed(err) => format!("Failed to load: {err}"),
                Status::Ready => "No unread sources".to_string(),
            };
            frame.render_widget(
                Paragraph::new(message)
                    .block(block)
                    .wrap(Wrap { trim: true }),
                area,
            );
            return;
        }
        let items: Vec<ListItem> = sources
            .iter()
            .map(|&(feed_id, count)| {
                ListItem::new(Line::from(vec![
                    Span::raw(strip_control_chars(self.feed_name(feed_id))),
                    Span::raw("  "),
                    Span::styled(format!("({count})"), Style::new().dim()),
                ]))
            })
            .collect();
        let list = List::new(items)
            .block(block)
            .highlight_style(self.highlight(focused));
        let mut state = ListState::default();
        state.select(self.source_index());
        frame.render_stateful_widget(list, area, &mut state);
    }

    fn draw_articles(&self, frame: &mut Frame, area: Rect) {
        let focused = self.focus == Focus::Articles;
        let block = self.column_block("Articles", focused);
        let Some(feed_id) = self.selected_source else {
            frame.render_widget(Paragraph::new("").block(block), area);
            return;
        };
        // Wrap each title to the pane's current inner width (recomputed every draw,
        // so resizes reflow). Each article is one multi-line `ListItem`, so
        // navigation and the selection highlight stay per-article — `List`
        // highlights the whole item (TASK-13).
        let width = block.inner(area).width;
        let items: Vec<ListItem> = self
            .articles(feed_id)
            .iter()
            .map(|e| {
                let title = e
                    .title
                    .as_deref()
                    .map(str::trim)
                    .filter(|t| !t.is_empty())
                    .unwrap_or("(untitled)");
                let title = strip_control_chars(title);
                let lines: Vec<Line> = wrap_title(&title, width)
                    .into_iter()
                    .map(Line::from)
                    .collect();
                ListItem::new(Text::from(lines))
            })
            .collect();
        let list = List::new(items)
            .block(block)
            .highlight_style(self.highlight(focused));
        let mut state = ListState::default();
        state.select(self.article_index());
        frame.render_stateful_widget(list, area, &mut state);
    }

    /// Ensure `reader_cache` holds the render for `entry_id` at `width`,
    /// rebuilding (re-parsing the article HTML) only on a key miss. Returns
    /// whether it rebuilt. The key folds in `image_generation`, so the cache
    /// invalidates when a visible image finishes loading (TASK-28).
    fn ensure_reader_cache(&mut self, entry_id: i64, width: u16) -> bool {
        let key = (entry_id, width, self.image_generation);
        if self.reader_cache.as_ref().map(|c| c.key) == Some(key) {
            return false;
        }
        let Some(entry) = self.entries.iter().find(|e| e.id == entry_id) else {
            return false;
        };
        // `base_accent` is constant for the process, so it needn't key the cache.
        let text = reader_text(entry, &self.images, width, self.base_accent);
        let wrapped = Paragraph::new(text.clone())
            .wrap(Wrap { trim: false })
            .line_count(width) as u16;
        self.reader_cache = Some(ReaderCache { key, text, wrapped });
        true
    }

    fn draw_reader(&mut self, frame: &mut Frame, area: Rect) {
        let focused = self.focus == Focus::Reader;
        let block = self.column_block("Reader", focused);
        // The reader shows the article only when focus has moved off the
        // sources column (TASK-11: a focused source shows nothing here).
        let entry_id = match self.focus {
            Focus::Sources => None,
            Focus::Articles | Focus::Reader => self.selected_article,
        };
        let Some(entry_id) = entry_id.filter(|id| self.entries.iter().any(|e| e.id == *id)) else {
            frame.render_widget(Paragraph::new("").block(block), area);
            return;
        };

        // The true content rect — inside the border *and* the padding (TASK-12) —
        // sets the wrap width and visible height used to clamp the scroll offset.
        let inner = block.inner(area);
        let inner_width = inner.width;
        let inner_height = inner.height;

        // Rebuild the reader Text only when the article, width, or image state
        // changed; otherwise reuse the memoized render, so scrolling and idle
        // frames don't re-parse the article HTML (TASK-28). `reader_text` clips
        // image art to `inner_width` so stale-width art can't wrap into
        // half-height fragment rows after a resize.
        self.ensure_reader_cache(entry_id, inner_width);
        let (text, wrapped) = {
            let cache = self.reader_cache.as_ref().expect("cache populated above");
            (cache.text.clone(), cache.wrapped)
        };

        // Clamp scroll to the *wrapped* height (not the raw line count): one long
        // paragraph is a single line that word-wraps to many rows, so clamping on
        // `text.lines.len()` would pin the reader at the top.
        let max_scroll = wrapped.saturating_sub(inner_height);
        self.reader_scroll = self.reader_scroll.min(max_scroll);

        let reader = Paragraph::new(text)
            .block(block)
            .wrap(Wrap { trim: false })
            .scroll((self.reader_scroll, 0));
        frame.render_widget(reader, area);

        // Scrollbar on the reader's right edge, shown only while the reader is the
        // active pane and its wrapped content overflows the viewport (TASK-15). The
        // track rides the right border between the corners (vertical inset of 1).
        if focused && wrapped > inner_height {
            // `content_length` is the count of scroll *positions* (0..=max_scroll),
            // not the total line count: ratatui's thumb only reaches the bottom of
            // the track when `position == content_length − 1`, so pass
            // `max_scroll + 1`. The viewport length sizes the thumb to the visible
            // fraction.
            let mut scrollbar_state = ScrollbarState::new(max_scroll as usize + 1)
                .viewport_content_length(inner_height as usize)
                .position(self.reader_scroll as usize);
            let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(None)
                .end_symbol(None)
                .track_symbol(Some("│"))
                .thumb_symbol("█");
            frame.render_stateful_widget(
                scrollbar,
                area.inner(Margin {
                    vertical: 1,
                    horizontal: 0,
                }),
                &mut scrollbar_state,
            );
        }
    }
}

/// The footer key-help line, with the action keys accented in rose (TASK-14); the
/// arrows, labels, and separators stay dim.
/// One keyboard binding. This table is the **single source of truth** for both
/// the 1-line footer hint and the `?` help overlay (TASK-32), so the two can't
/// drift apart as bindings are added. Entries are grouped for the overlay by
/// consecutive `group`; `footer` is `Some((keys, label))` for the compact subset
/// shown in the footer, else the binding is overlay-only (e.g. `M`/`A`).
struct Binding {
    group: &'static str,
    keys: &'static str,
    desc: &'static str,
    footer: Option<(&'static str, &'static str)>,
}

const BINDINGS: &[Binding] = &[
    Binding {
        group: "Navigation",
        keys: "↑ ↓  k j",
        desc: "Move within the column",
        footer: Some(("↑↓", "move")),
    },
    Binding {
        group: "Navigation",
        keys: "← →  h l",
        desc: "Change the focused column",
        footer: Some(("←→", "focus")),
    },
    Binding {
        group: "Navigation",
        keys: "g / G",
        desc: "First / last item",
        footer: None,
    },
    Binding {
        group: "Reading",
        keys: "PgUp / PgDn",
        desc: "Page the reader",
        footer: None,
    },
    Binding {
        group: "Reading",
        keys: "o",
        desc: "Open in browser",
        footer: Some(("o", "open")),
    },
    Binding {
        group: "Marking read",
        keys: "m",
        desc: "Mark the article read",
        footer: Some(("m", "read")),
    },
    Binding {
        group: "Marking read",
        keys: "M",
        desc: "Mark the source read",
        footer: None,
    },
    Binding {
        group: "Marking read",
        keys: "A",
        desc: "Mark the window read (asks y / n)",
        footer: None,
    },
    Binding {
        group: "Marking read",
        keys: "u",
        desc: "Undo the last mark",
        footer: Some(("u", "undo")),
    },
    Binding {
        group: "App",
        keys: "r",
        desc: "Reload",
        footer: Some(("r", "reload")),
    },
    Binding {
        group: "App",
        keys: "? / Esc",
        desc: "Toggle this help",
        footer: Some(("?", "help")),
    },
    Binding {
        group: "App",
        keys: "q",
        desc: "Quit",
        footer: Some(("q", "quit")),
    },
];

/// The 1-line footer hint: the footer-flagged bindings as `keys label · …`, keys
/// in the `accent` color and labels dim, derived from [`BINDINGS`] so it never
/// drifts. `accent` is the configured highlight color (TASK-45).
fn footer_help(accent: Color) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = vec![Span::raw(" ")];
    let mut first = true;
    for b in BINDINGS {
        let Some((keys, label)) = b.footer else {
            continue;
        };
        if !first {
            spans.push(Span::raw(" · ").dim());
        }
        first = false;
        spans.push(Span::styled(keys, Style::new().fg(accent).bold()));
        spans.push(Span::raw(format!(" {label}")).dim());
    }
    spans.push(Span::raw(" "));
    Line::from(spans)
}

/// The help-overlay body (TASK-32): every binding, grouped under bold headings,
/// with an `accent`-colored key column aligned to the widest key. Built from the
/// same [`BINDINGS`] table as the footer.
fn help_lines(accent: Color) -> Vec<Line<'static>> {
    let key_w = BINDINGS
        .iter()
        .map(|b| UnicodeWidthStr::width(b.keys))
        .max()
        .unwrap_or(0);
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut group = "";
    for b in BINDINGS {
        if b.group != group {
            if !group.is_empty() {
                lines.push(Line::from(""));
            }
            lines.push(Line::from(Span::raw(b.group).bold()));
            group = b.group;
        }
        let pad = key_w.saturating_sub(UnicodeWidthStr::width(b.keys)) + 2;
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(b.keys, Style::new().fg(accent).bold()),
            Span::raw(" ".repeat(pad)),
            Span::raw(b.desc),
        ]));
    }
    lines
}

/// Draw the keybinding help overlay: a centered, `accent`-bordered box floating
/// over `area`, sized to its content (clamped to `area`). Pure chrome — it reads
/// no mutable state, so background loading and selection are unaffected (TASK-32).
fn draw_help_overlay(frame: &mut Frame, area: Rect, accent: Color) {
    let lines = help_lines(accent);
    let content_w = lines.iter().map(Line::width).max().unwrap_or(0) as u16;
    let title = " Keyboard shortcuts ";
    let title_w = UnicodeWidthStr::width(title) as u16;
    // + border (2) + horizontal padding (2).
    let width = (content_w.max(title_w) + 4).min(area.width);
    let height = (lines.len() as u16 + 2).min(area.height); // + top/bottom border
    let rect = centered_rect(area, width, height);
    let block = Block::bordered()
        .border_style(Style::new().fg(accent))
        .title(Span::styled(title, Style::new().fg(accent).bold()))
        .padding(Padding::horizontal(1));
    frame.render_widget(Clear, rect);
    frame.render_widget(Paragraph::new(lines).block(block), rect);
}

/// A `width`×`height` rect centered within `area` (both axes) via `Flex::Center`.
fn centered_rect(area: Rect, width: u16, height: u16) -> Rect {
    let [row] = Layout::vertical([Constraint::Length(height)])
        .flex(Flex::Center)
        .areas(area);
    let [cell] = Layout::horizontal([Constraint::Length(width)])
        .flex(Flex::Center)
        .areas(row);
    cell
}

/// Braille spinner frames for the background-loading indicator (TASK-19).
const SPINNER_FRAMES: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

/// The lower-right loading indicator text, e.g. "⠙ Loading 4 of 19 images". Pure
/// (the animation frame is passed in) so a test can assert it with a frozen
/// `tick`, per the project's no-flaky-tests rule.
fn loading_indicator(done: usize, total: usize, tick: usize) -> String {
    let frame = SPINNER_FRAMES[tick % SPINNER_FRAMES.len()];
    format!("{frame} Loading {done} of {total} images")
}

/// The "all caught up" rose (TASK-14): a vertically-centered ASCII rose graded
/// light→deep rose over a green stem, with a caption beneath. Degrades to just the
/// centered caption when the area is too small for the art.
fn draw_caught_up(frame: &mut Frame, area: Rect) {
    // Each row is padded to one width so centering keeps the bloom symmetric.
    const ART: [&str; 7] = [
        "  .---.  ",
        " / .-. \\ ",
        "| ( @ ) |",
        " \\ `-' / ",
        "  `---'  ",
        "   \\|/   ",
        "    |    ",
    ];
    const CAPTION: &str = "All caught up";
    /// The first rows are the bloom (rose gradient); the rest are the stem/leaf.
    const PETAL_ROWS: usize = 5;

    let art_w = ART
        .iter()
        .map(|line| UnicodeWidthStr::width(*line))
        .chain(std::iter::once(UnicodeWidthStr::width(CAPTION)))
        .max()
        .unwrap_or(0) as u16;
    let art_h = ART.len() as u16 + 2; // blank spacer + caption

    // Too small for the rose: show just the centered caption (still informative).
    if area.height < art_h || area.width < art_w {
        let [band] = Layout::vertical([Constraint::Length(1)])
            .flex(Flex::Center)
            .areas(area);
        let caption = Line::from(CAPTION);
        frame.render_widget(Paragraph::new(caption).alignment(Alignment::Center), band);
        return;
    }

    // All art rows share one width, so centering each line keeps the art's internal
    // alignment while centering the whole bloom.
    let mut lines: Vec<Line> = ART
        .iter()
        .enumerate()
        .map(|(row, art)| {
            let color = if row < PETAL_ROWS {
                let t = row as f32 / (PETAL_ROWS - 1) as f32;
                theme::lerp(theme::ROSE_LIGHT, theme::ROSE_DEEP, t)
            } else {
                theme::LEAF
            };
            Line::from(Span::styled((*art).to_string(), Style::new().fg(color)))
        })
        .collect();
    lines.push(Line::from(""));
    lines.push(Line::from(CAPTION));

    let [band] = Layout::vertical([Constraint::Length(art_h)])
        .flex(Flex::Center)
        .areas(area);
    frame.render_widget(Paragraph::new(lines).alignment(Alignment::Center), band);
}

/// Word-wrap `text` to `width` display columns for the articles list, breaking on
/// whitespace and hard-splitting any single word wider than the line. Widths use
/// Unicode display width (matching how the terminal measures cells), so wrapped
/// lines don't overflow and get truncated. Always returns at least one line.
fn wrap_title(text: &str, width: u16) -> Vec<String> {
    let width = usize::from(width.max(1));
    let mut lines: Vec<String> = Vec::new();
    let mut line = String::new();
    let mut line_width = 0usize;
    for word in text.split_whitespace() {
        let word_width = UnicodeWidthStr::width(word);
        // A word too wide for any line: flush, then emit it in width-sized pieces.
        if word_width > width {
            if line_width > 0 {
                lines.push(std::mem::take(&mut line));
                line_width = 0;
            }
            for ch in word.chars() {
                let char_width = UnicodeWidthChar::width(ch).unwrap_or(0);
                if line_width + char_width > width && line_width > 0 {
                    lines.push(std::mem::take(&mut line));
                    line_width = 0;
                }
                line.push(ch);
                line_width += char_width;
            }
            continue;
        }
        let separator = usize::from(line_width > 0);
        if line_width + separator + word_width > width {
            lines.push(std::mem::take(&mut line));
            line_width = 0;
        }
        if line_width > 0 {
            line.push(' ');
            line_width += 1;
        }
        line.push_str(word);
        line_width += word_width;
    }
    if line_width > 0 || lines.is_empty() {
        lines.push(line);
    }
    lines
}

/// chrono layout for the reader's published date, e.g.
/// "Sunday, June 15, 2026 at 6:00 AM". `%-d`/`%-I` drop the leading zero —
/// chrono honors the `-` no-pad flag on every platform (unlike libc strftime).
const PUBLISHED_FORMAT: &str = "%A, %B %-d, %Y at %-I:%M %p";

/// Parse a Feedbin RFC 3339 timestamp (e.g. "2023-12-02T02:30:21.000000Z") and
/// render it in the host's local timezone. Returns `None` for a missing or
/// unparseable value so the caller can simply omit the line (TASK-17 AC #3).
fn format_published(raw: &str) -> Option<String> {
    format_published_in(raw, &Local)
}

/// The timezone-agnostic core of [`format_published`], split out so it can be
/// unit-tested against a fixed offset: `Local` reads the host timezone, which
/// would make the test machine-dependent (and we have zero tolerance for flaky
/// tests). Production passes `&Local`; tests pass `&Utc` / a `FixedOffset`.
fn format_published_in<Tz>(raw: &str, tz: &Tz) -> Option<String>
where
    Tz: chrono::TimeZone,
    Tz::Offset: std::fmt::Display,
{
    let parsed = DateTime::parse_from_rfc3339(raw).ok()?;
    Some(
        parsed
            .with_timezone(tz)
            .format(PUBLISHED_FORMAT)
            .to_string(),
    )
}

/// Truncate a line to at most `max_width` display columns. Used to clamp cached
/// image art to the reader's *current* inner width: art is rendered once at the
/// width it was fetched at, so if the terminal later narrows, an over-wide art
/// line would wrap into a full row + a short fragment (the "half-height rows"
/// artifact). Clipping keeps each art line within the wrap width so it never
/// wraps — a no-op when the art already fits, a graceful right-crop when it
/// doesn't (until a reload re-renders it at the new width).
fn clip_line_to_width(line: &Line<'static>, max_width: u16) -> Line<'static> {
    let max = max_width as usize;
    let mut used = 0usize;
    let mut spans: Vec<Span<'static>> = Vec::new();
    for span in &line.spans {
        let span_width = span.content.width();
        if used + span_width <= max {
            spans.push(span.clone());
            used += span_width;
        } else {
            // This span crosses the limit: keep whole chars up to the remainder.
            let remaining = max - used;
            let mut clipped = String::new();
            let mut w = 0usize;
            for ch in span.content.chars() {
                let cw = ch.width().unwrap_or(0);
                if w + cw > remaining {
                    break;
                }
                clipped.push(ch);
                w += cw;
            }
            if !clipped.is_empty() {
                spans.push(Span::styled(clipped, span.style));
            }
            break;
        }
    }
    Line::from(spans)
}

/// Render one image URL into the reader lines: its cached half-block art
/// (clipped to `max_width`), or a loading / unavailable placeholder. Shared by
/// inline body images and the extended-mode lead image (TASK-21).
fn push_image(
    lines: &mut Vec<Line<'static>>,
    url: &str,
    images: &HashMap<String, ImageState>,
    max_width: u16,
) {
    match images.get(url) {
        Some(ImageState::Ready(art)) => {
            lines.push(Line::from(""));
            lines.extend(art.iter().map(|line| clip_line_to_width(line, max_width)));
            lines.push(Line::from(""));
        }
        Some(ImageState::Failed) => {
            lines.push(Line::from(format!("[image unavailable: {url}]")).dim());
        }
        _ => lines.push(Line::from(format!("[image loading… {url}]")).dim()),
    }
}

/// A short podcast/media indicator for the reader header (TASK-22), e.g.
/// "Audio · 47:03" — the media kind from the enclosure type, plus a formatted
/// duration when present.
fn podcast_indicator(enc: &Enclosure) -> String {
    let kind = match enc.enclosure_type.as_deref() {
        Some(t) if t.starts_with("video") => "Video",
        Some(t) if t.starts_with("audio") => "Audio",
        _ => "Media",
    };
    match enc
        .itunes_duration
        .as_deref()
        .map(str::trim)
        .filter(|d| !d.is_empty())
    {
        Some(d) => format!("{kind} · {}", format_duration(d)),
        None => kind.to_string(),
    }
}

/// Format an iTunes duration: bare seconds become `H:MM:SS` / `M:SS`; anything
/// else (Feedbin sometimes sends `HH:MM:SS` already) passes through.
fn format_duration(raw: &str) -> String {
    match raw.parse::<u64>() {
        Ok(secs) => {
            let (h, m, s) = (secs / 3600, (secs % 3600) / 60, secs % 60);
            if h > 0 {
                format!("{h}:{m:02}:{s:02}")
            } else {
                format!("{m}:{s:02}")
            }
        }
        Err(_) => raw.to_string(),
    }
}

/// Build the reader pane's content for one entry: a title / author·date / link
/// header, then the body — text rendered from HTML, with images shown as
/// half-block art (a placeholder while loading, a notice when unavailable).
/// `max_width` is the reader's current inner width; image art is clipped to it
/// so stale-width art (after a resize) can't wrap into fragment rows.
fn reader_text(
    entry: &Entry,
    images: &HashMap<String, ImageState>,
    max_width: u16,
    accent: Color,
) -> Text<'static> {
    let mut lines: Vec<Line> = Vec::new();
    let title = strip_control_chars(entry.title.as_deref().unwrap_or("(untitled)"));
    lines.push(Line::from(title.bold().fg(accent)));
    // Meta line: author (if any) · formatted date (if any). The feed/blog name
    // is intentionally omitted — it's already the highlighted source on the
    // left, so we show the author instead when Feedbin gives us one (TASK-18),
    // and the human-readable published date in place of the raw ISO string
    // (TASK-17). When we have neither, the line is dropped entirely.
    let mut meta_parts: Vec<String> = Vec::new();
    if let Some(author) = entry
        .author
        .as_deref()
        .map(str::trim)
        .filter(|a| !a.is_empty())
    {
        meta_parts.push(strip_control_chars(author));
    }
    if let Some(date) = entry.published.as_deref().and_then(format_published) {
        meta_parts.push(date);
    }
    if !meta_parts.is_empty() {
        lines.push(Line::from(meta_parts.join(" · ").dim()));
    }
    // Podcast/media indicator when there's an enclosure (TASK-22).
    if let Some(enc) = &entry.enclosure {
        lines.push(Line::from(strip_control_chars(&podcast_indicator(enc))).dim());
    }
    // Link line(s): a link-blog entry points out to `external_url`, so show that
    // as the primary link (what `o` opens) and keep the permalink visible so it
    // stays accessible (TASK-23). Otherwise just the entry url.
    match (entry.external_url(), &entry.url) {
        (Some(external), permalink) => {
            lines.push(Line::from(strip_control_chars(external).underlined()));
            if let Some(url) = permalink {
                lines.push(Line::from(format!("permalink: {}", strip_control_chars(url))).dim());
            }
        }
        (None, Some(url)) => lines.push(Line::from(strip_control_chars(url).underlined())),
        (None, None) => {}
    }
    lines.push(Line::from(""));

    let body = entry
        .content
        .as_deref()
        .or(entry.summary.as_deref())
        .unwrap_or("(no content)");
    let blocks = content_blocks(body);
    // Lead image (TASK-21): Feedbin's extracted hero image, shown at the top only
    // when the body has no inline <img> of its own — so image-rich articles are
    // unchanged and metadata-only feeds still get a picture.
    let has_inline_image = blocks.iter().any(|b| matches!(b, Segment::Image(_)));
    if !has_inline_image && let Some(url) = entry.lead_image_url() {
        push_image(&mut lines, url, images, max_width);
    }
    for block in blocks {
        match block {
            Segment::Text(text) => {
                for line in text.lines() {
                    lines.push(Line::from(line.to_string()));
                }
            }
            Segment::Image(url) => push_image(&mut lines, &url, images, max_width),
        }
    }
    Text::from(lines)
}

/// The lowercased tag name (e.g. `</P >` -> `p`).
fn tag_name(tag: &str) -> String {
    tag.trim_start_matches('/')
        .split([' ', '\t', '\n', '/'])
        .next()
        .unwrap_or("")
        .to_ascii_lowercase()
}

fn is_block_tag(tag: &str) -> bool {
    matches!(
        tag_name(tag).as_str(),
        "p" | "br"
            | "div"
            | "li"
            | "tr"
            | "ul"
            | "ol"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "blockquote"
            | "section"
            | "article"
            | "header"
            | "footer"
            | "hr"
            | "pre"
            | "table"
    )
}

/// Extract the `src` URL from an `<img …>` tag's inner text, ignoring lookalike
/// attributes such as `srcset`. Returns `None` when there's no usable `src`.
fn extract_img_src(tag: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let bytes = tag.as_bytes();
    let mut from = 0;
    while let Some(rel) = lower[from..].find("src") {
        let pos = from + rel;
        let boundary_ok = pos == 0 || !lower.as_bytes()[pos - 1].is_ascii_alphanumeric();
        let mut i = pos + 3;
        while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
            i += 1;
        }
        if boundary_ok && i < bytes.len() && bytes[i] == b'=' {
            let mut j = i + 1;
            while j < bytes.len() && (bytes[j] == b' ' || bytes[j] == b'\t') {
                j += 1;
            }
            let value = &tag[j..];
            let src = match value.chars().next() {
                Some(quote @ ('"' | '\'')) => {
                    let rest = &value[1..];
                    let end = rest.find(quote).unwrap_or(rest.len());
                    rest[..end].trim().to_string()
                }
                _ => {
                    let end = value.find(char::is_whitespace).unwrap_or(value.len());
                    value[..end].trim().to_string()
                }
            };
            return (!src.is_empty()).then_some(src);
        }
        from = pos + 3;
    }
    None
}

/// Split entry HTML into ordered text/image segments for the reader. Text
/// segments are tag-stripped, entity-decoded, and control-char-stripped (see
/// [`sanitize`] / [`decode_entities`]); each `<img>` becomes a
/// `Segment::Image(src)`.
fn content_blocks(html: &str) -> Vec<Segment> {
    fn flush(text: &mut String, blocks: &mut Vec<Segment>) {
        let rendered = sanitize(&decode_entities(text));
        if !rendered.is_empty() {
            blocks.push(Segment::Text(rendered));
        }
        text.clear();
    }

    let mut blocks = Vec::new();
    let mut text = String::new();
    let mut chars = html.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '<' {
            let mut tag = String::new();
            for n in chars.by_ref() {
                if n == '>' {
                    break;
                }
                tag.push(n);
            }
            if tag_name(&tag) == "img" {
                flush(&mut text, &mut blocks);
                if let Some(src) = extract_img_src(&tag) {
                    blocks.push(Segment::Image(src));
                }
            } else if is_block_tag(&tag) {
                text.push('\n');
            }
        } else {
            text.push(c);
        }
    }
    flush(&mut text, &mut blocks);
    blocks
}

fn decode_entities(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '&' {
            out.push(c);
            continue;
        }
        let mut ent = String::new();
        let mut terminated = false;
        for _ in 0..12 {
            match chars.peek() {
                Some(';') => {
                    chars.next();
                    terminated = true;
                    break;
                }
                Some('&') | None => break,
                Some(&ch) => {
                    ent.push(ch);
                    chars.next();
                }
            }
        }
        match terminated.then(|| decode_entity(&ent)).flatten() {
            Some(decoded) => out.push_str(&decoded),
            None => {
                out.push('&');
                out.push_str(&ent);
                if terminated {
                    out.push(';');
                }
            }
        }
    }
    out
}

fn decode_entity(ent: &str) -> Option<String> {
    let named = match ent {
        "amp" => "&",
        "lt" => "<",
        "gt" => ">",
        "quot" => "\"",
        "apos" => "'",
        "nbsp" => " ",
        "mdash" => "—",
        "ndash" => "–",
        "hellip" => "…",
        "rsquo" | "lsquo" => "'",
        "ldquo" | "rdquo" => "\"",
        "copy" => "©",
        "trade" => "™",
        _ => {
            let code = if let Some(hex) = ent.strip_prefix("#x").or_else(|| ent.strip_prefix("#X"))
            {
                u32::from_str_radix(hex, 16).ok()?
            } else {
                ent.strip_prefix('#')?.parse().ok()?
            };
            return char::from_u32(code).map(String::from);
        }
    };
    Some(named.to_string())
}

/// Drop control characters (keeping `\n`/`\t`), trim line ends, and collapse
/// runs of blank lines so the reader body stays tidy and escape-free.
fn sanitize(s: &str) -> String {
    let cleaned: String = s
        .chars()
        .filter(|&c| c == '\n' || c == '\t' || !c.is_control())
        .collect();

    let mut out_lines: Vec<&str> = Vec::new();
    let mut prev_blank = false;
    for line in cleaned.lines() {
        let trimmed = line.trim_end();
        let blank = trimmed.trim().is_empty();
        if blank && prev_blank {
            continue;
        }
        out_lines.push(trimmed);
        prev_blank = blank;
    }
    out_lines.join("\n").trim().to_string()
}

/// The stored HTTP validators for the unread endpoint, or empty when there's no
/// cache yet (TASK-42).
fn stored_validators(store: &Option<Store>) -> Validators {
    store
        .as_ref()
        .map(|s| s.get_validators("unread"))
        .unwrap_or_default()
}

/// Whether the background auto-refresh timer should fire now: it's enabled, no
/// fetch is already in flight, and at least the configured interval has elapsed
/// since the last fetch fired (TASK-37). Pure so the decision is unit-testable
/// without real time.
fn should_auto_refresh(interval: Option<Duration>, elapsed: Duration, in_flight: bool) -> bool {
    matches!(interval, Some(i) if !in_flight && elapsed >= i)
}

fn spawn_fetch(handle: &Handle, client: Client, tx: UnboundedSender<Msg>, validators: Validators) {
    handle.spawn_blocking(move || match load(&client, &validators) {
        Ok(LoadOutcome::NotModified) => {
            let _ = tx.send(Msg::NotModified);
        }
        // Send the snapshot, then the fresh validators for persist_msg to store.
        Ok(LoadOutcome::Fresh(loaded, new_validators)) => {
            let _ = tx.send(Msg::Loaded(Ok(loaded)));
            let _ = tx.send(Msg::Validators(new_validators));
        }
        Err(e) => {
            let _ = tx.send(Msg::Loaded(Err(format!("{e:#}"))));
        }
    });
}

/// Run a mark-read / undo network write on the blocking pool and report the
/// outcome — with the entry + index for rollback — back to the UI loop.
/// Write a batch of unread-state changes in one request (the client batches at
/// its 1,000-id limit internally) and deliver the result with the batch so the
/// UI can finalize or roll back as a unit (TASK-30). A one-element batch is the
/// single `m`/`u` path.
fn spawn_write(
    handle: &Handle,
    client: &Client,
    tx: &UnboundedSender<Msg>,
    op: WriteOp,
    batch: Vec<(Entry, usize)>,
) {
    let client = client.clone();
    let tx = tx.clone();
    let ids: Vec<i64> = batch.iter().map(|(e, _)| e.id).collect();
    handle.spawn_blocking(move || {
        let net = match op {
            WriteOp::MarkRead => client.mark_read(&ids),
            WriteOp::Undo => client.mark_unread(&ids),
        };
        let result = net.map(|_| ()).map_err(|e| format!("{e:#}"));
        let _ = tx.send(Msg::Write { op, batch, result });
    });
}

/// Fetch + render an entry image on the blocking pool and deliver the result.
fn spawn_image(handle: &Handle, tx: &UnboundedSender<Msg>, url: String, max_cols: u16) {
    let tx = tx.clone();
    handle.spawn_blocking(move || {
        let result = crate::images::fetch_and_render(&url, max_cols).map_err(|e| format!("{e:#}"));
        let _ = tx.send(Msg::Image { url, result });
    });
}

/// Hydrate a batch of older unread entries on the blocking pool and deliver them
/// to be appended to the loaded set (TASK-40).
fn spawn_load_more(handle: &Handle, client: &Client, tx: &UnboundedSender<Msg>, ids: Vec<i64>) {
    let client = client.clone();
    let tx = tx.clone();
    handle.spawn_blocking(move || {
        let result = client.entries(&ids).map_err(|e| format!("{e:#}"));
        let _ = tx.send(Msg::LoadedMore(result));
    });
}

/// Write a background result through to the offline cache (TASK-41). Called on
/// the main thread as messages drain, so the `Store` never crosses threads.
/// Cache errors are swallowed — persistence is best-effort and never overrides
/// the network truth or blocks the UI.
fn persist_msg(store: &mut Store, msg: &Msg) {
    match msg {
        Msg::Loaded(Ok(loaded)) => {
            // The full unread set is the hydrated entries plus the un-hydrated
            // pending ids; cached entries no longer in it are marked read.
            let unread_ids: Vec<i64> = loaded
                .entries
                .iter()
                .map(|e| e.id)
                .chain(loaded.pending_ids.iter().copied())
                .collect();
            let _ = store.replace_snapshot(&loaded.entries, &loaded.feed_titles, &unread_ids);
        }
        Msg::LoadedMore(Ok(more)) => {
            let _ = store.upsert_entries(more);
        }
        Msg::Write {
            op,
            batch,
            result: Ok(()),
        } => {
            let unread = matches!(op, WriteOp::Undo);
            for (entry, _) in batch {
                let _ = store.set_unread(entry.id, unread);
            }
        }
        // Persist fresh HTTP validators so the next fetch can 304 (TASK-42).
        Msg::Validators(v) => {
            let _ = store.set_validators("unread", v);
        }
        _ => {}
    }
}

/// Blocking, conditional fetch of the newest unread entries plus their feed
/// names. Replays `validators`; a `304` short-circuits to `NotModified` without
/// touching subscriptions/entries (TASK-42).
fn load(client: &Client, validators: &Validators) -> Result<LoadOutcome> {
    let (mut unread, new_validators) = match client.unread_entry_ids_conditional(validators)? {
        Conditional::NotModified => return Ok(LoadOutcome::NotModified),
        Conditional::Modified { data, validators } => (data, validators),
    };
    let total_unread = unread.len();
    unread.sort_unstable_by(|a, b| b.cmp(a));
    // Hydrate the newest window now; keep the rest as pending ids to hydrate on
    // demand as the user reads toward the end (TASK-40).
    let pending_ids: Vec<i64> = unread.split_off(unread.len().min(DISPLAY_LIMIT));
    let sample = unread;
    if sample.is_empty() {
        return Ok(LoadOutcome::Fresh(
            Loaded {
                entries: Vec::new(),
                feed_titles: HashMap::new(),
                total_unread,
                pending_ids,
            },
            new_validators,
        ));
    }
    let feed_titles = client.feed_titles()?;
    let mut entries = client.entries(&sample)?;
    entries.sort_by(|a, b| b.published.cmp(&a.published));
    Ok(LoadOutcome::Fresh(
        Loaded {
            entries,
            feed_titles,
            total_unread,
            pending_ids,
        },
        new_validators,
    ))
}

/// Run the full-screen TUI until the user quits, restoring the terminal on the
/// way out (including on panic, via ratatui's panic hook).
pub fn run(client: Client) -> Result<()> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .context("building the Tokio runtime")?;
    let handle = runtime.handle().clone();
    let browser_pref = crate::config::load_browser_pref().unwrap_or_default();
    // Optional background auto-refresh; `None` (unset/zero) leaves it off (TASK-37).
    let refresh_interval = crate::config::load_refresh_interval().unwrap_or(None);
    // Optional accent override; an unset/invalid `highlight_color` falls back to
    // the rose default (TASK-45).
    let accent = crate::config::load_highlight_color()
        .unwrap_or(None)
        .as_deref()
        .and_then(theme::parse_hex)
        .unwrap_or(theme::ROSE);

    let (tx, mut rx) = mpsc::unbounded_channel::<Msg>();

    let mut terminal = ratatui::init();
    let result = run_loop(
        &mut terminal,
        &handle,
        &client,
        &tx,
        &mut rx,
        UiConfig {
            browser_pref: &browser_pref,
            refresh_interval,
            accent,
        },
    );
    ratatui::restore();
    result
}

/// Preview the "all caught up" rose (TASK-14) without logging in or hitting the
/// network — handy for eyeballing the empty state. Seeds the `Ready` + no-entries
/// state and renders it until `q`/`Esc`.
pub fn run_preview() -> Result<()> {
    let mut app = App::new();
    app.status = Status::Ready; // Ready with no entries → the caught-up screen
    let mut terminal = ratatui::init();
    let result = preview_loop(&mut terminal, &mut app);
    ratatui::restore();
    result
}

fn preview_loop(terminal: &mut DefaultTerminal, app: &mut App) -> Result<()> {
    loop {
        terminal
            .draw(|frame| app.draw(frame))
            .context("drawing the UI")?;
        if event::poll(TICK).context("polling for input")?
            && let Event::Key(key) = event::read().context("reading input")?
            && key.kind == KeyEventKind::Press
            && matches!(key.code, KeyCode::Char('q') | KeyCode::Esc)
        {
            return Ok(());
        }
    }
}

/// User-configuration inputs for the UI loop, bundled so the arg list stays lean.
struct UiConfig<'a> {
    browser_pref: &'a BrowserPref,
    /// Background auto-refresh interval, `None` when disabled (TASK-37).
    refresh_interval: Option<Duration>,
    /// Resolved accent color — rose default or the user's override (TASK-45).
    accent: Color,
}

fn run_loop(
    terminal: &mut DefaultTerminal,
    handle: &Handle,
    client: &Client,
    tx: &UnboundedSender<Msg>,
    rx: &mut mpsc::UnboundedReceiver<Msg>,
    config: UiConfig<'_>,
) -> Result<()> {
    let UiConfig {
        browser_pref,
        refresh_interval,
        accent,
    } = config;
    let mut app = App::new();
    app.base_accent = accent;
    // Open the offline cache and paint from it immediately (TASK-41); a cache
    // failure is non-fatal — roses just runs without persistence.
    let mut store = Store::open().ok();
    if let Some(store) = &store
        && let Ok(snap) = store.load_unread(DISPLAY_LIMIT)
        && !snap.entries.is_empty()
    {
        app.apply(Msg::Loaded(Ok(Loaded {
            entries: snap.entries,
            feed_titles: snap.feed_titles,
            total_unread: snap.total_unread,
            pending_ids: Vec::new(),
        })));
    }
    // Kick off the first reconcile, replaying any stored validators so an
    // unchanged unread set comes back as a cheap 304 (TASK-42).
    spawn_fetch(
        handle,
        client.clone(),
        tx.clone(),
        stored_validators(&store),
    );
    let mut images_in_flight = 0usize;
    let mut last_selected = None;
    // Auto-refresh bookkeeping (TASK-37): `last_fetch` is when the most recent
    // fetch *fired* (initial, manual, or auto) and `fetch_in_flight` guards
    // against overlapping the next auto-refresh with an outstanding one. The
    // initial fetch above is in flight, so start guarded.
    let mut last_fetch = Instant::now();
    let mut fetch_in_flight = true;
    while !app.should_quit {
        // Advance the spinner every iteration; the loop ticks ~every 100 ms
        // (the `event::poll(TICK)` below), so it animates without input.
        app.spinner_tick = app.spinner_tick.wrapping_add(1);
        while let Ok(msg) = rx.try_recv() {
            if matches!(msg, Msg::Image { .. }) {
                images_in_flight = images_in_flight.saturating_sub(1);
            }
            // A fetch completed (200/error or a 304) — clear the auto-refresh
            // guard so the next interval can fire (TASK-37).
            if matches!(msg, Msg::Loaded(_) | Msg::NotModified) {
                fetch_in_flight = false;
            }
            // Persist network results to the cache before applying them (main-
            // thread store writes; the cache seed above doesn't come via `rx`).
            if let Some(store) = store.as_mut() {
                persist_msg(store, &msg);
            }
            app.apply(msg);
        }
        terminal
            .draw(|frame| app.draw(frame))
            .context("drawing the UI")?;

        // Bump the focused article's still-queued images to the front (only if
        // not fetched yet), then drain the queue up to the concurrency cap.
        if app.selected_article != last_selected {
            last_selected = app.selected_article;
            app.prioritize_selected_images();
        }
        while images_in_flight < MAX_IMAGE_FETCHES {
            let Some(url) = app.next_queued_image() else {
                break;
            };
            images_in_flight += 1;
            spawn_image(handle, tx, url, app.reader_width.max(1));
        }

        // Lazily hydrate the next batch of unread as the user nears the end
        // (TASK-40); the guard inside prevents overlapping loads.
        if let Some(ids) = app.maybe_begin_load_more() {
            spawn_load_more(handle, client, tx, ids);
        }

        // Background auto-refresh (TASK-37): silently re-fetch on the configured
        // interval. No `Status::Loading`, so a mid-read isn't disturbed — the
        // gentle `Msg::Loaded` apply preserves selection/scroll, and an
        // unchanged unread set 304s to a no-op (TASK-42).
        if should_auto_refresh(refresh_interval, last_fetch.elapsed(), fetch_in_flight) {
            spawn_fetch(
                handle,
                client.clone(),
                tx.clone(),
                stored_validators(&store),
            );
            last_fetch = Instant::now();
            fetch_in_flight = true;
        }

        if event::poll(TICK).context("polling for input")?
            && let Event::Key(key) = event::read().context("reading input")?
            && key.kind == KeyEventKind::Press
        {
            match app.handle_key(key.code) {
                Action::None => {}
                Action::Reload => {
                    app.status = Status::Loading;
                    spawn_fetch(
                        handle,
                        client.clone(),
                        tx.clone(),
                        stored_validators(&store),
                    );
                    // Reset the auto-refresh timer so it doesn't immediately
                    // re-fetch on the heels of a manual reload (TASK-37).
                    last_fetch = Instant::now();
                    fetch_in_flight = true;
                }
                Action::MarkRead => {
                    if let Some(batch) = app.begin_mark_read() {
                        spawn_write(handle, client, tx, WriteOp::MarkRead, batch);
                    }
                }
                Action::MarkSourceRead => {
                    if let Some(batch) = app.begin_mark_source_read() {
                        spawn_write(handle, client, tx, WriteOp::MarkRead, batch);
                    }
                }
                Action::MarkWindowRead => {
                    if let Some(batch) = app.begin_mark_window_read() {
                        spawn_write(handle, client, tx, WriteOp::MarkRead, batch);
                    }
                }
                Action::Undo => {
                    if let Some(batch) = app.begin_undo() {
                        spawn_write(handle, client, tx, WriteOp::Undo, batch);
                    }
                }
                Action::OpenInBrowser => open_selected(terminal, &mut app, browser_pref),
            }
        }
    }
    Ok(())
}

/// Open the selected article's URL, suspending the TUI around a terminal browser
/// so it can take over the screen (AC #3), then restoring it.
fn open_selected(terminal: &mut DefaultTerminal, app: &mut App, pref: &BrowserPref) {
    let Some(url) = app.selected_url() else {
        app.notice = Some("No URL for this entry.".to_string());
        return;
    };
    let launch = browser::resolve(pref, std::env::var("BROWSER").ok().as_deref(), &url);
    let result = if launch.terminal {
        suspend_and_run(terminal, &launch)
    } else {
        browser::run(&launch)
    };
    if let Err(err) = result {
        app.notice = Some(format!("Browser failed: {err:#}"));
    }
}

/// Leave the alt screen + raw mode, run a terminal browser to completion, then
/// restore the TUI and force a full redraw.
fn suspend_and_run(terminal: &mut DefaultTerminal, launch: &browser::Launch) -> Result<()> {
    use ratatui::crossterm::execute;
    use ratatui::crossterm::terminal::{
        EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
    };

    let mut stdout = std::io::stdout();
    disable_raw_mode().context("leaving raw mode")?;
    execute!(stdout, LeaveAlternateScreen).context("leaving the alternate screen")?;

    let result = browser::run(launch);

    enable_raw_mode().context("re-entering raw mode")?;
    execute!(stdout, EnterAlternateScreen).context("re-entering the alternate screen")?;
    terminal.clear().context("clearing the terminal")?;
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Plain-text rendering of HTML via the content pipeline (test helper).
    fn html_to_text(html: &str) -> String {
        content_blocks(html)
            .into_iter()
            .filter_map(|segment| match segment {
                Segment::Text(text) => Some(text),
                Segment::Image(_) => None,
            })
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    fn entry(id: i64, feed_id: i64, title: &str, content: Option<&str>) -> Entry {
        Entry {
            id,
            feed_id,
            title: Some(title.to_string()),
            url: Some(format!("https://example.com/{id}")),
            author: None,
            published: Some("2026-06-29T00:00:00.000000Z".to_string()),
            summary: Some("summary".to_string()),
            content: content.map(str::to_string),
            images: None,
            enclosure: None,
            json_feed: None,
        }
    }

    /// Build a ready app from `(feed_id, name, article_count)` tuples.
    fn app_with(feeds: &[(i64, &str, usize)]) -> App {
        let mut feed_titles = HashMap::new();
        let mut entries = Vec::new();
        let mut next_id = 100;
        for &(feed_id, name, count) in feeds {
            feed_titles.insert(feed_id, name.to_string());
            for j in 0..count {
                entries.push(entry(
                    next_id,
                    feed_id,
                    &format!("{name} #{j}"),
                    Some("<p>Body</p>"),
                ));
                next_id += 1;
            }
        }
        let total_unread = entries.len();
        let mut app = App::new();
        app.apply(Msg::Loaded(Ok(Loaded {
            entries,
            feed_titles,
            total_unread,
            pending_ids: Vec::new(),
        })));
        app
    }

    #[test]
    fn sources_group_and_count_ordered_by_name() {
        let app = app_with(&[(7, "Rust Blog", 2), (9, "Hacker News", 3)]);
        // Sorted by feed name: Hacker News (9) before Rust Blog (7).
        assert_eq!(app.sources(), vec![(9, 3), (7, 2)]);
    }

    #[test]
    fn articles_show_oldest_first() {
        // load() stores entries newest-first; the articles column reverses that.
        let mut feed_titles = HashMap::new();
        feed_titles.insert(9, "Feed".to_string());
        let mk = |id: i64, published: &str| Entry {
            id,
            feed_id: 9,
            title: Some(format!("a{id}")),
            url: None,
            author: None,
            published: Some(published.to_string()),
            summary: None,
            content: None,
            images: None,
            enclosure: None,
            json_feed: None,
        };
        // Newest-first, as load() produces (published descending).
        let entries = vec![
            mk(3, "2026-03-01T00:00:00Z"),
            mk(2, "2026-02-01T00:00:00Z"),
            mk(1, "2026-01-01T00:00:00Z"),
        ];
        let mut app = App::new();
        app.apply(Msg::Loaded(Ok(Loaded {
            entries,
            feed_titles,
            total_unread: 3,
            pending_ids: Vec::new(),
        })));
        // The articles column shows oldest (id 1) at the top, newest (id 3) last.
        assert_eq!(app.article_ids(9), vec![1, 2, 3]);
        // The cursor starts on the top (oldest) article.
        assert_eq!(app.selected_article, Some(1));
    }

    #[test]
    fn load_focuses_first_source_and_its_first_article() {
        let app = app_with(&[(7, "Rust Blog", 2), (9, "Hacker News", 3)]);
        assert!(matches!(app.focus, Focus::Sources));
        assert_eq!(app.selected_source, Some(9));
        assert_eq!(app.selected_article, app.article_ids(9).first().copied());
    }

    #[test]
    fn down_in_sources_changes_source_and_resets_article() {
        let mut app = app_with(&[(7, "Rust Blog", 2), (9, "Hacker News", 3)]);
        app.move_cursor(1); // Hacker News -> Rust Blog
        assert_eq!(app.selected_source, Some(7));
        assert_eq!(app.selected_article, app.article_ids(7).first().copied());
        app.move_cursor(1); // clamps at the last source
        assert_eq!(app.selected_source, Some(7));
    }

    #[test]
    fn right_and_left_move_focus_across_columns() {
        let mut app = app_with(&[(9, "Hacker News", 3)]);
        assert!(matches!(app.focus, Focus::Sources));
        app.focus_right();
        assert!(matches!(app.focus, Focus::Articles));
        app.focus_right();
        assert!(matches!(app.focus, Focus::Reader));
        app.focus_right(); // stays at the rightmost
        assert!(matches!(app.focus, Focus::Reader));
        app.focus_left();
        assert!(matches!(app.focus, Focus::Articles));
        app.focus_left();
        assert!(matches!(app.focus, Focus::Sources));
        app.focus_left(); // stays at the leftmost
        assert!(matches!(app.focus, Focus::Sources));
    }

    #[test]
    fn focus_changes_preserve_each_columns_cursor() {
        let mut app = app_with(&[(9, "Hacker News", 3)]);
        app.focus_right(); // Articles
        app.move_cursor(1); // second article
        let ids = app.article_ids(9);
        assert_eq!(app.selected_article, Some(ids[1]));
        app.focus_left(); // back to Sources (cursor remembered)
        assert!(matches!(app.focus, Focus::Sources));
        app.focus_right(); // back to Articles
        assert_eq!(
            app.selected_article,
            Some(ids[1]),
            "article cursor preserved"
        );
    }

    #[test]
    fn down_in_articles_moves_within_the_source() {
        let mut app = app_with(&[(9, "Hacker News", 3)]);
        app.focus_right();
        let ids = app.article_ids(9);
        assert_eq!(app.selected_article, Some(ids[0]));
        app.move_cursor(1);
        assert_eq!(app.selected_article, Some(ids[1]));
    }

    #[test]
    fn arrows_scroll_the_reader_when_focused() {
        let mut app = app_with(&[(9, "Hacker News", 1)]);
        app.focus_right();
        app.focus_right(); // Reader
        assert_eq!(app.reader_scroll, 0);
        app.move_cursor(1);
        assert_eq!(app.reader_scroll, READER_SCROLL_STEP);
        app.move_cursor(-1);
        assert_eq!(app.reader_scroll, 0);
    }

    #[test]
    fn mark_read_in_sources_focus_is_a_noop() {
        let mut app = app_with(&[(9, "Hacker News", 2)]);
        assert!(
            app.begin_mark_read().is_none(),
            "no article target in sources focus"
        );
    }

    #[test]
    fn mark_read_success_then_undo_round_trips() {
        let mut app = app_with(&[(9, "Hacker News", 2)]);
        app.focus_right(); // Articles
        let batch = app.begin_mark_read().expect("an article is selected");
        assert_eq!(batch.len(), 1, "single mark is a one-element batch");
        assert_eq!(app.article_ids(9).len(), 1);
        assert_eq!(app.total_unread, 1);
        app.apply(Msg::Write {
            op: WriteOp::MarkRead,
            batch,
            result: Ok(()),
        });
        assert_eq!(app.undo_stack.len(), 1);

        let batch = app.begin_undo().expect("something to undo");
        assert_eq!(app.article_ids(9).len(), 2);
        assert_eq!(app.total_unread, 2);
        app.apply(Msg::Write {
            op: WriteOp::Undo,
            batch,
            result: Ok(()),
        });
        assert_eq!(app.article_ids(9).len(), 2);
    }

    #[test]
    fn mark_read_failure_rolls_back() {
        let mut app = app_with(&[(9, "Hacker News", 2)]);
        app.focus_right();
        let batch = app.begin_mark_read().unwrap();
        assert_eq!(app.article_ids(9).len(), 1);
        app.apply(Msg::Write {
            op: WriteOp::MarkRead,
            batch,
            result: Err("boom".to_string()),
        });
        assert_eq!(app.article_ids(9).len(), 2, "rolled back");
        assert_eq!(app.total_unread, 2);
        assert!(app.notice.is_some());
    }

    #[test]
    fn emptying_a_source_drops_focus_back_to_sources() {
        let mut app = app_with(&[(7, "Rust Blog", 1), (9, "Hacker News", 1)]);
        // Sources by name: Hacker News (9), Rust Blog (7). Focus the only HN article.
        app.focus_right();
        let _ = app.begin_mark_read().unwrap();
        assert_eq!(app.sources(), vec![(7, 1)], "Hacker News is gone");
        assert!(matches!(app.focus, Focus::Sources));
        assert_eq!(app.selected_source, Some(7));
    }

    // --- Bulk mark-read (TASK-30) -----------------------------------------

    #[test]
    fn mark_source_read_batches_every_loaded_article_in_the_source() {
        let mut app = app_with(&[(9, "Hacker News", 3), (7, "Rust Blog", 2)]);
        // reset_selection focuses the first source by name — Hacker News (9).
        assert_eq!(app.selected_source, Some(9));
        let want: Vec<i64> = app.article_ids(9); // the batch should carry exactly these
        let batch = app.begin_mark_source_read().expect("a source is selected");
        let got: Vec<i64> = batch.iter().map(|(e, _)| e.id).collect();
        let mut got_sorted = got.clone();
        let mut want_sorted = want.clone();
        got_sorted.sort_unstable();
        want_sorted.sort_unstable();
        assert_eq!(
            got_sorted, want_sorted,
            "one batched write covers the source"
        );
        assert!(app.articles(9).is_empty(), "source cleared optimistically");
        assert_eq!(app.sources(), vec![(7, 2)], "only Rust Blog remains");
        assert_eq!(app.total_unread, 2, "3 of 5 removed");
        assert!(matches!(app.focus, Focus::Sources));
    }

    #[test]
    fn mark_source_read_works_from_sources_focus() {
        // Unlike single `m`, `M` operates on the selected source regardless of
        // focus (you're pointing at it in the left column).
        let mut app = app_with(&[(9, "Hacker News", 2)]);
        assert!(matches!(app.focus, Focus::Sources));
        let batch = app.begin_mark_source_read().expect("source selected");
        assert_eq!(batch.len(), 2);
    }

    #[test]
    fn mark_source_read_undo_restores_the_whole_source_in_order() {
        let mut app = app_with(&[(9, "Hacker News", 3)]);
        let before = app.article_ids(9);
        let batch = app.begin_mark_source_read().unwrap();
        app.apply(Msg::Write {
            op: WriteOp::MarkRead,
            batch,
            result: Ok(()),
        });
        assert_eq!(app.undo_stack.len(), 1, "one undo entry for the batch");
        assert!(app.articles(9).is_empty());

        let batch = app.begin_undo().expect("the batch is undoable");
        assert_eq!(batch.len(), 3, "undo restores the whole batch at once");
        assert_eq!(app.article_ids(9), before, "order preserved after undo");
        assert_eq!(app.total_unread, 3);
    }

    #[test]
    fn mark_source_read_failure_rolls_back_the_batch() {
        let mut app = app_with(&[(9, "Hacker News", 3), (7, "Rust Blog", 1)]);
        let before = app.article_ids(9);
        let batch = app.begin_mark_source_read().unwrap();
        assert!(app.articles(9).is_empty());
        app.apply(Msg::Write {
            op: WriteOp::MarkRead,
            batch,
            result: Err("boom".to_string()),
        });
        assert_eq!(
            app.article_ids(9),
            before,
            "whole batch rolled back in order"
        );
        assert_eq!(app.total_unread, 4);
        assert!(app.notice.is_some());
    }

    #[test]
    fn mark_window_read_clears_loaded_only_and_keeps_pending() {
        let mut app = app_with(&[(9, "Hacker News", 2)]);
        // Simulate more unread than are hydrated: 2 loaded + 2 pending.
        app.pending_ids = vec![200, 201];
        app.total_unread = 4;
        let batch = app
            .begin_mark_window_read()
            .expect("the window has entries");
        assert_eq!(batch.len(), 2, "only the loaded window is marked");
        assert!(app.entries.is_empty(), "window cleared");
        assert_eq!(app.total_unread, 2, "the pending ids stay unread");
        assert_eq!(app.pending_ids, vec![200, 201], "pending ids untouched");
        assert_eq!(app.selected_source, None, "nothing selected once empty");

        // Undo restores the whole loaded window as a unit.
        app.apply(Msg::Write {
            op: WriteOp::MarkRead,
            batch,
            result: Ok(()),
        });
        let batch = app.begin_undo().expect("the window is undoable");
        assert_eq!(batch.len(), 2);
        assert_eq!(app.entries.len(), 2, "window restored");
        assert_eq!(app.total_unread, 4);
    }

    #[test]
    fn mark_window_read_is_gated_by_a_confirmation() {
        let mut app = app_with(&[(9, "Hacker News", 2)]);
        // `A` arms a confirmation but takes no action yet.
        assert!(matches!(app.handle_key(KeyCode::Char('A')), Action::None));
        assert!(app.pending_confirm.is_some(), "confirmation armed");
        assert!(
            app.confirm_prompt().is_some_and(|p| p.contains('2')),
            "prompt names the loaded count"
        );
        // `y` confirms and consumes the pending state, yielding the bulk action.
        assert!(matches!(
            app.handle_key(KeyCode::Char('y')),
            Action::MarkWindowRead
        ));
        assert!(app.pending_confirm.is_none(), "confirmation consumed");
    }

    #[test]
    fn mark_window_confirmation_cancels_on_anything_but_yes() {
        let mut app = app_with(&[(9, "Hacker News", 2)]);
        let _ = app.handle_key(KeyCode::Char('A'));
        assert!(app.pending_confirm.is_some());
        // `n` (or any non-`y` key) cancels without marking.
        assert!(matches!(app.handle_key(KeyCode::Char('n')), Action::None));
        assert!(app.pending_confirm.is_none(), "cancelled");
        assert_eq!(app.entries.len(), 2, "nothing was marked");
    }

    #[test]
    fn mark_window_read_needs_something_loaded_to_confirm() {
        let mut app = App::new();
        app.apply(Msg::Loaded(Ok(Loaded {
            entries: vec![],
            feed_titles: HashMap::new(),
            total_unread: 0,
            pending_ids: Vec::new(),
        })));
        // `A` with an empty window arms nothing (no prompt, no action).
        assert!(matches!(app.handle_key(KeyCode::Char('A')), Action::None));
        assert!(app.pending_confirm.is_none());
    }

    // --- Help overlay (TASK-32) -------------------------------------------

    #[test]
    fn help_overlay_toggles_with_question_mark_and_any_key_closes() {
        let mut app = app_with(&[(9, "Hacker News", 2)]);
        let (src, art, focus) = (app.selected_source, app.selected_article, app.focus);
        assert!(!app.show_help);
        // `?` opens it, taking no other action.
        assert!(matches!(app.handle_key(KeyCode::Char('?')), Action::None));
        assert!(app.show_help, "help opened");
        // Any key closes it and does nothing else (here `j` must not also move).
        assert!(matches!(app.handle_key(KeyCode::Char('j')), Action::None));
        assert!(!app.show_help, "help closed by any key");
        // Closing help via a nav key doesn't also navigate.
        assert_eq!(app.selected_source, src);
        assert_eq!(app.selected_article, art);
        assert!(app.focus == focus, "focus unchanged");
    }

    #[test]
    fn help_overlay_esc_and_q_close_without_quitting() {
        let mut app = app_with(&[(9, "Hacker News", 1)]);
        let _ = app.handle_key(KeyCode::Char('?'));
        let _ = app.handle_key(KeyCode::Esc);
        assert!(!app.show_help, "Esc closes the overlay");
        assert!(!app.should_quit, "Esc closes help rather than quitting");

        let _ = app.handle_key(KeyCode::Char('?'));
        let _ = app.handle_key(KeyCode::Char('q'));
        assert!(!app.show_help, "q closes the overlay");
        assert!(!app.should_quit, "q closes help rather than quitting");
    }

    #[test]
    fn help_overlay_does_not_disturb_selection_or_loading() {
        // AC #2: opening the overlay is pure chrome — a background load still
        // applies and the selection is preserved.
        let mut app = app_with(&[(9, "Hacker News", 2)]);
        app.focus_right();
        let sel = app.selected_article;
        let _ = app.handle_key(KeyCode::Char('?'));
        app.apply(Msg::Loaded(Ok(Loaded {
            entries: app.entries.clone(),
            feed_titles: app.feed_titles.clone(),
            total_unread: 2,
            pending_ids: Vec::new(),
        })));
        assert!(app.show_help, "still open across a background load");
        assert_eq!(app.selected_article, sel, "selection preserved");
    }

    #[test]
    fn footer_moves_bulk_marks_into_the_overlay() {
        // The footer no longer hints M/A (they moved to the overlay) but gains
        // the `?` help hint — both derived from the single BINDINGS table.
        let footer: String = footer_help(theme::ROSE)
            .spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect();
        assert!(footer.contains("help"), "footer advertises the help key");
        assert!(
            !footer.contains(" src "),
            "bulk source hint left the footer"
        );
        assert!(
            !footer.contains(" all "),
            "bulk window hint left the footer"
        );

        let overlay: String = help_lines(theme::ROSE)
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.as_ref())
            .collect();
        assert!(overlay.contains("Mark the source read"), "M documented");
        assert!(
            overlay.contains("Mark the window read (asks y / n)"),
            "A documented"
        );
    }

    #[test]
    fn help_overlay_renders_all_bindings() {
        // AC #3: the rendered overlay shows the expected keys + group headings.
        let mut app = app_with(&[(9, "Hacker News", 2)]);
        app.show_help = true;
        let backend = ratatui::backend::TestBackend::new(80, 30);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|frame| app.draw(frame)).unwrap();
        let rendered: String = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(ratatui::buffer::Cell::symbol)
            .collect();
        assert!(rendered.contains("Keyboard shortcuts"), "the titled box");
        for group in ["Navigation", "Reading", "Marking read", "App"] {
            assert!(rendered.contains(group), "group heading: {group}");
        }
        for phrase in [
            "Mark the article read",
            "Mark the source read",
            "Mark the window read",
            "Toggle this help",
            "Quit",
        ] {
            assert!(rendered.contains(phrase), "binding: {phrase}");
        }
    }

    // --- Auto-refresh (TASK-37) -------------------------------------------

    #[test]
    fn auto_refresh_predicate_gates_on_interval_and_in_flight() {
        // Disabled → never fires, however long it's been.
        assert!(!should_auto_refresh(None, Duration::from_secs(3600), false));
        // Enabled but not yet due.
        let interval = Some(Duration::from_secs(60));
        assert!(!should_auto_refresh(
            interval,
            Duration::from_secs(30),
            false
        ));
        // Enabled and due, with no fetch outstanding → fire.
        assert!(should_auto_refresh(
            interval,
            Duration::from_secs(60),
            false
        ));
        assert!(should_auto_refresh(
            interval,
            Duration::from_secs(120),
            false
        ));
        // Due, but a fetch is already in flight → hold off.
        assert!(!should_auto_refresh(
            interval,
            Duration::from_secs(120),
            true
        ));
    }

    #[test]
    fn reload_preserves_selection_and_scroll_when_the_article_survives() {
        let mut app = app_with(&[(7, "Rust Blog", 3), (9, "Hacker News", 3)]);
        let ids = app.article_ids(7);
        app.selected_source = Some(7);
        app.selected_article = Some(ids[1]);
        app.focus = Focus::Reader;
        app.reader_scroll = 5;

        // A reload that returns the identical unread set (e.g. auto-refresh with
        // changes elsewhere) must not disturb the reader's place.
        app.apply(Msg::Loaded(Ok(Loaded {
            entries: app.entries.clone(),
            feed_titles: app.feed_titles.clone(),
            total_unread: app.total_unread,
            pending_ids: Vec::new(),
        })));

        assert_eq!(app.selected_source, Some(7));
        assert_eq!(app.selected_article, Some(ids[1]), "selection kept by id");
        assert_eq!(app.reader_scroll, 5, "scroll kept mid-read");
        assert!(matches!(app.focus, Focus::Reader), "focus kept");
    }

    #[test]
    fn reload_reselects_when_the_selected_article_vanished() {
        let mut app = app_with(&[(7, "Rust Blog", 3)]);
        let ids = app.article_ids(7);
        app.selected_source = Some(7);
        app.selected_article = Some(ids[1]);
        app.focus = Focus::Reader;
        app.reader_scroll = 4;

        // The selected article was read on another client, so it's absent now.
        let entries: Vec<Entry> = app
            .entries
            .iter()
            .filter(|e| e.id != ids[1])
            .cloned()
            .collect();
        app.apply(Msg::Loaded(Ok(Loaded {
            entries,
            feed_titles: app.feed_titles.clone(),
            total_unread: 2,
            pending_ids: Vec::new(),
        })));

        assert_eq!(app.selected_source, Some(7), "source kept");
        assert_ne!(app.selected_article, Some(ids[1]));
        assert_eq!(
            app.selected_article,
            app.article_ids(7).first().copied(),
            "falls back to the source's first article"
        );
        assert_eq!(
            app.reader_scroll, 0,
            "reader reset when the article vanished"
        );
    }

    #[test]
    fn reload_preserves_undo_stack_but_prunes_readded_entries() {
        let mut app = app_with(&[(9, "Hacker News", 2)]);
        app.focus_right();
        let batch = app.begin_mark_read().unwrap();
        let entry = batch[0].0.clone();
        app.apply(Msg::Write {
            op: WriteOp::MarkRead,
            batch,
            result: Ok(()),
        });
        assert_eq!(app.undo_stack.len(), 1);

        // A refresh that doesn't re-add the marked-read entry keeps the undo —
        // a silent timer must not wipe it (unlike the old reset-on-load).
        app.apply(Msg::Loaded(Ok(Loaded {
            entries: app.entries.clone(),
            feed_titles: app.feed_titles.clone(),
            total_unread: 1,
            pending_ids: Vec::new(),
        })));
        assert_eq!(app.undo_stack.len(), 1, "undo survived the refresh");

        // But if the server re-added it (marked unread elsewhere), the stale
        // undo is pruned so a later `u` can't duplicate a now-present row.
        let mut entries = app.entries.clone();
        entries.push(entry);
        app.apply(Msg::Loaded(Ok(Loaded {
            entries,
            feed_titles: app.feed_titles.clone(),
            total_unread: 2,
            pending_ids: Vec::new(),
        })));
        assert!(
            app.undo_stack.is_empty(),
            "undo pruned once the entry returned"
        );
    }

    #[test]
    fn renders_three_columns_with_reader_when_article_focused() {
        let mut app = app_with(&[(9, "Hacker News", 2)]);
        app.focus_right(); // focus Articles so the reader shows the article
        let backend = ratatui::backend::TestBackend::new(120, 20);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|frame| app.draw(frame)).unwrap();
        let rendered: String = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(ratatui::buffer::Cell::symbol)
            .collect();

        assert!(rendered.contains("Sources"), "sources column titled");
        assert!(rendered.contains("Articles"), "articles column titled");
        assert!(rendered.contains("Reader"), "reader column titled");
        assert!(rendered.contains("Hacker News"), "source name shown");
        assert!(rendered.contains("Body"), "reader shows the article body");
        assert!(rendered.contains("quit"), "footer help shown");
    }

    #[test]
    fn html_to_text_strips_tags_and_breaks_paragraphs() {
        assert_eq!(html_to_text("a<br>b"), "a\nb");
        assert_eq!(
            html_to_text("<p>One <b>bold</b></p><p>Two</p>"),
            "One bold\n\nTwo"
        );
    }

    #[test]
    fn html_to_text_decodes_entities() {
        assert_eq!(
            html_to_text("Tom &amp; Jerry &lt;3 &#39;hi&#39; &#x2764; &nbsp;end"),
            "Tom & Jerry <3 'hi' ❤  end"
        );
    }

    #[test]
    fn html_to_text_strips_control_chars_blocking_escape_injection() {
        let out = html_to_text("safe\u{1b}[31mtext");
        assert!(
            !out.contains('\u{1b}'),
            "escape byte must be stripped: {out:?}"
        );
        assert!(out.contains("safe"));
    }

    #[test]
    fn content_blocks_separate_images_from_text() {
        let blocks =
            content_blocks("<p>before</p><img src=\"https://x/i.png\" alt=\"a\"><p>after</p>");
        assert_eq!(blocks.len(), 3);
        assert!(matches!(&blocks[0], Segment::Text(t) if t == "before"));
        assert!(matches!(&blocks[1], Segment::Image(u) if u == "https://x/i.png"));
        assert!(matches!(&blocks[2], Segment::Text(t) if t == "after"));
    }

    #[test]
    fn extract_img_src_handles_quotes_and_ignores_srcset() {
        assert_eq!(
            extract_img_src("img src=\"https://x/a.png\""),
            Some("https://x/a.png".to_string())
        );
        assert_eq!(
            extract_img_src("img src='https://x/b.png'"),
            Some("https://x/b.png".to_string())
        );
        // `srcset` must not be mistaken for `src`.
        assert_eq!(
            extract_img_src("img srcset=\"https://x/s.png 2x\" src=\"https://x/real.png\""),
            Some("https://x/real.png".to_string())
        );
        assert_eq!(extract_img_src("img alt=\"no source\""), None);
    }

    #[test]
    fn reader_renders_image_placeholder_then_graceful_failure() {
        let entry = Entry {
            id: 1,
            feed_id: 9,
            title: Some("T".to_string()),
            url: None,
            author: None,
            published: None,
            summary: None,
            content: Some("<img src=\"https://x/i.png\">".to_string()),
            images: None,
            enclosure: None,
            json_feed: None,
        };
        let collect = |images: &HashMap<String, ImageState>| -> String {
            reader_text(&entry, images, 80, theme::ROSE)
                .lines
                .iter()
                .flat_map(|line| line.spans.iter().map(|span| span.content.to_string()))
                .collect()
        };
        let mut images = HashMap::new();
        assert!(
            collect(&images).contains("image loading"),
            "placeholder while loading"
        );
        images.insert("https://x/i.png".to_string(), ImageState::Failed);
        assert!(
            collect(&images).contains("image unavailable"),
            "graceful failure (AC #2)"
        );
    }

    /// Flatten a rendered reader `Text` into one newline-joined string.
    fn render_reader(entry: &Entry) -> String {
        reader_text(entry, &HashMap::new(), 80, theme::ROSE)
            .lines
            .iter()
            .flat_map(|line| line.spans.iter().map(|span| span.content.to_string()))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn header_entry(author: Option<&str>, published: Option<&str>) -> Entry {
        Entry {
            id: 1,
            feed_id: 9,
            title: Some("Headline".to_string()),
            url: None,
            author: author.map(str::to_string),
            published: published.map(str::to_string),
            summary: None,
            content: None,
            images: None,
            enclosure: None,
            json_feed: None,
        }
    }

    #[test]
    fn published_date_formats_in_the_target_timezone() {
        use chrono::{FixedOffset, Utc};
        // In UTC this instant is Saturday, December 2, 2023 at 2:30 AM.
        assert_eq!(
            format_published_in("2023-12-02T02:30:21.000000Z", &Utc).as_deref(),
            Some("Saturday, December 2, 2023 at 2:30 AM"),
        );
        // West of UTC (-08:00) rolls back to the previous evening — exercises the
        // timezone conversion, the PM half, and a day-boundary crossing.
        let pst = FixedOffset::west_opt(8 * 3600).unwrap();
        assert_eq!(
            format_published_in("2023-12-02T02:30:21.000000Z", &pst).as_deref(),
            Some("Friday, December 1, 2023 at 6:30 PM"),
        );
        // Missing / unparseable input degrades to None so the caller drops the
        // line rather than showing a raw or garbage value (TASK-17 AC #3).
        assert_eq!(format_published_in("", &Utc), None);
        assert_eq!(format_published_in("not-a-date", &Utc), None);
    }

    #[test]
    fn reader_header_shows_author_and_humanized_date_not_the_feed_name() {
        // TASK-18: the feed/blog name is the highlighted source on the left, so
        // it must not repeat in the reader header — the author shows instead.
        // TASK-17: the published date is humanized, not the raw ISO-8601 string.
        let rendered = render_reader(&header_entry(
            Some("Ada Lovelace"),
            Some("2023-12-02T02:30:21.000000Z"),
        ));
        assert!(
            rendered.contains("Ada Lovelace · "),
            "author then a separator: {rendered:?}"
        );
        // Month + year are stable in any host timezone (±14h can't move
        // 2023-12-02 02:30 UTC out of December 2023), so this stays deterministic.
        assert!(rendered.contains("December"), "month name: {rendered:?}");
        assert!(rendered.contains("2023"), "year: {rendered:?}");
        assert!(rendered.contains(" at "), "human time: {rendered:?}");
        assert!(
            !rendered.contains("2023-12-02T02:30:21"),
            "raw ISO timestamp must be reformatted: {rendered:?}"
        );
    }

    #[test]
    fn reader_header_without_author_shows_only_the_date() {
        // No author: the date stands alone, with no leading separator and no
        // feed name in its place.
        let rendered = render_reader(&header_entry(None, Some("2023-12-02T02:30:21.000000Z")));
        assert!(rendered.contains("December"), "date shown: {rendered:?}");
        assert!(
            !rendered.contains(" · "),
            "no separator when author is absent: {rendered:?}"
        );
    }

    #[test]
    fn reader_header_drops_the_meta_line_when_nothing_to_show() {
        // Unparseable date + no author → the meta line is omitted entirely, and
        // the raw value never leaks through.
        let rendered = render_reader(&header_entry(None, Some("not-a-date")));
        assert!(
            !rendered.contains("not-a-date"),
            "raw value hidden: {rendered:?}"
        );
        assert!(!rendered.contains(" · "), "no meta separator: {rendered:?}");
        // Wholly absent published + author is likewise clean.
        let none = render_reader(&header_entry(None, None));
        assert!(!none.contains(" · "), "no meta separator: {none:?}");
    }

    #[test]
    fn reader_header_strips_control_characters_from_title_author_and_url() {
        // TASK-27: a hostile feed embeds ESC/BEL in the header fields. They must
        // be stripped so no escape sequence reaches the terminal, while the
        // visible text survives.
        let mut e = header_entry(Some("Auth\x07or"), None);
        e.title = Some("Ti\x1btle".to_string());
        e.url = Some("https://example.com/\x1bpath".to_string());
        let rendered = render_reader(&e);
        assert!(
            !rendered.contains('\u{1b}') && !rendered.contains('\u{7}'),
            "no control chars may survive in the header: {rendered:?}"
        );
        assert!(
            rendered.contains("Title"),
            "visible title kept: {rendered:?}"
        );
        assert!(
            rendered.contains("Author"),
            "visible author kept: {rendered:?}"
        );
        assert!(
            rendered.contains("https://example.com/path"),
            "visible url kept: {rendered:?}"
        );
    }

    #[test]
    fn reader_cache_rebuilds_only_on_key_change() {
        // TASK-28: the reader render is memoized by (entry, width, image gen);
        // idle frames and scrolling reuse it instead of re-parsing the HTML.
        let mut app = App::new();
        app.status = Status::Ready;
        app.entries = vec![entry(1, 7, "Title", Some("<p>Body text here.</p>"))];
        app.selected_source = Some(7);
        app.selected_article = Some(1);

        assert!(app.ensure_reader_cache(1, 40), "first build is a miss");
        assert!(
            !app.ensure_reader_cache(1, 40),
            "an identical key reuses the cache (no re-parse on idle/scroll)"
        );
        app.image_generation += 1;
        assert!(
            app.ensure_reader_cache(1, 40),
            "an image resolving invalidates the cache"
        );
        assert!(!app.ensure_reader_cache(1, 40), "then stable again");
        assert!(
            app.ensure_reader_cache(1, 60),
            "a width change invalidates the cache"
        );
        assert!(
            !app.ensure_reader_cache(2, 60),
            "an unknown entry id doesn't rebuild (guarded before draw)"
        );
    }

    #[test]
    fn load_more_hydrates_next_batch_and_preserves_selection() {
        // TASK-40: nearing the oldest loaded entry drains the next batch of
        // pending ids; the hydrated batch is appended without disturbing the
        // (id-tracked) selection or the unread total.
        let mut app = App::new();
        app.status = Status::Ready;
        app.entries = vec![
            entry(5, 7, "E5", None),
            entry(4, 7, "E4", None),
            entry(3, 7, "E3", None),
        ];
        app.feed_titles.insert(7, "Feed".to_string());
        app.total_unread = 5;
        app.pending_ids = vec![2, 1];
        app.selected_source = Some(7);
        app.selected_article = Some(3);

        let batch = app
            .maybe_begin_load_more()
            .expect("nearing the end starts a load");
        assert_eq!(batch, vec![2, 1], "drains the next pending ids in order");
        assert!(
            app.pending_ids.is_empty(),
            "pending drained while in flight"
        );
        assert!(app.in_flight_more.is_some(), "guard set");

        app.apply(Msg::LoadedMore(Ok(vec![
            entry(2, 7, "E2", None),
            entry(1, 7, "E1", None),
        ])));
        assert_eq!(app.entries.len(), 5, "batch appended");
        assert!(app.in_flight_more.is_none(), "guard cleared");
        assert_eq!(app.selected_article, Some(3), "selection preserved by id");
        assert_eq!(
            app.total_unread, 5,
            "load-more leaves the unread total alone"
        );
    }

    #[test]
    fn load_more_guards_and_stops_when_exhausted() {
        let mut app = App::new();
        app.status = Status::Ready;
        app.entries = vec![entry(1, 7, "E1", None)];
        app.total_unread = 1;
        app.selected_source = Some(7);
        app.selected_article = Some(1);

        // Nothing pending → nothing to load (no wasted request).
        app.pending_ids = vec![];
        assert!(app.maybe_begin_load_more().is_none(), "exhausted: no load");

        // A load already in flight is not duplicated.
        app.pending_ids = vec![9, 8];
        app.in_flight_more = Some(vec![10]);
        assert!(
            app.maybe_begin_load_more().is_none(),
            "in-flight guard holds"
        );
    }

    #[test]
    fn load_more_error_restores_the_batch_for_retry() {
        let mut app = App::new();
        app.status = Status::Ready;
        app.entries = vec![entry(3, 7, "E3", None)];
        app.total_unread = 3;
        app.pending_ids = vec![2, 1];
        app.selected_source = Some(7);
        app.selected_article = Some(3);

        let batch = app.maybe_begin_load_more().expect("starts a load");
        assert_eq!(batch, vec![2, 1]);

        app.apply(Msg::LoadedMore(Err("boom".to_string())));
        assert!(app.in_flight_more.is_none(), "guard cleared on error");
        assert_eq!(
            app.pending_ids,
            vec![2, 1],
            "the batch is restored to the front for retry"
        );
        assert!(app.notice.is_some(), "a failure notice is shown");
    }

    #[test]
    fn failed_refresh_keeps_cached_view() {
        // TASK-41 offline-first: a fetch error with cached entries present keeps
        // them visible with a notice, rather than blanking to a Failed screen.
        let mut app = App::new();
        app.status = Status::Ready;
        app.entries = vec![entry(1, 7, "E1", None)];
        app.selected_source = Some(7);
        app.selected_article = Some(1);

        app.apply(Msg::Loaded(Err("offline".to_string())));
        assert!(
            matches!(app.status, Status::Ready),
            "cached view is preserved"
        );
        assert!(app.notice.is_some(), "an offline notice is shown");
        assert_eq!(app.entries.len(), 1, "cached entries kept");

        // With nothing cached, a failure still surfaces as Failed.
        let mut empty = App::new();
        empty.apply(Msg::Loaded(Err("boom".to_string())));
        assert!(matches!(empty.status, Status::Failed(_)));
    }

    #[test]
    fn not_modified_keeps_the_current_view() {
        // TASK-42: a 304 keeps the current entries/selection and settles a
        // pending Loading state to Ready — no re-render churn, no reset.
        let mut app = App::new();
        app.status = Status::Ready;
        app.entries = vec![entry(1, 7, "E1", None)];
        app.selected_source = Some(7);
        app.selected_article = Some(1);
        app.apply(Msg::NotModified);
        assert!(matches!(app.status, Status::Ready));
        assert_eq!(app.entries.len(), 1, "304 keeps cached entries");
        assert_eq!(app.selected_article, Some(1), "304 doesn't reset selection");

        // A 304 arriving while still Loading settles the status to Ready.
        let mut loading = App::new(); // status starts as Loading
        loading.apply(Msg::NotModified);
        assert!(matches!(loading.status, Status::Ready));
    }

    #[test]
    fn oversize_image_art_is_clipped_so_it_cannot_wrap() {
        // Regression: image art is rendered at the reader width when it was
        // fetched, then cached. If the terminal later narrows, the stale-wide art
        // would overflow the reader's wrap width and wrap into a full row + a
        // short fragment — the "half-height rows" artifact. reader_text clips art
        // to max_width so no art line can exceed it (and thus can't wrap).
        let url = "https://x/i.png";
        let entry = Entry {
            id: 1,
            feed_id: 9,
            title: Some("T".to_string()),
            url: None,
            author: None,
            published: None,
            summary: None,
            content: Some(format!("<img src=\"{url}\">")),
            images: None,
            enclosure: None,
            json_feed: None,
        };
        // One art line 10 cells wide, as if rendered when the terminal was wider.
        let wide = Line::from(Span::styled(
            "▀".repeat(10),
            Style::default().fg(theme::ROSE).bg(theme::LEAF),
        ));
        let mut images = HashMap::new();
        images.insert(url.to_string(), ImageState::Ready(vec![wide]));

        // Render into a reader only 6 columns wide.
        let text = reader_text(&entry, &images, 6, theme::ROSE);
        for line in &text.lines {
            let w: usize = line.spans.iter().map(|s| s.content.width()).sum();
            assert!(
                w <= 6,
                "no line may exceed the wrap width (would fragment): {w}"
            );
        }
        // The art row is still present, clipped to exactly 6 blocks (not dropped).
        let blocks: usize = text
            .lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.matches('▀').count())
            .sum();
        assert_eq!(blocks, 6, "art clipped to the wrap width");
    }

    fn img_entry(id: i64, feed_id: i64, img_url: &str) -> Entry {
        Entry {
            id,
            feed_id,
            title: Some(format!("t{id}")),
            url: None,
            author: None,
            published: None,
            summary: None,
            content: Some(format!("<p>body</p><img src=\"{img_url}\">")),
            images: None,
            enclosure: None,
            json_feed: None,
        }
    }

    #[test]
    fn images_are_prefetched_for_all_articles_on_load() {
        let mut feed_titles = HashMap::new();
        feed_titles.insert(7, "Feed".to_string());
        let no_image = Entry {
            id: 3,
            feed_id: 7,
            title: Some("t3".to_string()),
            url: None,
            author: None,
            published: None,
            summary: None,
            content: Some("<p>no image</p>".to_string()),
            images: None,
            enclosure: None,
            json_feed: None,
        };
        let entries = vec![
            img_entry(1, 7, "https://x/1.png"),
            img_entry(2, 7, "https://x/2.png"),
            no_image,
        ];
        let mut app = App::new();
        app.apply(Msg::Loaded(Ok(Loaded {
            entries,
            feed_titles,
            total_unread: 3,
            pending_ids: Vec::new(),
        })));
        // Every article's image is queued proactively, not just the selected one.
        assert_eq!(app.image_queue.len(), 2);
        assert!(matches!(
            app.images.get("https://x/1.png"),
            Some(ImageState::Loading)
        ));
        assert!(matches!(
            app.images.get("https://x/2.png"),
            Some(ImageState::Loading)
        ));
    }

    #[test]
    fn next_queued_image_drains_the_queue() {
        let mut feed_titles = HashMap::new();
        feed_titles.insert(7, "Feed".to_string());
        let mut app = App::new();
        app.apply(Msg::Loaded(Ok(Loaded {
            entries: vec![img_entry(1, 7, "https://x/1.png")],
            feed_titles,
            total_unread: 1,
            pending_ids: Vec::new(),
        })));
        assert_eq!(app.next_queued_image().as_deref(), Some("https://x/1.png"));
        assert!(app.next_queued_image().is_none());
    }

    #[test]
    fn loading_indicator_frames_and_text() {
        // Frozen ticks give deterministic frames (no timing flakiness).
        assert_eq!(loading_indicator(4, 19, 0), "⠋ Loading 4 of 19 images");
        assert_eq!(loading_indicator(4, 19, 1), "⠙ Loading 4 of 19 images");
        // The frame cycles with period 10.
        assert_eq!(loading_indicator(0, 1, 10), "⠋ Loading 0 of 1 images");
    }

    #[test]
    fn image_progress_counts_done_of_total_and_hides_when_idle() {
        let mut feed_titles = HashMap::new();
        feed_titles.insert(7, "Feed".to_string());
        let mut app = App::new();
        // Nothing loaded yet -> nothing to show.
        assert_eq!(app.image_progress(), None);
        app.apply(Msg::Loaded(Ok(Loaded {
            entries: vec![
                img_entry(1, 7, "https://x/1.png"),
                img_entry(2, 7, "https://x/2.png"),
            ],
            feed_titles,
            total_unread: 2,
            pending_ids: Vec::new(),
        })));
        // Both queued (Loading): 0 of 2.
        assert_eq!(app.image_progress(), Some((0, 2)));
        // One resolves (Ready): 1 of 2.
        app.images.insert(
            "https://x/1.png".to_string(),
            ImageState::Ready(vec![Line::from("art")]),
        );
        assert_eq!(app.image_progress(), Some((1, 2)));
        // The other fails -> all resolved -> hidden.
        app.images
            .insert("https://x/2.png".to_string(), ImageState::Failed);
        assert_eq!(app.image_progress(), None);
    }

    #[test]
    fn footer_shows_loading_indicator_right_aligned_without_hiding_help() {
        let mut feed_titles = HashMap::new();
        feed_titles.insert(7, "Feed".to_string());
        let mut app = App::new();
        app.apply(Msg::Loaded(Ok(Loaded {
            entries: vec![img_entry(1, 7, "https://x/1.png")],
            feed_titles,
            total_unread: 1,
            pending_ids: Vec::new(),
        })));
        app.spinner_tick = 0; // freeze the animation frame
        let backend = ratatui::backend::TestBackend::new(100, 20);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|frame| app.draw(frame)).unwrap();
        let buffer = terminal.backend().buffer();
        // The footer is the last row.
        let footer: String = (0..100)
            .map(|x| buffer.cell((x, 19)).unwrap().symbol().to_string())
            .collect();
        assert!(
            footer.contains("Loading 0 of 1 images"),
            "indicator text present: {footer:?}"
        );
        assert!(footer.contains('⠋'), "frozen spinner frame: {footer:?}");
        // Help text is intact and the indicator sits to its right.
        let move_at = footer.find("move").expect("help still shown");
        let loading_at = footer.find("Loading").unwrap();
        assert!(loading_at > move_at, "indicator right of help: {footer:?}");
    }

    // --- extended-mode features (TASK-21/22/23) ---------------------------

    use crate::feedbin::{EntryImages, ImageSize, JsonFeed};

    fn lead_images(cdn_url: &str) -> Option<Box<EntryImages>> {
        Some(Box::new(EntryImages {
            size_1: Some(ImageSize {
                cdn_url: Some(cdn_url.to_string()),
            }),
        }))
    }

    /// Flatten a rendered reader into a newline-joined string.
    fn flatten_reader(entry: &Entry, images: &HashMap<String, ImageState>) -> String {
        reader_text(entry, images, 80, theme::ROSE)
            .lines
            .iter()
            .flat_map(|l| l.spans.iter().map(|s| s.content.to_string()))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn extended_entry(
        content: &str,
        images: Option<Box<EntryImages>>,
        enclosure: Option<Box<Enclosure>>,
        json_feed: Option<Box<JsonFeed>>,
    ) -> Entry {
        Entry {
            id: 1,
            feed_id: 7,
            title: Some("T".to_string()),
            url: Some("https://permalink".to_string()),
            author: None,
            published: None,
            summary: None,
            content: Some(content.to_string()),
            images,
            enclosure,
            json_feed,
        }
    }

    #[test]
    fn open_precedence_is_enclosure_then_external_then_url() {
        let build = |entry: Entry| {
            let mut feed_titles = HashMap::new();
            feed_titles.insert(7, "Feed".to_string());
            let mut app = App::new();
            app.apply(Msg::Loaded(Ok(Loaded {
                entries: vec![entry],
                feed_titles,
                total_unread: 1,
                pending_ids: Vec::new(),
            })));
            app
        };
        let enc = || {
            Some(Box::new(Enclosure {
                enclosure_url: Some("https://media.mp3".to_string()),
                enclosure_type: Some("audio/mpeg".to_string()),
                itunes_duration: None,
            }))
        };
        let ext = || {
            Some(Box::new(JsonFeed {
                external_url: Some("https://external".to_string()),
            }))
        };
        // Plain entry: the permalink.
        assert_eq!(
            build(extended_entry("<p>b</p>", None, None, None))
                .selected_url()
                .as_deref(),
            Some("https://permalink")
        );
        // Link blog: external_url wins over the permalink (TASK-23).
        assert_eq!(
            build(extended_entry("<p>b</p>", None, None, ext()))
                .selected_url()
                .as_deref(),
            Some("https://external")
        );
        // Podcast: the enclosure wins over everything (TASK-22).
        assert_eq!(
            build(extended_entry("<p>b</p>", None, enc(), ext()))
                .selected_url()
                .as_deref(),
            Some("https://media.mp3")
        );
    }

    #[test]
    fn lead_image_shown_and_prefetched_only_without_an_inline_image() {
        let mut images = HashMap::new();
        // A resolved (Failed) state makes the placeholder echo the URL so we can
        // assert which image the reader chose.
        images.insert("https://cdn/lead.jpg".to_string(), ImageState::Failed);

        // No inline <img>: the lead image is used (TASK-21).
        let no_inline = extended_entry(
            "<p>text only</p>",
            lead_images("https://cdn/lead.jpg"),
            None,
            None,
        );
        assert!(
            flatten_reader(&no_inline, &images).contains("https://cdn/lead.jpg"),
            "lead image shown when the body has none"
        );

        // Body has an inline <img>: the lead image is suppressed (no dup).
        let with_inline = extended_entry(
            "<p>x</p><img src=\"https://body/img.png\">",
            lead_images("https://cdn/lead.jpg"),
            None,
            None,
        );
        let out = flatten_reader(&with_inline, &images);
        assert!(
            !out.contains("https://cdn/lead.jpg"),
            "lead image suppressed when the body has an inline image: {out:?}"
        );
        assert!(out.contains("https://body/img.png"), "inline image kept");

        // Pre-fetch picks up the lead image too, so it loads and counts.
        let mut feed_titles = HashMap::new();
        feed_titles.insert(7, "Feed".to_string());
        let mut app = App::new();
        app.apply(Msg::Loaded(Ok(Loaded {
            entries: vec![extended_entry(
                "<p>text only</p>",
                lead_images("https://cdn/lead.jpg"),
                None,
                None,
            )],
            feed_titles,
            total_unread: 1,
            pending_ids: Vec::new(),
        })));
        assert!(
            app.image_urls.contains(&"https://cdn/lead.jpg".to_string()),
            "lead image is pre-fetched"
        );
    }

    #[test]
    fn reader_shows_podcast_indicator_and_external_link_with_permalink() {
        let entry = extended_entry(
            "<p>body</p>",
            None,
            Some(Box::new(Enclosure {
                enclosure_url: Some("https://media.mp3".to_string()),
                enclosure_type: Some("audio/mpeg".to_string()),
                itunes_duration: Some("2823".to_string()),
            })),
            Some(Box::new(JsonFeed {
                external_url: Some("https://external-target".to_string()),
            })),
        );
        let out = flatten_reader(&entry, &HashMap::new());
        // Podcast: media kind + duration (2823s = 47:03) (TASK-22).
        assert!(out.contains("Audio · 47:03"), "podcast indicator: {out:?}");
        // Link blog: external target is the primary link; permalink kept (TASK-23).
        assert!(
            out.contains("https://external-target"),
            "external url: {out:?}"
        );
        assert!(
            out.contains("permalink: https://permalink"),
            "permalink kept: {out:?}"
        );
    }

    #[test]
    fn format_duration_seconds_and_passthrough() {
        assert_eq!(format_duration("2823"), "47:03");
        assert_eq!(format_duration("3661"), "1:01:01");
        assert_eq!(format_duration("42"), "0:42");
        // Already formatted (non-numeric) passes through untouched.
        assert_eq!(format_duration("47:03"), "47:03");
    }

    #[test]
    fn images_are_queued_top_to_bottom() {
        // Two sources; the queue must follow on-screen order: sources by name
        // (Apple before Banana), and within each, articles oldest-first.
        let mut feed_titles = HashMap::new();
        feed_titles.insert(9, "Apple".to_string());
        feed_titles.insert(7, "Banana".to_string());
        let mk = |id: i64, feed: i64, url: &str, published: &str| Entry {
            id,
            feed_id: feed,
            title: Some(format!("t{id}")),
            url: None,
            author: None,
            published: Some(published.to_string()),
            summary: None,
            content: Some(format!("<img src=\"{url}\">")),
            images: None,
            enclosure: None,
            json_feed: None,
        };
        // Stored newest-first per source, as load() produces.
        let entries = vec![
            mk(2, 9, "apple-new", "2026-02"),
            mk(1, 9, "apple-old", "2026-01"),
            mk(4, 7, "banana-new", "2026-02"),
            mk(3, 7, "banana-old", "2026-01"),
        ];
        let mut app = App::new();
        app.apply(Msg::Loaded(Ok(Loaded {
            entries,
            feed_titles,
            total_unread: 4,
            pending_ids: Vec::new(),
        })));
        let queue: Vec<String> = app.image_queue.iter().cloned().collect();
        assert_eq!(
            queue,
            vec![
                "apple-old".to_string(),
                "apple-new".to_string(),
                "banana-old".to_string(),
                "banana-new".to_string(),
            ]
        );
    }

    #[test]
    fn focusing_an_unfetched_article_bumps_it_to_the_front() {
        let mut feed_titles = HashMap::new();
        feed_titles.insert(7, "Feed".to_string());
        // Stored newest-first; display (and base queue) is oldest-first: a, b, c.
        let entries = vec![
            img_entry(3, 7, "c"),
            img_entry(2, 7, "b"),
            img_entry(1, 7, "a"),
        ];
        let mut app = App::new();
        app.apply(Msg::Loaded(Ok(Loaded {
            entries,
            feed_titles,
            total_unread: 3,
            pending_ids: Vec::new(),
        })));
        assert_eq!(
            app.image_queue.iter().cloned().collect::<Vec<_>>(),
            vec!["a".to_string(), "b".to_string(), "c".to_string()],
            "base order is top-to-bottom"
        );

        // Jump to the bottom article (id 3); its still-queued image bumps front,
        // the rest keep their top-to-bottom order.
        app.selected_article = Some(3);
        app.prioritize_selected_images();
        assert_eq!(
            app.image_queue.iter().cloned().collect::<Vec<_>>(),
            vec!["c".to_string(), "a".to_string(), "b".to_string()]
        );
    }

    #[test]
    fn reader_scrolls_long_wrapped_content() {
        // One long paragraph with no newlines: few *unwrapped* lines, but it
        // word-wraps to many rows — so it must still be scrollable. (Regression
        // for clamping scroll against the raw line count instead of the wrapped
        // height, which pinned the reader at the top.)
        let long = "word ".repeat(300);
        let mut feed_titles = HashMap::new();
        feed_titles.insert(9, "Feed".to_string());
        let entry = Entry {
            id: 1,
            feed_id: 9,
            title: Some("T".to_string()),
            url: None,
            author: None,
            published: None,
            summary: None,
            content: Some(format!("<p>{long}</p>")),
            images: None,
            enclosure: None,
            json_feed: None,
        };
        let mut app = App::new();
        app.apply(Msg::Loaded(Ok(Loaded {
            entries: vec![entry],
            feed_titles,
            total_unread: 1,
            pending_ids: Vec::new(),
        })));
        app.focus_right();
        app.focus_right(); // Reader

        let backend = ratatui::backend::TestBackend::new(40, 8);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        app.move_cursor(1); // scroll down one step
        terminal.draw(|frame| app.draw(frame)).unwrap();
        assert_eq!(
            app.reader_scroll, READER_SCROLL_STEP,
            "overflowing wrapped content must scroll, not clamp to 0"
        );
    }

    #[test]
    fn panes_inset_content_from_their_borders() {
        // TASK-12: every pane insets its content one cell from the left border
        // (horizontal padding), with no top/bottom inset, applied consistently
        // through the shared column block.
        let mut app = app_with(&[(9, "Hacker News", 2)]);
        app.focus_right(); // focus Articles so the reader also shows a body
        let backend = ratatui::backend::TestBackend::new(120, 20);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|frame| app.draw(frame)).unwrap();
        let buffer = terminal.backend().buffer();
        let sym = |x: u16, y: u16| buffer.cell((x, y)).unwrap().symbol().to_string();

        // Left borders: Sources at x=0, Articles at x=30 (25% of 120), Reader at
        // x=72 (25%+35%). With one cell of horizontal padding and no top inset,
        // content starts at column border(1)+padding(1)=2, on the first row below
        // the top border (row 1).
        for left in [0u16, 30, 72] {
            let content = left + 2;
            assert_eq!(
                sym(left + 1, 1),
                " ",
                "left padding: blank cell before content at column {}",
                left + 1
            );
            assert_ne!(
                sym(content, 1),
                " ",
                "content is present on row 1, inset from the border at column {content}"
            );
        }

        // Spot-check the inset text itself in the Sources pane.
        let name: String = (2..13).map(|x| sym(x, 1)).collect();
        assert_eq!(name, "Hacker News", "source name inset by border + padding");
    }

    #[test]
    fn reader_is_empty_while_a_source_is_focused() {
        let app = app_with(&[(9, "Hacker News", 1)]);
        // Default focus is Sources -> reader shows nothing (no body).
        let backend = ratatui::backend::TestBackend::new(120, 20);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        let mut app = app;
        terminal.draw(|frame| app.draw(frame)).unwrap();
        let rendered: String = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(ratatui::buffer::Cell::symbol)
            .collect();
        assert!(
            !rendered.contains("Body"),
            "reader empty while on a source: {rendered:?}"
        );
    }

    #[test]
    fn wrap_title_keeps_short_titles_on_one_line() {
        assert_eq!(wrap_title("Hello world", 40), vec!["Hello world"]);
    }

    #[test]
    fn wrap_title_wraps_on_whitespace_within_width() {
        let lines = wrap_title("Hello World Foo", 10);
        assert!(lines.len() >= 2, "a long title wraps: {lines:?}");
        for line in &lines {
            assert!(
                UnicodeWidthStr::width(line.as_str()) <= 10,
                "each wrapped line fits the width: {line:?}"
            );
        }
        // Every word is preserved, in order (nothing truncated).
        assert_eq!(lines.join(" "), "Hello World Foo");
    }

    #[test]
    fn wrap_title_hard_breaks_a_word_wider_than_the_line() {
        assert_eq!(wrap_title("abcdefghij", 4), vec!["abcd", "efgh", "ij"]);
    }

    #[test]
    fn wrap_title_is_never_empty_and_guards_zero_width() {
        assert_eq!(wrap_title("", 10), vec![String::new()]);
        // width 0 is clamped to 1 — must not loop forever.
        assert_eq!(wrap_title("ab", 0), vec!["a", "b"]);
    }

    #[test]
    fn article_titles_wrap_and_highlight_covers_the_whole_item() {
        // Two articles in one source; the oldest (selected) has a title that must
        // wrap at the narrow pane width.
        let mut feed_titles = HashMap::new();
        feed_titles.insert(9, "Feed".to_string());
        let mk = |id: i64, title: &str, published: &str| Entry {
            id,
            feed_id: 9,
            title: Some(title.to_string()),
            url: None,
            author: None,
            published: Some(published.to_string()),
            summary: None,
            content: None,
            images: None,
            enclosure: None,
            json_feed: None,
        };
        // Stored newest-first, as load() produces; the column shows oldest-first.
        let entries = vec![
            mk(2, "Second", "2026-02-01T00:00:00Z"),
            mk(1, "Alpha Bravo", "2026-01-01T00:00:00Z"),
        ];
        let mut app = App::new();
        app.apply(Msg::Loaded(Ok(Loaded {
            entries,
            feed_titles,
            total_unread: 2,
            pending_ids: Vec::new(),
        })));
        app.focus_right(); // focus Articles; the oldest article (id 1) is selected
        assert_eq!(app.selected_article, Some(1));

        // Width 40: Articles pane is x∈[10,24); inner content is cols [12,22),
        // 10 wide, starting at row 1 (no top inset). "Alpha Bravo" wraps to two
        // lines at width 10, and items render contiguously (no blank gap).
        let backend = ratatui::backend::TestBackend::new(40, 20);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|frame| app.draw(frame)).unwrap();
        let buffer = terminal.backend().buffer();
        let cell = |x: u16, y: u16| buffer.cell((x, y)).unwrap();
        let content = |y: u16| (12u16..22).map(|x| cell(x, y).symbol()).collect::<String>();
        let reversed = |y: u16| {
            cell(12, y)
                .modifier
                .contains(ratatui::style::Modifier::REVERSED)
        };

        // No blank separator now, so the highlight delimits the selected item: its
        // wrapped lines are the leading contiguous highlighted rows (AC#3).
        let mut title_rows = Vec::new();
        let mut y = 1u16;
        while y < 18 && reversed(y) {
            title_rows.push(y);
            y += 1;
        }
        assert!(
            title_rows.len() >= 2,
            "the long title wrapped onto multiple highlighted lines, got {title_rows:?}"
        );
        // The full title is visible across those lines — no truncation (AC#1).
        let joined = title_rows
            .iter()
            .map(|&r| content(r).trim().to_string())
            .collect::<Vec<_>>()
            .join(" ");
        assert_eq!(joined, "Alpha Bravo");
        // The next article follows immediately — items are contiguous (no gap) and
        // the unselected item is not highlighted.
        assert!(
            content(y).starts_with("Second"),
            "next item directly follows: {:?}",
            content(y)
        );
        assert!(!reversed(y), "the unselected next item is not highlighted");
    }

    #[test]
    fn accent_is_applied_to_focused_chrome_and_selection() {
        let mut app = app_with(&[(9, "Hacker News", 2)]);
        app.focus_right(); // focus Articles (x∈[30,72)); Sources stays unfocused
        let backend = ratatui::backend::TestBackend::new(120, 20);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|frame| app.draw(frame)).unwrap();
        let buffer = terminal.backend().buffer();
        let cell = |x: u16, y: u16| buffer.cell((x, y)).unwrap();

        // Focused pane's border is rose; the unfocused pane's border stays neutral.
        assert_eq!(
            cell(50, 0).fg,
            theme::ROSE,
            "focused (Articles) border is rose"
        );
        assert_eq!(
            cell(15, 0).fg,
            ratatui::style::Color::Reset,
            "unfocused (Sources) border stays neutral — accent is restrained"
        );
        // The selection bar is rose AND still reversed (keeps the existing contract).
        let selected = cell(32, 1);
        assert_eq!(selected.fg, theme::ROSE, "selection bar is rose");
        assert!(
            selected
                .modifier
                .contains(ratatui::style::Modifier::REVERSED),
            "selection stays reversed"
        );
        // The reader title is rose.
        assert_eq!(cell(74, 1).fg, theme::ROSE, "reader title is rose");
    }

    #[test]
    fn help_overlay_mutes_focus_and_selection_accent_and_restores_it() {
        // TASK-46: while the overlay is open the focused column's border and the
        // selection bar recede to grey; the overlay's own title stays rose; and
        // closing it restores the rose accent with no lingering grey.
        use ratatui::style::Modifier;
        let mut app = app_with(&[(9, "Hacker News", 2)]);
        app.focus_right(); // focus Articles (x∈[30,72))
        // Tall enough that the centered overlay never covers rows 0–1 (the focused
        // border row and the first selection row we probe).
        let backend = ratatui::backend::TestBackend::new(120, 30);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();

        // Closed: focused border + selection are rose.
        terminal.draw(|f| app.draw(f)).unwrap();
        {
            let b = terminal.backend().buffer();
            assert_eq!(
                b.cell((50, 0)).unwrap().fg,
                theme::ROSE,
                "border rose (closed)"
            );
            assert_eq!(
                b.cell((32, 1)).unwrap().fg,
                theme::ROSE,
                "selection rose (closed)"
            );
        }

        // Open: they mute to grey; the overlay itself stays rose.
        app.show_help = true;
        terminal.draw(|f| app.draw(f)).unwrap();
        {
            let b = terminal.backend().buffer();
            assert_eq!(
                b.cell((50, 0)).unwrap().fg,
                theme::MUTED,
                "focused border greyed under the overlay"
            );
            let sel = b.cell((32, 1)).unwrap();
            assert_eq!(sel.fg, theme::MUTED, "selection greyed under the overlay");
            assert!(
                sel.modifier.contains(Modifier::REVERSED),
                "selection stays reversed even when muted"
            );
            // The overlay's own title stays rose (the only 'K' on screen).
            let overlay_rose = (0..30u16).any(|y| {
                (0..120u16).any(|x| {
                    let c = b.cell((x, y)).unwrap();
                    c.symbol() == "K" && c.fg == theme::ROSE
                })
            });
            assert!(overlay_rose, "the overlay title stays rose");
        }

        // Closed again: rose restored, no lingering grey.
        app.show_help = false;
        terminal.draw(|f| app.draw(f)).unwrap();
        {
            let b = terminal.backend().buffer();
            assert_eq!(
                b.cell((50, 0)).unwrap().fg,
                theme::ROSE,
                "border rose again after close"
            );
            assert_eq!(
                b.cell((32, 1)).unwrap().fg,
                theme::ROSE,
                "selection rose again after close"
            );
        }
    }

    #[test]
    fn configured_highlight_color_recolors_all_chrome() {
        // TASK-45 AC #1: a configured accent replaces rose everywhere it's used
        // (focused border/title, selection bar, reader title, footer keys) — and
        // nothing is missed (no rose leaks into the chrome).
        const BLUE: Color = Color::Rgb(0x5f, 0xaf, 0xff);
        let mut app = app_with(&[(9, "Hacker News", 2)]);
        app.base_accent = BLUE;
        app.focus_right(); // focus Articles so the reader shows the article
        let backend = ratatui::backend::TestBackend::new(120, 20);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|f| app.draw(f)).unwrap();
        let b = terminal.backend().buffer();
        let cell = |x: u16, y: u16| b.cell((x, y)).unwrap();

        assert_eq!(cell(50, 0).fg, BLUE, "focused border uses the accent");
        assert_eq!(cell(32, 1).fg, BLUE, "selection bar uses the accent");
        assert_eq!(cell(74, 1).fg, BLUE, "reader title uses the accent");
        assert!(
            (0..120).any(|x| b.cell((x, 19)).unwrap().fg == BLUE),
            "footer keys use the accent"
        );
        // With an accent configured and no mascot on screen, no rose remains.
        let rose_leak =
            (0..20u16).any(|y| (0..120u16).any(|x| b.cell((x, y)).unwrap().fg == theme::ROSE));
        assert!(
            !rose_leak,
            "no rose accent leaks once a highlight color is set"
        );
    }

    #[test]
    fn configured_accent_leaves_the_caught_up_rose_alone() {
        // The mascot keeps its own rose palette; only the chrome (here, the
        // footer keys) takes the configured accent.
        const BLUE: Color = Color::Rgb(0x5f, 0xaf, 0xff);
        let mut app = App::new();
        app.base_accent = BLUE;
        app.apply(Msg::Loaded(Ok(Loaded {
            entries: vec![],
            feed_titles: HashMap::new(),
            total_unread: 0,
            pending_ids: Vec::new(),
        }))); // Ready + empty → the caught-up rose
        let backend = ratatui::backend::TestBackend::new(60, 20);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|f| app.draw(f)).unwrap();
        let b = terminal.backend().buffer();
        let any = |c: Color| {
            (0..b.area.height).any(|y| (0..b.area.width).any(|x| b.cell((x, y)).unwrap().fg == c))
        };
        assert!(any(theme::ROSE_LIGHT), "the mascot keeps its rose petals");
        assert!(any(BLUE), "footer keys still take the configured accent");
    }

    #[test]
    fn footer_keys_are_accented() {
        let mut app = app_with(&[(9, "Hacker News", 1)]);
        let backend = ratatui::backend::TestBackend::new(120, 20);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|frame| app.draw(frame)).unwrap();
        let buffer = terminal.backend().buffer();
        // At least one footer key letter is rose (footer is the last row).
        let has_rose = (0..120).any(|x| buffer.cell((x, 19)).unwrap().fg == theme::ROSE);
        assert!(has_rose, "footer key letters are accented rose");
        // The arrow glyphs are accented too (rose), not just the letters: the line
        // is " ↑↓ …", so the up arrow sits at column 1.
        let arrow = buffer.cell((1, 19)).unwrap();
        assert_eq!(arrow.symbol(), "↑", "first footer glyph is the up arrow");
        assert_eq!(arrow.fg, theme::ROSE, "arrow keys are accented rose too");
        // The help wording is intact (e.g. the 'quit' label).
        let rendered: String = buffer
            .content
            .iter()
            .map(ratatui::buffer::Cell::symbol)
            .collect();
        assert!(rendered.contains("quit"), "footer help text preserved");
    }

    #[test]
    fn all_caught_up_shows_the_rose() {
        let mut app = App::new();
        app.apply(Msg::Loaded(Ok(Loaded {
            entries: vec![],
            feed_titles: HashMap::new(),
            total_unread: 0,
            pending_ids: Vec::new(),
        }))); // Ready + empty → the rose splash
        let backend = ratatui::backend::TestBackend::new(80, 24);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|frame| app.draw(frame)).unwrap();
        let buffer = terminal.backend().buffer();
        let rendered: String = buffer
            .content
            .iter()
            .map(ratatui::buffer::Cell::symbol)
            .collect();
        assert!(
            rendered.contains("All caught up"),
            "caught-up caption shown"
        );
        assert!(
            !rendered.contains("Sources"),
            "no three-column chrome on the splash"
        );
        assert!(
            !rendered.contains("Articles"),
            "no three-column chrome on the splash"
        );
        // The art/caption is colored (a rose-family RGB foreground appears).
        let has_rose = buffer
            .content
            .iter()
            .any(|c| matches!(c.fg, ratatui::style::Color::Rgb(r, _, b) if r > 0x80 && b > 0x40));
        assert!(has_rose, "the rose splash is colored");
    }

    #[test]
    fn loading_first_frame_is_unchanged() {
        let mut app = App::new(); // Loading + empty
        let backend = ratatui::backend::TestBackend::new(80, 24);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|frame| app.draw(frame)).unwrap();
        let rendered: String = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(ratatui::buffer::Cell::symbol)
            .collect();
        assert!(
            rendered.contains("Loading"),
            "loading still shows its placeholder"
        );
        assert!(
            !rendered.contains("All caught up"),
            "the rose is reserved for the caught-up state, not loading"
        );
    }

    #[test]
    fn caught_up_degrades_on_a_tiny_terminal() {
        let mut app = App::new();
        app.apply(Msg::Loaded(Ok(Loaded {
            entries: vec![],
            feed_titles: HashMap::new(),
            total_unread: 0,
            pending_ids: Vec::new(),
        })));
        // Far too small for the art — must fall back to the caption without panicking.
        let backend = ratatui::backend::TestBackend::new(10, 3);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|frame| app.draw(frame)).unwrap();
        let rendered: String = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(ratatui::buffer::Cell::symbol)
            .collect();
        assert!(
            rendered.contains("All"),
            "tiny terminal still shows the caption"
        );
    }

    /// One article whose body overflows the reader; `extra` focus_right() steps move
    /// the cursor (1 → Articles, 2 → Reader).
    fn app_with_long_article(content: &str, focus_steps: usize) -> App {
        let mut feed_titles = HashMap::new();
        feed_titles.insert(9, "Feed".to_string());
        let entry = Entry {
            id: 1,
            feed_id: 9,
            title: Some("T".to_string()),
            url: None,
            author: None,
            published: None,
            summary: None,
            content: Some(content.to_string()),
            images: None,
            enclosure: None,
            json_feed: None,
        };
        let mut app = App::new();
        app.apply(Msg::Loaded(Ok(Loaded {
            entries: vec![entry],
            feed_titles,
            total_unread: 1,
            pending_ids: Vec::new(),
        })));
        for _ in 0..focus_steps {
            app.focus_right();
        }
        app
    }

    /// Is the reader's right-edge column (col 119 at width 120) showing a scrollbar
    /// thumb? The thumb glyph (█) is distinct from the border/track (│).
    fn reader_has_scrollbar(app: &mut App) -> bool {
        let backend = ratatui::backend::TestBackend::new(120, 20);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|frame| app.draw(frame)).unwrap();
        let buffer = terminal.backend().buffer();
        (0..20u16).any(|y| buffer.cell((119, y)).unwrap().symbol() == "█")
    }

    #[test]
    fn reader_scrollbar_shows_when_focused_and_overflowing() {
        let long = format!("<p>{}</p>", "word ".repeat(300));
        let mut app = app_with_long_article(&long, 2); // focus Reader
        assert!(matches!(app.focus, Focus::Reader));
        assert!(
            reader_has_scrollbar(&mut app),
            "a scrollbar thumb rides the reader's right edge when content overflows"
        );
    }

    #[test]
    fn reader_scrollbar_hidden_when_content_fits() {
        let mut app = app_with_long_article("<p>Short.</p>", 2); // focus Reader
        assert!(matches!(app.focus, Focus::Reader));
        assert!(
            !reader_has_scrollbar(&mut app),
            "no scrollbar when the content fits the viewport"
        );
    }

    #[test]
    fn reader_scrollbar_hidden_when_reader_unfocused() {
        let long = format!("<p>{}</p>", "word ".repeat(300));
        // Focus Articles: the reader shows the (overflowing) article but isn't active.
        let mut app = app_with_long_article(&long, 1);
        assert!(matches!(app.focus, Focus::Articles));
        assert!(
            !reader_has_scrollbar(&mut app),
            "no scrollbar unless the reader is the focused pane"
        );
    }

    #[test]
    fn reader_scrollbar_thumb_reaches_the_bottom_when_fully_scrolled() {
        let long = format!("<p>{}</p>", "word ".repeat(300));
        let mut app = app_with_long_article(&long, 2); // Reader focused
        app.reader_scroll = u16::MAX; // draw() clamps to max_scroll → fully scrolled
        let backend = ratatui::backend::TestBackend::new(120, 20);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|frame| app.draw(frame)).unwrap();
        let buffer = terminal.backend().buffer();
        // The reader area is 19 rows tall, so the scrollbar track (vertical inset 1)
        // spans rows 1..=17. Fully scrolled, the thumb must reach the last track row
        // (17) — the bug was it stopped partway down.
        assert_eq!(
            buffer.cell((119, 17)).unwrap().symbol(),
            "█",
            "thumb reaches the bottom of the track at max scroll"
        );
    }
}
