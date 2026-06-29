//! Full-screen terminal UI (TASK-6, redesigned in TASK-11).
//!
//! A three-column Miller-columns layout on the crossterm backend: **sources**
//! (feeds with unread counts) | **articles** for the selected source | a
//! scrollable **reader**. A single focus "cursor" (reversed text) moves with the
//! arrow/`hjkl` keys — up/down within the focused column, left/right between
//! columns. Feedbin is queried on a background `tokio::spawn_blocking` task so
//! input never blocks; results arrive over a channel drained each tick.

use std::collections::HashMap;
use std::time::Duration;

use anyhow::{Context, Result};
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::{DefaultTerminal, Frame};
use tokio::runtime::Handle;
use tokio::sync::mpsc::{self, UnboundedSender};

use crate::browser;
use crate::config::BrowserPref;
use crate::feedbin::{Client, Entry};

/// How many of the newest unread entries to load.
const DISPLAY_LIMIT: usize = 50;
/// Lines the reader scrolls per PageUp/PageDown.
const READER_PAGE: u16 = 10;
/// How long to wait for input before redrawing (also bounds how quickly a
/// finished background task shows up).
const TICK: Duration = Duration::from_millis(100);

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
        let [sources, articles, reader] = Layout::horizontal([
            Constraint::Percentage(25),
            Constraint::Percentage(35),
            Constraint::Percentage(40),
        ])
        .areas(main);

        self.draw_sources(frame, sources);
        self.draw_articles(frame, articles);
        self.draw_reader(frame, reader);

        let footer_line = match &self.notice {
            Some(text) => Line::from(format!(" {text} ")).red(),
            None => {
                Line::from(" ↑↓ move · ←→ focus · m read · u undo · o open · r reload · q quit ")
                    .dim()
            }
        };
        frame.render_widget(footer_line, footer);
    }

    fn column_block(&self, title: &'static str, focused: bool) -> Block<'static> {
        let border = if focused {
            Style::new().bold()
        } else {
            Style::new().dim()
        };
        Block::bordered().title(title).border_style(border)
    }

    /// Reversed text marks the active cursor; a bold row marks the remembered
    /// selection in an unfocused column.
    fn highlight(&self, focused: bool) -> Style {
        if focused {
            Style::new().reversed()
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
        let items: Vec<ListItem> = self
            .articles(feed_id)
            .iter()
            .map(|e| {
                ListItem::new(Line::from(
                    e.title.as_deref().unwrap_or("(untitled)").to_string(),
                ))
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

        let text = reader_text(entry, self.feed_name(entry.feed_id));
        let inner_width = area.width.saturating_sub(2);
        let inner_height = area.height.saturating_sub(2);

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

/// Build the reader pane's text for one entry: a title/feed/url header, then the
/// body rendered from HTML to plain text.
fn reader_text(entry: &Entry, feed: &str) -> Text<'static> {
    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(
        entry
            .title
            .clone()
            .unwrap_or_else(|| "(untitled)".to_string())
            .bold(),
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
    for line in html_to_text(body).lines() {
        lines.push(Line::from(line.to_string()));
    }
    Text::from(lines)
}

/// Convert entry HTML to plain text for the reader: drop tags, turn block-level
/// tags into line breaks, decode common entities, and **strip control
/// characters** so a hostile feed can't inject terminal escape sequences.
fn html_to_text(html: &str) -> String {
    let mut text = String::with_capacity(html.len());
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
            if is_block_tag(&tag) {
                text.push('\n');
            }
        } else {
            text.push(c);
        }
    }
    sanitize(&decode_entities(&text))
}

fn is_block_tag(tag: &str) -> bool {
    let name = tag
        .trim_start_matches('/')
        .split([' ', '\t', '\n', '/'])
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    matches!(
        name.as_str(),
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

fn run_loop(
    terminal: &mut DefaultTerminal,
    handle: &Handle,
    client: &Client,
    tx: &UnboundedSender<Msg>,
    rx: &mut mpsc::UnboundedReceiver<Msg>,
    browser_pref: &BrowserPref,
) -> Result<()> {
    let mut app = App::new();
    while !app.should_quit {
        while let Ok(msg) = rx.try_recv() {
            app.apply(msg);
        }
        terminal
            .draw(|frame| app.draw(frame))
            .context("drawing the UI")?;

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
}
