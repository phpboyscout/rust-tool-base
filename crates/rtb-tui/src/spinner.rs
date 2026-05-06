//! [`Spinner`] — TTY-aware progress indicator.

use console::Term;

/// TTY-aware spinner.
///
/// When stderr isn't a terminal, every method on this type is a
/// no-op — important for CI logs and tools running behind an
/// MCP-stdio transport (where stderr is captured by the MCP client
/// and arbitrary control sequences would corrupt logs).
///
/// The spinner is single-threaded by design: there is no internal
/// `tokio::task::spawn` that animates frames. Callers tick the
/// spinner manually via [`Self::set_message`] between awaits — see
/// W3 in the spec for the rationale.
pub struct Spinner {
    /// `None` when stderr isn't a TTY — every method short-circuits.
    term: Option<Term>,
    /// Current message; tracked so [`Self::finish`] can clear the
    /// previously-rendered line.
    message: String,
    /// Whether [`Self::finish`] has already run; the `Drop` impl
    /// short-circuits when this is set.
    finished: bool,
}

impl Spinner {
    /// Construct a spinner with `msg` and emit the initial frame.
    /// When stderr isn't a TTY, this constructs the no-op variant
    /// and writes nothing.
    #[must_use]
    pub fn new(msg: impl Into<String>) -> Self {
        let term = Term::stderr();
        let active = term.is_term().then_some(term);
        let message = msg.into();
        let s = Self { term: active, message, finished: false };
        s.draw();
        s
    }

    /// Update the message and redraw. No-op when not a TTY.
    pub fn set_message(&mut self, msg: impl Into<String>) {
        self.message = msg.into();
        self.draw();
    }

    /// Stop spinning and clear the rendered line. Idempotent —
    /// safe to call multiple times. The `Drop` impl invokes this
    /// automatically.
    pub fn finish(mut self) {
        self.finish_inner();
    }

    fn finish_inner(&mut self) {
        if self.finished {
            return;
        }
        self.finished = true;
        if let Some(term) = &self.term {
            // Clear the current line so subsequent stderr output
            // starts at column 0.
            let _ = term.clear_line();
        }
    }

    fn draw(&self) {
        if let Some(term) = &self.term {
            // `\r` + clear-line + write — keeps the spinner pinned
            // to the same line across redraws.
            let _ = term.clear_line();
            let _ = term.write_str(&format!("⠋ {}", self.message));
        }
    }
}

impl Drop for Spinner {
    fn drop(&mut self) {
        self.finish_inner();
    }
}
