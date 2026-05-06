//! Reusable TUI building blocks for RTB-built CLI tools.
//!
//! Three pieces:
//!
//! - [`Wizard`] / [`WizardStep`] — multi-step interactive form with
//!   escape-to-back navigation, backed by [`inquire`].
//! - [`render_table`] / [`render_json`] — uniform structured-output
//!   helpers used by the v0.4 `rtb-cli` ops subtrees behind a global
//!   `--output text|json` flag.
//! - [`Spinner`] — TTY-aware progress indicator that no-ops when
//!   stderr isn't a terminal (CI logs, MCP transports).
//!
//! See `docs/development/specs/2026-05-06-rtb-tui-v0.1.md` for the
//! authoritative contract.

#![forbid(unsafe_code)]

mod error;
mod render;
mod spinner;
mod wizard;

pub use error::{RenderError, WizardError};
pub use render::{render_json, render_table};
pub use spinner::Spinner;
pub use wizard::{StepOutcome, Wizard, WizardBuilder, WizardStep};

/// Re-export so downstream consumers (and `WizardStep` impls) can
/// `?`-propagate without adding `inquire` as a direct dependency.
pub use inquire::InquireError;
