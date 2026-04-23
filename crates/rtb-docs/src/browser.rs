//! `DocsBrowser` — two-pane ratatui TUI over an embedded markdown
//! tree.
//!
//! v0.1 ships the state machine + render contract but stops short of
//! a full event loop. The loop wiring (`crossterm::event::read` +
//! terminal mode management) lands in the v0.2.x CLI dispatch
//! follow-up; for now consumers use [`DocsBrowser::render`] to draw
//! a frame and [`DocsBrowser::handle_key`] to mutate state so tests
//! and future callers can drive the browser programmatically.

use std::collections::HashMap;

use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Widget, Wrap};

use crate::error::Result;
use crate::index::Index;
use crate::render;
use crate::search::SearchIndex;

/// Which pane currently has focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    /// Left (index) pane.
    Index,
    /// Right (content) pane.
    Content,
}

/// The input mode — normal navigation or search.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    /// Arrow keys move the selection; `Enter` opens the selected page.
    Normal,
    /// Typing filters the index by fuzzy title match.
    Search(String),
}

/// Browser state. Hold it across frames; pass it to [`render`] + the
/// key-event handler.
pub struct DocsBrowser {
    index: Index,
    pages: HashMap<String, String>,
    search: SearchIndex,
    list_state: ListState,
    pub(crate) focus: Focus,
    pub(crate) mode: Mode,
    /// Flat list of (section-label, entry) pairs for the left pane.
    flat_entries: Vec<FlatEntry>,
    /// Currently-rendered right-pane body, in markdown form. Empty
    /// when no page is selected.
    current_body: String,
    /// `true` when the user has requested quit.
    quit_requested: bool,
}

struct FlatEntry {
    section: String,
    path: String,
    title: String,
}

impl DocsBrowser {
    /// Build the browser from an already-loaded index + pages map.
    ///
    /// # Errors
    ///
    /// Propagates [`crate::error::DocsError::Search`] from the FTS
    /// index build.
    pub fn new(index: Index, pages: HashMap<String, String>) -> Result<Self> {
        let flat_entries: Vec<FlatEntry> = index
            .entries()
            .map(|(section, entry)| FlatEntry {
                section: section.to_string(),
                path: entry.path.clone(),
                title: entry.title.clone(),
            })
            .collect();
        let search_input: Vec<(String, String, String)> = flat_entries
            .iter()
            .map(|e| {
                (e.path.clone(), e.title.clone(), pages.get(&e.path).cloned().unwrap_or_default())
            })
            .collect();
        let search = SearchIndex::build(&search_input)?;
        let mut list_state = ListState::default();
        if !flat_entries.is_empty() {
            list_state.select(Some(0));
        }
        let current_body =
            flat_entries.first().and_then(|e| pages.get(&e.path)).cloned().unwrap_or_default();
        Ok(Self {
            index,
            pages,
            search,
            list_state,
            focus: Focus::Index,
            mode: Mode::Normal,
            flat_entries,
            current_body,
            quit_requested: false,
        })
    }

    /// `true` once the user has pressed `q`.
    #[must_use]
    pub const fn quit_requested(&self) -> bool {
        self.quit_requested
    }

    /// The currently-selected entry's path, or `None` when the index
    /// is empty.
    #[must_use]
    pub fn selected_path(&self) -> Option<&str> {
        let idx = self.list_state.selected()?;
        self.flat_entries.get(idx).map(|e| e.path.as_str())
    }

    /// Borrow the FTS index — useful for tests that want to exercise
    /// the search side independently of key events.
    #[must_use]
    pub const fn search(&self) -> &SearchIndex {
        &self.search
    }

    /// Advance the selection by `delta` (positive = down, negative =
    /// up). Wraps neither end — out-of-range is clamped.
    pub fn move_selection(&mut self, delta: isize) {
        if self.flat_entries.is_empty() {
            return;
        }
        let current = self.list_state.selected().unwrap_or(0);
        let new = if delta >= 0 {
            current.saturating_add(delta.unsigned_abs()).min(self.flat_entries.len() - 1)
        } else {
            current.saturating_sub(delta.unsigned_abs())
        };
        self.list_state.select(Some(new));
    }

    /// Open the currently-selected page into the right pane.
    pub fn open_selected(&mut self) {
        if let Some(path) = self.selected_path() {
            self.current_body = self.pages.get(path).cloned().unwrap_or_default();
        }
    }

    /// Request quit — callers exit their event loop on the next
    /// iteration.
    pub const fn request_quit(&mut self) {
        self.quit_requested = true;
    }

    /// Key-event entry point. Pass the crossterm `KeyEvent`'s `code`.
    /// Abstracted away from `crossterm::event::KeyCode` so unit tests
    /// don't need the crossterm types in scope.
    pub fn handle_key(&mut self, key: KeyCode) {
        match (&self.mode, key) {
            (Mode::Normal, KeyCode::Char('q')) => self.request_quit(),
            (Mode::Normal, KeyCode::Char('j') | KeyCode::Down) => self.move_selection(1),
            (Mode::Normal, KeyCode::Char('k') | KeyCode::Up) => self.move_selection(-1),
            (Mode::Normal, KeyCode::Enter) => self.open_selected(),
            (Mode::Normal, KeyCode::Tab) => {
                self.focus = match self.focus {
                    Focus::Index => Focus::Content,
                    Focus::Content => Focus::Index,
                };
            }
            (Mode::Normal, KeyCode::Char('/')) => {
                self.mode = Mode::Search(String::new());
            }
            (Mode::Search(_), KeyCode::Esc) => {
                self.mode = Mode::Normal;
            }
            (Mode::Search(buf), KeyCode::Char(c)) => {
                let mut new = buf.clone();
                new.push(c);
                self.mode = Mode::Search(new);
            }
            (Mode::Search(buf), KeyCode::Backspace) => {
                let mut new = buf.clone();
                new.pop();
                self.mode = Mode::Search(new);
            }
            (Mode::Search(buf), KeyCode::Enter) => {
                // Open the top hit (if any) and exit search mode.
                let top = self.search.title_search(buf).into_iter().next();
                if let Some(hit) = top {
                    if let Some(idx) = self.flat_entries.iter().position(|e| e.path == hit.path) {
                        self.list_state.select(Some(idx));
                        self.open_selected();
                    }
                }
                self.mode = Mode::Normal;
            }
            _ => {}
        }
    }

    /// Render the browser into the given `Rect` on the supplied
    /// `Buffer`. Call once per frame.
    pub fn render(&mut self, area: Rect, buf: &mut Buffer) {
        let horizontal = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
            .split(area);

        self.render_index(horizontal[0], buf);
        self.render_content(horizontal[1], buf);
    }

    fn render_index(&mut self, area: Rect, buf: &mut Buffer) {
        let items: Vec<ListItem<'_>> = self
            .flat_entries
            .iter()
            .map(|entry| {
                ListItem::new(Line::from(vec![
                    Span::styled(
                        format!("{} ", entry.section),
                        Style::default().add_modifier(Modifier::DIM),
                    ),
                    Span::raw(entry.title.clone()),
                ]))
            })
            .collect();
        let border_style = if self.focus == Focus::Index {
            Style::default().add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        let mut list_state = self.list_state;
        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(self.index.title.clone())
                    .border_style(border_style),
            )
            .highlight_symbol("▶ ")
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
        ratatui::widgets::StatefulWidget::render(list, area, buf, &mut list_state);
        // Persist any changes the widget made back to the state.
        self.list_state = list_state;
    }

    fn render_content(&self, area: Rect, buf: &mut Buffer) {
        let body_text: Text<'static> = render::to_ratatui_text(&self.current_body)
            .map(render::text_into_owned)
            .unwrap_or_default();
        let title = if let Mode::Search(query) = &self.mode {
            format!("search: {query}")
        } else {
            self.selected_path().unwrap_or("").to_string()
        };
        let border_style = if self.focus == Focus::Content {
            Style::default().add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        let block = Block::default().borders(Borders::ALL).title(title).border_style(border_style);
        Paragraph::new(body_text).wrap(Wrap { trim: false }).block(block).render(area, buf);
    }
}

/// Key code subset the browser cares about. Kept independent of
/// `crossterm::event::KeyCode` so tests don't pull crossterm in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyCode {
    /// Literal character.
    Char(char),
    /// Up arrow.
    Up,
    /// Down arrow.
    Down,
    /// Enter / Return.
    Enter,
    /// Tab.
    Tab,
    /// Backspace.
    Backspace,
    /// Escape.
    Esc,
}

// ---------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::{IndexEntry, IndexSection};

    fn browser() -> DocsBrowser {
        let index = Index {
            title: "Docs".into(),
            sections: vec![IndexSection {
                title: "Start".into(),
                pages: vec![
                    IndexEntry { path: "a.md".into(), title: "Alpha".into() },
                    IndexEntry { path: "b.md".into(), title: "Beta".into() },
                    IndexEntry { path: "c.md".into(), title: "Gamma".into() },
                ],
            }],
        };
        let mut pages = HashMap::new();
        pages.insert("a.md".into(), "# Alpha\n\nA content".into());
        pages.insert("b.md".into(), "# Beta\n\nB content".into());
        pages.insert("c.md".into(), "# Gamma\n\nC content".into());
        DocsBrowser::new(index, pages).expect("build")
    }

    #[test]
    fn first_entry_selected_by_default() {
        let b = browser();
        assert_eq!(b.selected_path(), Some("a.md"));
    }

    #[test]
    fn down_arrow_advances_selection() {
        let mut b = browser();
        b.handle_key(KeyCode::Down);
        assert_eq!(b.selected_path(), Some("b.md"));
    }

    #[test]
    fn up_arrow_at_top_clamps() {
        let mut b = browser();
        b.handle_key(KeyCode::Up);
        assert_eq!(b.selected_path(), Some("a.md"));
    }

    #[test]
    fn j_and_k_move_selection() {
        let mut b = browser();
        b.handle_key(KeyCode::Char('j'));
        b.handle_key(KeyCode::Char('j'));
        assert_eq!(b.selected_path(), Some("c.md"));
        b.handle_key(KeyCode::Char('k'));
        assert_eq!(b.selected_path(), Some("b.md"));
    }

    #[test]
    fn q_requests_quit() {
        let mut b = browser();
        assert!(!b.quit_requested());
        b.handle_key(KeyCode::Char('q'));
        assert!(b.quit_requested());
    }

    #[test]
    fn tab_toggles_focus() {
        let mut b = browser();
        assert_eq!(b.focus, Focus::Index);
        b.handle_key(KeyCode::Tab);
        assert_eq!(b.focus, Focus::Content);
        b.handle_key(KeyCode::Tab);
        assert_eq!(b.focus, Focus::Index);
    }

    #[test]
    fn slash_enters_search_mode() {
        let mut b = browser();
        b.handle_key(KeyCode::Char('/'));
        assert!(matches!(b.mode, Mode::Search(_)));
        b.handle_key(KeyCode::Esc);
        assert!(matches!(b.mode, Mode::Normal));
    }

    #[test]
    fn search_mode_typing_accumulates_buffer() {
        let mut b = browser();
        b.handle_key(KeyCode::Char('/'));
        b.handle_key(KeyCode::Char('b'));
        b.handle_key(KeyCode::Char('e'));
        match &b.mode {
            Mode::Search(buf) => assert_eq!(buf, "be"),
            other @ Mode::Normal => panic!("expected Search, got {other:?}"),
        }
    }

    #[test]
    fn search_enter_jumps_to_top_hit() {
        let mut b = browser();
        b.handle_key(KeyCode::Char('/'));
        b.handle_key(KeyCode::Char('g'));
        b.handle_key(KeyCode::Char('a'));
        b.handle_key(KeyCode::Char('m'));
        b.handle_key(KeyCode::Enter);
        assert_eq!(b.selected_path(), Some("c.md"));
        assert!(matches!(b.mode, Mode::Normal));
    }

    #[test]
    fn render_fills_the_buffer() {
        use ratatui::buffer::Buffer;
        let mut b = browser();
        let area = Rect { x: 0, y: 0, width: 80, height: 24 };
        let mut buf = Buffer::empty(area);
        b.render(area, &mut buf);
        // Smoke: no panic. Detailed rendering assertions live in a
        // follow-up PR using TestBackend once the extras layer lands.
    }
}
