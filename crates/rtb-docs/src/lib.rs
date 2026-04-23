//! Interactive TUI docs browser.
//!
//! Uses `ratatui` for the two-pane layout and `termimad` for markdown
//! rendering. AI-powered Q&A (`docs ask`) is gated on the `ai` Cargo feature
//! and, when enabled, streams responses from `rtb-ai` into the viewport via
//! a `tokio::sync::mpsc` channel.
//!
//! **Status:** stub awaiting its real v0.1 spec + implementation.
//! Target milestone is **v0.2**; see the framework spec's Roadmap
//! (§16) in `docs/development/specs/rust-tool-base.md`.

// Stub crate — remove `#![allow(missing_docs)]` when the real surface
// is documented. See the framework spec Roadmap for the target version.
#![allow(missing_docs)]
