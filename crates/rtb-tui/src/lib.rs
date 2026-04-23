//! Reusable TUI widgets.
//!
//! * `Wizard` — multi-step form built on `inquire`. Escape returns
//!   `InquireError::OperationCanceled`, which the wizard interprets as a
//!   back-navigation and re-prompts the previous step.
//! * Table helpers around `tabled` for dual text/JSON output.
//!
//! **Status:** stub awaiting its real v0.1 spec + implementation.
//! Target milestone is **v0.4**; see the framework spec's Roadmap
//! (§16) in `docs/development/specs/rust-tool-base.md`.

// Stub crate — remove `#![allow(missing_docs)]` when the real surface
// is documented. See the framework spec Roadmap for the target version.
#![allow(missing_docs)]
