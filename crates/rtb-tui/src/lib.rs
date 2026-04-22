//! Reusable TUI widgets.
//!
//! * `Wizard` — multi-step form built on `inquire`. Escape returns
//!   `InquireError::OperationCanceled`, which the wizard interprets as a
//!   back-navigation and re-prompts the previous step.
//! * Table helpers around `tabled` for dual text/JSON output.
