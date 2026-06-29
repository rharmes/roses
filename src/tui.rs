//! Full-screen terminal UI (TASK-6, redesigned in TASK-11).
//!
//! A three-column Miller-columns layout on the crossterm backend: **sources**
//! (feeds with unread counts) | **articles** for the selected source | a
//! scrollable **reader**. A single focus "cursor" (reversed text) moves with the
//! arrow/`hjkl` keys — up/down within the focused column, left/right between
//! columns. Feedbin is queried on a background `tokio::spawn_blocking` task so
//! input never blocks; results arrive over a channel drained each tick.

use std::collections::{HashMap, HashSet, VecDeque};
use std::time::Duration;

use anyhow::{Context, Result};
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::layout::{Alignment, Constraint, Flex, Layout, Rect};
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, List, ListItem, ListState, Padding, Paragraph, Wrap};
use ratatui::{DefaultTerminal, Frame};
use tokio::runtime::Handle;
use tokio::sync::mpsc::{self, UnboundedSender};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::browser;
use crate::config::BrowserPref;
use crate::feedbin::{Client, Entry};
use crate::theme;

/// How many of the newest unread entries to load.
const DISPLAY_LIMIT: usize = 50;
/// Lines the reader scrolls per PageUp/PageDown.
const READER_PAGE: u16 = 10;
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
}

/// Which unread-state write a background task performed.
#[derive(Clone, Copy)]
enum WriteOp {
    MarkRead,
    Undo,
}

/// An entry that was marked read and can be restored, remembering its position
/// in `entries` so undo can put it back in published order.
struct Undone {
    entry: Entry,
    index: usize,
}

/// Message from a background worker to the UI loop.
enum Msg {
    Loaded(Result<Loaded, String>),
    /// Result of a mark-read / undo write, carrying the entry + index so the UI
    /// can finalize on success or roll back on failure (TASK-7 AC #4).
    Write {
        op: WriteOp,
        entry: Entry,
        index: usize,
        result: Result<(), String>,
    },
    /// Rendered half-block art for an entry image, keyed by its source URL.
    Image {
        url: String,
        result: Result<Vec<Line<'static>>, String>,
    },
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
    Undo,
    OpenInBrowser,
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

struct App {
    status: Status,
    /// All loaded unread entries, newest first.
    entries: Vec<Entry>,
    feed_titles: HashMap<i64, String>,
    total_unread: usize,
    focus: Focus,
    /// Selection is tracked by id, not index, so it survives mark/undo edits.
    selected_source: Option<i64>,
    selected_article: Option<i64>,
    reader_scroll: u16,
    /// Rendered/loading entry images, keyed by source URL (TASK-8).
    images: HashMap<String, ImageState>,
    /// Image URLs awaiting a fetch slot, in priority order (pre-fetch queue).
    image_queue: VecDeque<String>,
    /// Inner width of the reader pane from the last draw, used to size images.
    reader_width: u16,
    /// Marked-read entries that can be restored, most recent last.
    undo_stack: Vec<Undone>,
    /// Transient status line (e.g. a write failure); cleared on the next key.
    notice: Option<String>,
    should_quit: bool,
}

impl App {
    fn new() -> Self {
        Self {
            status: Status::Loading,
            entries: Vec::new(),
            feed_titles: HashMap::new(),
            total_unread: 0,
            focus: Focus::Sources,
            selected_source: None,
            selected_article: None,
            reader_scroll: 0,
            images: HashMap::new(),
            image_queue: VecDeque::new(),
            reader_width: 0,
            undo_stack: Vec::new(),
            notice: None,
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
    fn selected_url(&self) -> Option<String> {
        self.selected_article_entry().and_then(|e| e.url.clone())
    }

    /// Image URLs in one article's body, in document order.
    fn article_image_urls(&self, entry: &Entry) -> Vec<String> {
        let body = entry
            .content
            .as_deref()
            .or(entry.summary.as_deref())
            .unwrap_or("");
        content_blocks(body)
            .into_iter()
            .filter_map(|block| match block {
                Segment::Image(url) => Some(url),
                Segment::Text(_) => None,
            })
            .collect()
    }

    /// Enqueue every not-yet-seen image for background pre-fetch in on-screen
    /// order — sources top-to-bottom (by name), and within each source articles
    /// top-to-bottom (oldest first) — marking each `Loading` so it is fetched
    /// once.
    fn refill_image_queue(&mut self) {
        let source_ids: Vec<i64> = self.sources().into_iter().map(|(id, _)| id).collect();
        for feed_id in source_ids {
            for article_id in self.article_ids(feed_id) {
                let Some(entry) = self.entries.iter().find(|e| e.id == article_id) else {
                    continue;
                };
                for url in self.article_image_urls(entry) {
                    if !self.images.contains_key(&url) {
                        self.images.insert(url.clone(), ImageState::Loading);
                        self.image_queue.push_back(url);
                    }
                }
            }
        }
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

    // --- state updates -----------------------------------------------------

    fn apply(&mut self, msg: Msg) {
        match msg {
            Msg::Loaded(Ok(loaded)) => {
                self.entries = loaded.entries;
                self.feed_titles = loaded.feed_titles;
                self.total_unread = loaded.total_unread;
                self.status = Status::Ready;
                self.undo_stack.clear();
                self.notice = None;
                self.reset_selection();
                self.refill_image_queue();
            }
            Msg::Loaded(Err(err)) => self.status = Status::Failed(err),

            Msg::Write {
                op: WriteOp::MarkRead,
                entry,
                index,
                result,
            } => match result {
                Ok(()) => self.undo_stack.push(Undone { entry, index }),
                Err(err) => {
                    self.reinsert(entry, index);
                    self.notice = Some(format!("Mark read failed (restored): {err}"));
                }
            },

            Msg::Write {
                op: WriteOp::Undo,
                entry,
                index,
                result,
            } => {
                if let Err(err) = result {
                    if let Some(pos) = self.entries.iter().position(|e| e.id == entry.id) {
                        let feed_id = entry.feed_id;
                        self.entries.remove(pos);
                        self.total_unread = self.total_unread.saturating_sub(1);
                        self.reselect_after_removal(feed_id, 0);
                    }
                    self.undo_stack.push(Undone { entry, index });
                    self.notice = Some(format!("Undo failed (kept read): {err}"));
                }
            }

            Msg::Image { url, result } => {
                let state = match result {
                    Ok(lines) => ImageState::Ready(lines),
                    Err(_) => ImageState::Failed,
                };
                self.images.insert(url, state);
            }
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

    /// Move the cursor up (`-1`) or down (`+1`) within the focused column.
    fn move_cursor(&mut self, delta: i32) {
        match self.focus {
            Focus::Sources => self.move_source(delta),
            Focus::Articles => self.move_article(delta),
            Focus::Reader => {
                self.reader_scroll = if delta > 0 {
                    self.reader_scroll.saturating_add(1)
                } else {
                    self.reader_scroll.saturating_sub(1)
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
    /// the active target). Returns the removed entry + its index for the write.
    fn begin_mark_read(&mut self) -> Option<(Entry, usize)> {
        if self.focus == Focus::Sources {
            return None;
        }
        let article = self.selected_article?;
        let hint = self.article_index().unwrap_or(0);
        let index = self.entries.iter().position(|e| e.id == article)?;
        let entry = self.entries.remove(index);
        self.total_unread = self.total_unread.saturating_sub(1);
        self.reselect_after_removal(entry.feed_id, hint);
        Some((entry, index))
    }

    /// Optimistically restore the most recently marked-read entry.
    fn begin_undo(&mut self) -> Option<(Entry, usize)> {
        let Undone { entry, index } = self.undo_stack.pop()?;
        self.reinsert(entry.clone(), index);
        Some((entry, index))
    }

    /// Re-insert an entry near its original index, bump the unread count, and
    /// focus it in the articles column. Used by undo and mark-read rollback.
    fn reinsert(&mut self, entry: Entry, index: usize) {
        let at = index.min(self.entries.len());
        let (feed_id, id) = (entry.feed_id, entry.id);
        self.entries.insert(at, entry);
        self.total_unread = self.total_unread.saturating_add(1);
        self.selected_source = Some(feed_id);
        self.selected_article = Some(id);
        self.focus = Focus::Articles;
        self.reader_scroll = 0;
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
            KeyCode::Char('u') => return Action::Undo,
            KeyCode::Char('o') => return Action::OpenInBrowser,
            KeyCode::Char('r') => return Action::Reload,
            _ => {}
        }
        Action::None
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

        let footer_line = match &self.notice {
            Some(text) => Line::from(format!(" {text} ")).red(),
            None => footer_help(),
        };
        frame.render_widget(footer_line, footer);
    }

    fn column_block(&self, title: &'static str, focused: bool) -> Block<'static> {
        // The focused pane lights up in rose (border + title); unfocused panes stay
        // neutral (dim border, plain title) so only the active column draws the eye
        // (TASK-14, chrome-only accent).
        let (border, title_line) = if focused {
            (
                Style::new().fg(theme::ROSE).bold(),
                Line::from(Span::styled(title, Style::new().fg(theme::ROSE).bold())),
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
            Style::new().fg(theme::ROSE).reversed()
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
                    Span::raw(self.feed_name(feed_id).to_string()),
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
                let lines: Vec<Line> = wrap_title(title, width)
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

    fn draw_reader(&mut self, frame: &mut Frame, area: Rect) {
        let focused = self.focus == Focus::Reader;
        let block = self.column_block("Reader", focused);
        // The reader shows the article only when focus has moved off the
        // sources column (TASK-11: a focused source shows nothing here).
        let entry = match self.focus {
            Focus::Sources => None,
            Focus::Articles | Focus::Reader => self.selected_article_entry(),
        };
        let Some(entry) = entry else {
            frame.render_widget(Paragraph::new("").block(block), area);
            return;
        };

        let text = reader_text(entry, self.feed_name(entry.feed_id), &self.images);
        // The true content rect — inside the border *and* the padding (TASK-12) —
        // sets the wrap width and visible height used to clamp the scroll offset.
        let inner = block.inner(area);
        let inner_width = inner.width;
        let inner_height = inner.height;

        // Clamp scroll to the *wrapped* height (not the raw line count): one long
        // paragraph is a single line that word-wraps to many rows, so clamping on
        // `text.lines.len()` would pin the reader at the top. Measure without the
        // block so `inner_width` is the true content width.
        let wrapped = Paragraph::new(text.clone())
            .wrap(Wrap { trim: false })
            .line_count(inner_width) as u16;
        let max_scroll = wrapped.saturating_sub(inner_height);
        self.reader_scroll = self.reader_scroll.min(max_scroll);

        let reader = Paragraph::new(text)
            .block(block)
            .wrap(Wrap { trim: false })
            .scroll((self.reader_scroll, 0));
        frame.render_widget(reader, area);
    }
}

/// The footer key-help line, with the action keys accented in rose (TASK-14); the
/// arrows, labels, and separators stay dim.
fn footer_help() -> Line<'static> {
    let key = |k: &'static str| Span::styled(k, Style::new().fg(theme::ROSE).bold());
    let text = |s: &'static str| Span::raw(s).dim();
    Line::from(vec![
        text(" ↑↓ move · ←→ focus · "),
        key("m"),
        text(" read · "),
        key("u"),
        text(" undo · "),
        key("o"),
        text(" open · "),
        key("r"),
        text(" reload · "),
        key("q"),
        text(" quit "),
    ])
}

/// The "all caught up" rose (TASK-14): a vertically-centered ASCII rose graded
/// light→deep rose over a green stem, with a caption beneath. Degrades to just the
/// centered caption when the area is too small for the art.
fn draw_caught_up(frame: &mut Frame, area: Rect) {
    const ART: [&str; 8] = [
        "   ___   ",
        "  (   )  ",
        " ( (@) ) ",
        "  )   (  ",
        "   '-'   ",
        "    |    ",
        "   \\|/   ",
        "    |    ",
    ];
    const CAPTION: &str = "All caught up.";
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
        let caption = Line::from(Span::styled(CAPTION, Style::new().fg(theme::ROSE_DIM)));
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
    lines.push(Line::from(Span::styled(
        CAPTION,
        Style::new().fg(theme::ROSE_DIM),
    )));

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

/// Build the reader pane's content for one entry: a title/feed/url header, then
/// the body — text rendered from HTML, with images shown as half-block art (a
/// placeholder while loading, a notice when unavailable).
fn reader_text(entry: &Entry, feed: &str, images: &HashMap<String, ImageState>) -> Text<'static> {
    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(
        entry
            .title
            .clone()
            .unwrap_or_else(|| "(untitled)".to_string())
            .bold()
            .fg(theme::ROSE),
    ));
    let meta = match &entry.published {
        Some(published) => format!("{feed} · {published}"),
        None => feed.to_string(),
    };
    lines.push(Line::from(meta.dim()));
    if let Some(url) = &entry.url {
        lines.push(Line::from(url.clone().underlined()));
    }
    lines.push(Line::from(""));

    let body = entry
        .content
        .as_deref()
        .or(entry.summary.as_deref())
        .unwrap_or("(no content)");
    for block in content_blocks(body) {
        match block {
            Segment::Text(text) => {
                for line in text.lines() {
                    lines.push(Line::from(line.to_string()));
                }
            }
            Segment::Image(url) => match images.get(&url) {
                Some(ImageState::Ready(art)) => {
                    lines.push(Line::from(""));
                    lines.extend(art.iter().cloned());
                    lines.push(Line::from(""));
                }
                Some(ImageState::Failed) => {
                    lines.push(Line::from(format!("[image unavailable: {url}]")).dim());
                }
                _ => lines.push(Line::from(format!("[image loading… {url}]")).dim()),
            },
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

fn spawn_fetch(handle: &Handle, client: Client, tx: UnboundedSender<Msg>) {
    handle.spawn_blocking(move || {
        let result = load(&client).map_err(|e| format!("{e:#}"));
        let _ = tx.send(Msg::Loaded(result));
    });
}

/// Run a mark-read / undo network write on the blocking pool and report the
/// outcome — with the entry + index for rollback — back to the UI loop.
fn spawn_write(
    handle: &Handle,
    client: &Client,
    tx: &UnboundedSender<Msg>,
    op: WriteOp,
    entry: Entry,
    index: usize,
) {
    let client = client.clone();
    let tx = tx.clone();
    let id = entry.id;
    handle.spawn_blocking(move || {
        let net = match op {
            WriteOp::MarkRead => client.mark_read(&[id]),
            WriteOp::Undo => client.mark_unread(&[id]),
        };
        let result = net.map(|_| ()).map_err(|e| format!("{e:#}"));
        let _ = tx.send(Msg::Write {
            op,
            entry,
            index,
            result,
        });
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

/// Blocking fetch of the newest unread entries plus their feed names.
fn load(client: &Client) -> Result<Loaded> {
    let mut unread = client.unread_entry_ids()?;
    let total_unread = unread.len();
    unread.sort_unstable_by(|a, b| b.cmp(a));
    let sample: Vec<i64> = unread.into_iter().take(DISPLAY_LIMIT).collect();
    if sample.is_empty() {
        return Ok(Loaded {
            entries: Vec::new(),
            feed_titles: HashMap::new(),
            total_unread,
        });
    }
    let feed_titles = client.feed_titles()?;
    let mut entries = client.entries(&sample)?;
    entries.sort_by(|a, b| b.published.cmp(&a.published));
    Ok(Loaded {
        entries,
        feed_titles,
        total_unread,
    })
}

/// Run the full-screen TUI until the user quits, restoring the terminal on the
/// way out (including on panic, via ratatui's panic hook).
pub fn run(client: Client) -> Result<()> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .context("building the Tokio runtime")?;
    let handle = runtime.handle().clone();
    let browser_pref = crate::config::load_browser_pref().unwrap_or_default();

    let (tx, mut rx) = mpsc::unbounded_channel::<Msg>();
    spawn_fetch(&handle, client.clone(), tx.clone());

    let mut terminal = ratatui::init();
    let result = run_loop(&mut terminal, &handle, &client, &tx, &mut rx, &browser_pref);
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

fn run_loop(
    terminal: &mut DefaultTerminal,
    handle: &Handle,
    client: &Client,
    tx: &UnboundedSender<Msg>,
    rx: &mut mpsc::UnboundedReceiver<Msg>,
    browser_pref: &BrowserPref,
) -> Result<()> {
    let mut app = App::new();
    let mut images_in_flight = 0usize;
    let mut last_selected = None;
    while !app.should_quit {
        while let Ok(msg) = rx.try_recv() {
            if matches!(msg, Msg::Image { .. }) {
                images_in_flight = images_in_flight.saturating_sub(1);
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

        if event::poll(TICK).context("polling for input")?
            && let Event::Key(key) = event::read().context("reading input")?
            && key.kind == KeyEventKind::Press
        {
            match app.handle_key(key.code) {
                Action::None => {}
                Action::Reload => {
                    app.status = Status::Loading;
                    spawn_fetch(handle, client.clone(), tx.clone());
                }
                Action::MarkRead => {
                    if let Some((entry, index)) = app.begin_mark_read() {
                        spawn_write(handle, client, tx, WriteOp::MarkRead, entry, index);
                    }
                }
                Action::Undo => {
                    if let Some((entry, index)) = app.begin_undo() {
                        spawn_write(handle, client, tx, WriteOp::Undo, entry, index);
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
            published: Some("2026-06-29T00:00:00.000000Z".to_string()),
            summary: Some("summary".to_string()),
            content: content.map(str::to_string),
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
            published: Some(published.to_string()),
            summary: None,
            content: None,
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
        assert_eq!(app.reader_scroll, 1);
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
        let (entry, index) = app.begin_mark_read().expect("an article is selected");
        assert_eq!(app.article_ids(9).len(), 1);
        assert_eq!(app.total_unread, 1);
        app.apply(Msg::Write {
            op: WriteOp::MarkRead,
            entry,
            index,
            result: Ok(()),
        });
        assert_eq!(app.undo_stack.len(), 1);

        let (entry, index) = app.begin_undo().expect("something to undo");
        assert_eq!(app.article_ids(9).len(), 2);
        assert_eq!(app.total_unread, 2);
        app.apply(Msg::Write {
            op: WriteOp::Undo,
            entry,
            index,
            result: Ok(()),
        });
        assert_eq!(app.article_ids(9).len(), 2);
    }

    #[test]
    fn mark_read_failure_rolls_back() {
        let mut app = app_with(&[(9, "Hacker News", 2)]);
        app.focus_right();
        let (entry, index) = app.begin_mark_read().unwrap();
        assert_eq!(app.article_ids(9).len(), 1);
        app.apply(Msg::Write {
            op: WriteOp::MarkRead,
            entry,
            index,
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
            published: None,
            summary: None,
            content: Some("<img src=\"https://x/i.png\">".to_string()),
        };
        let collect = |images: &HashMap<String, ImageState>| -> String {
            reader_text(&entry, "Feed", images)
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

    fn img_entry(id: i64, feed_id: i64, img_url: &str) -> Entry {
        Entry {
            id,
            feed_id,
            title: Some(format!("t{id}")),
            url: None,
            published: None,
            summary: None,
            content: Some(format!("<p>body</p><img src=\"{img_url}\">")),
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
            published: None,
            summary: None,
            content: Some("<p>no image</p>".to_string()),
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
        })));
        assert_eq!(app.next_queued_image().as_deref(), Some("https://x/1.png"));
        assert!(app.next_queued_image().is_none());
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
            published: Some(published.to_string()),
            summary: None,
            content: Some(format!("<img src=\"{url}\">")),
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
            published: None,
            summary: None,
            content: Some(format!("<p>{long}</p>")),
        };
        let mut app = App::new();
        app.apply(Msg::Loaded(Ok(Loaded {
            entries: vec![entry],
            feed_titles,
            total_unread: 1,
        })));
        app.focus_right();
        app.focus_right(); // Reader

        let backend = ratatui::backend::TestBackend::new(40, 8);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        app.move_cursor(1); // scroll down one line
        terminal.draw(|frame| app.draw(frame)).unwrap();
        assert_eq!(
            app.reader_scroll, 1,
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
            published: Some(published.to_string()),
            summary: None,
            content: None,
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
    fn footer_keys_are_accented() {
        let mut app = app_with(&[(9, "Hacker News", 1)]);
        let backend = ratatui::backend::TestBackend::new(120, 20);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|frame| app.draw(frame)).unwrap();
        let buffer = terminal.backend().buffer();
        // At least one footer key letter is rose (footer is the last row).
        let has_rose = (0..120).any(|x| buffer.cell((x, 19)).unwrap().fg == theme::ROSE);
        assert!(has_rose, "footer key letters are accented rose");
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
}
