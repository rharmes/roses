//! Full-screen terminal UI (TASK-6).
//!
//! An immediate-mode ratatui app on the crossterm backend: a two-pane
//! list/detail layout (unread entries on the left, a scrollable reader on the
//! right). Feedbin is queried on a background `tokio::spawn_blocking` task so
//! input never blocks (the blocking client from `feedbin` is reused as-is); the
//! result arrives over a channel that the draw loop drains each tick.

use std::collections::HashMap;
use std::time::Duration;

use anyhow::{Context, Result};
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::{DefaultTerminal, Frame};
use tokio::runtime::Handle;
use tokio::sync::mpsc::{self, UnboundedSender};

use crate::feedbin::{Client, Entry};

/// How many of the newest unread entries to load into the list.
const DISPLAY_LIMIT: usize = 50;
/// Lines the reader scrolls per page key.
const READER_PAGE: u16 = 10;
/// How long to wait for input before redrawing (also bounds how quickly a
/// finished background fetch shows up).
const TICK: Duration = Duration::from_millis(100);

/// A fully-loaded snapshot from Feedbin.
struct Loaded {
    entries: Vec<Entry>,
    feed_titles: HashMap<i64, String>,
    total_unread: usize,
}

/// Message from the background fetch worker to the UI loop.
enum Msg {
    Loaded(Result<Loaded, String>),
}

/// What a keypress asks the run loop to do beyond mutating `App`.
enum Action {
    None,
    Reload,
}

enum Status {
    Loading,
    Ready,
    Failed(String),
}

struct App {
    status: Status,
    entries: Vec<Entry>,
    feed_titles: HashMap<i64, String>,
    total_unread: usize,
    list_state: ListState,
    reader_scroll: u16,
    should_quit: bool,
}

impl App {
    fn new() -> Self {
        Self {
            status: Status::Loading,
            entries: Vec::new(),
            feed_titles: HashMap::new(),
            total_unread: 0,
            list_state: ListState::default(),
            reader_scroll: 0,
            should_quit: false,
        }
    }

    fn apply(&mut self, msg: Msg) {
        match msg {
            Msg::Loaded(Ok(loaded)) => {
                self.entries = loaded.entries;
                self.feed_titles = loaded.feed_titles;
                self.total_unread = loaded.total_unread;
                self.status = Status::Ready;
                self.list_state
                    .select((!self.entries.is_empty()).then_some(0));
                self.reader_scroll = 0;
            }
            Msg::Loaded(Err(err)) => self.status = Status::Failed(err),
        }
    }

    fn feed_name(&self, feed_id: i64) -> &str {
        self.feed_titles
            .get(&feed_id)
            .map(String::as_str)
            .unwrap_or("(unknown feed)")
    }

    fn selected_entry(&self) -> Option<&Entry> {
        self.list_state.selected().and_then(|i| self.entries.get(i))
    }

    fn select_next(&mut self) {
        if self.entries.is_empty() {
            return;
        }
        let i = self.list_state.selected().unwrap_or(0);
        self.list_state
            .select(Some((i + 1).min(self.entries.len() - 1)));
        self.reader_scroll = 0;
    }

    fn select_prev(&mut self) {
        if self.entries.is_empty() {
            return;
        }
        let i = self.list_state.selected().unwrap_or(0);
        self.list_state.select(Some(i.saturating_sub(1)));
        self.reader_scroll = 0;
    }

    fn select_first(&mut self) {
        if !self.entries.is_empty() {
            self.list_state.select(Some(0));
            self.reader_scroll = 0;
        }
    }

    fn select_last(&mut self) {
        if !self.entries.is_empty() {
            self.list_state.select(Some(self.entries.len() - 1));
            self.reader_scroll = 0;
        }
    }

    /// Handle one key press; returns whether a reload was requested.
    fn handle_key(&mut self, code: KeyCode) -> Action {
        match code {
            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
            KeyCode::Down | KeyCode::Char('j') => self.select_next(),
            KeyCode::Up | KeyCode::Char('k') => self.select_prev(),
            KeyCode::Char('g') | KeyCode::Home => self.select_first(),
            KeyCode::Char('G') | KeyCode::End => self.select_last(),
            KeyCode::PageDown | KeyCode::Char('d') | KeyCode::Char(' ') => {
                self.reader_scroll = self.reader_scroll.saturating_add(READER_PAGE);
            }
            KeyCode::PageUp | KeyCode::Char('u') => {
                self.reader_scroll = self.reader_scroll.saturating_sub(READER_PAGE);
            }
            KeyCode::Char('r') => return Action::Reload,
            _ => {}
        }
        Action::None
    }

    fn draw(&mut self, frame: &mut Frame) {
        let [main, footer] =
            Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(frame.area());
        let [list_area, reader_area] =
            Layout::horizontal([Constraint::Percentage(40), Constraint::Percentage(60)])
                .areas(main);

        self.draw_list(frame, list_area);
        self.draw_reader(frame, reader_area);

        let help = Line::from(" ↑/↓ select · u/d scroll · r reload · q quit ").dim();
        frame.render_widget(help, footer);
    }

    fn draw_list(&mut self, frame: &mut Frame, area: ratatui::layout::Rect) {
        // Show old entries while a refresh is in flight, rather than blanking.
        if self.entries.is_empty() {
            let message = match &self.status {
                Status::Loading => "Loading unread entries…".to_string(),
                Status::Ready => "You're all caught up — no unread entries.".to_string(),
                Status::Failed(err) => format!("Failed to load: {err}"),
            };
            let block = Block::bordered().title("Entries");
            frame.render_widget(
                Paragraph::new(message)
                    .block(block)
                    .wrap(Wrap { trim: true }),
                area,
            );
            return;
        }

        let title = match self.status {
            Status::Loading => format!(
                "Unread {}/{} (refreshing…)",
                self.entries.len(),
                self.total_unread
            ),
            _ => format!("Unread {}/{}", self.entries.len(), self.total_unread),
        };
        let items: Vec<ListItem> = self
            .entries
            .iter()
            .map(|entry| {
                let heading = entry.title.as_deref().unwrap_or("(untitled)");
                let feed = self.feed_name(entry.feed_id);
                ListItem::new(Line::from(vec![
                    Span::raw(heading.to_string()),
                    Span::raw("  "),
                    Span::styled(format!("· {feed}"), Style::new().dim()),
                ]))
            })
            .collect();
        let list = List::new(items)
            .block(Block::bordered().title(title))
            .highlight_style(Style::new().reversed())
            .highlight_symbol("▶ ");
        frame.render_stateful_widget(list, area, &mut self.list_state);
    }

    fn draw_reader(&mut self, frame: &mut Frame, area: ratatui::layout::Rect) {
        let block = Block::bordered().title("Reader");
        let Some(entry) = self.selected_entry() else {
            frame.render_widget(Paragraph::new("").block(block), area);
            return;
        };

        let text = reader_text(entry, self.feed_name(entry.feed_id));

        // Clamp the scroll so you can't page past the end (approximate: counts
        // unwrapped lines, which is fine as a soft bound).
        let visible = area.height.saturating_sub(2);
        let max_scroll = (text.lines.len() as u16).saturating_sub(visible);
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

/// Blocking fetch of the newest unread entries plus their feed names. Runs on
/// tokio's blocking pool via [`spawn_fetch`].
fn load(client: &Client) -> Result<Loaded> {
    let mut unread = client.unread_entry_ids()?;
    let total_unread = unread.len();
    // Feedbin IDs grow over time; newest first, then keep a readable sample.
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

    let (tx, mut rx) = mpsc::unbounded_channel::<Msg>();
    spawn_fetch(&handle, client.clone(), tx.clone());

    let mut terminal = ratatui::init();
    let result = run_loop(&mut terminal, &handle, &client, &tx, &mut rx);
    ratatui::restore();
    result
}

fn run_loop(
    terminal: &mut DefaultTerminal,
    handle: &Handle,
    client: &Client,
    tx: &UnboundedSender<Msg>,
    rx: &mut mpsc::UnboundedReceiver<Msg>,
) -> Result<()> {
    let mut app = App::new();
    while !app.should_quit {
        while let Ok(msg) = rx.try_recv() {
            app.apply(msg);
        }
        terminal
            .draw(|frame| app.draw(frame))
            .context("drawing the UI")?;

        // `handle_key` is evaluated for every key press (for its nav/quit side
        // effects); the body runs only when it asks for a reload.
        if event::poll(TICK).context("polling for input")?
            && let Event::Key(key) = event::read().context("reading input")?
            && key.kind == KeyEventKind::Press
            && let Action::Reload = app.handle_key(key.code)
        {
            app.status = Status::Loading;
            spawn_fetch(handle, client.clone(), tx.clone());
        }
    }
    Ok(())
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

    fn ready_app(n: usize) -> App {
        let mut feed_titles = HashMap::new();
        feed_titles.insert(7, "Rust Blog".to_string());
        let entries = (0..n)
            .map(|i| entry(i as i64, 7, &format!("Headline {i}"), Some("<p>Body</p>")))
            .collect();
        let mut app = App::new();
        app.apply(Msg::Loaded(Ok(Loaded {
            entries,
            feed_titles,
            total_unread: n,
        })));
        app
    }

    #[test]
    fn html_to_text_strips_tags_and_breaks_paragraphs() {
        // `<br>` is a single line break; a `</p><p>` boundary becomes a blank line.
        assert_eq!(html_to_text("a<br>b"), "a\nb");
        assert_eq!(
            html_to_text("<p>One <b>bold</b></p><p>Two</p>"),
            "One bold\n\nTwo"
        );
    }

    #[test]
    fn html_to_text_decodes_entities() {
        let out = html_to_text("Tom &amp; Jerry &lt;3 &#39;hi&#39; &#x2764; &nbsp;end");
        assert_eq!(out, "Tom & Jerry <3 'hi' ❤  end");
    }

    #[test]
    fn html_to_text_strips_control_chars_blocking_escape_injection() {
        // A feed trying to smuggle an ANSI color escape: the ESC byte is dropped.
        let out = html_to_text("safe\u{1b}[31mtext");
        assert!(
            !out.contains('\u{1b}'),
            "escape byte must be stripped: {out:?}"
        );
        assert!(out.contains("safe"));
    }

    #[test]
    fn selection_moves_and_clamps() {
        let mut app = ready_app(3);
        assert_eq!(app.list_state.selected(), Some(0));
        app.select_prev(); // clamps at top
        assert_eq!(app.list_state.selected(), Some(0));
        app.select_next();
        app.select_next();
        app.select_next(); // clamps at bottom (len 3)
        assert_eq!(app.list_state.selected(), Some(2));
        app.select_first();
        assert_eq!(app.list_state.selected(), Some(0));
        app.select_last();
        assert_eq!(app.list_state.selected(), Some(2));
    }

    #[test]
    fn moving_selection_resets_reader_scroll() {
        let mut app = ready_app(3);
        app.reader_scroll = 5;
        app.select_next();
        assert_eq!(app.reader_scroll, 0);
    }

    #[test]
    fn quit_key_sets_should_quit() {
        let mut app = ready_app(1);
        let _ = app.handle_key(KeyCode::Char('q'));
        assert!(app.should_quit);
    }

    #[test]
    fn reload_key_requests_reload() {
        let mut app = ready_app(1);
        assert!(matches!(app.handle_key(KeyCode::Char('r')), Action::Reload));
    }

    #[test]
    fn renders_two_pane_layout_when_ready() {
        let mut app = ready_app(2);
        let backend = ratatui::backend::TestBackend::new(80, 20);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|frame| app.draw(frame)).unwrap();

        let rendered: String = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(ratatui::buffer::Cell::symbol)
            .collect();

        assert!(rendered.contains("Headline 0"), "list shows entry titles");
        assert!(rendered.contains("Rust Blog"), "list shows feed names");
        assert!(rendered.contains("Reader"), "reader pane is present");
        assert!(
            rendered.contains("Body"),
            "reader shows the selected entry body"
        );
        assert!(rendered.contains("quit"), "footer shows key help");
    }

    #[test]
    fn empty_unread_renders_caught_up_message() {
        let mut app = App::new();
        app.apply(Msg::Loaded(Ok(Loaded {
            entries: Vec::new(),
            feed_titles: HashMap::new(),
            total_unread: 0,
        })));
        let backend = ratatui::backend::TestBackend::new(80, 20);
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
            rendered.contains("caught up"),
            "shows the friendly empty state"
        );
    }
}
