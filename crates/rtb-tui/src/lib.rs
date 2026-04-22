// TODO: remove when this crate ships v0.1 — docs are added alongside implementation.
#![allow(missing_docs)]

//! Reusable TUI widgets.
//!
//! * `Wizard` — multi-step form built on `inquire`. Escape returns
//!   `InquireError::OperationCanceled`, which the wizard interprets as a
//!   back-navigation and re-prompts the previous step.
//! * Table helpers around `tabled` for dual text/JSON output.
