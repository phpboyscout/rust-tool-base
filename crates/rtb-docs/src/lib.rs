//! Interactive TUI docs browser.
//!
//! Uses `ratatui` for the two-pane layout and `termimad` for markdown
//! rendering. AI-powered Q&A (`docs ask`) is gated on the `ai` Cargo feature
//! and, when enabled, streams responses from `rtb-ai` into the viewport via
//! a `tokio::sync::mpsc` channel.
