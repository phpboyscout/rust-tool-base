//! CLI application scaffolding.
//!
//! # Entry point
//!
//! Downstream tools use [`Application::builder`] to wire their
//! metadata, version info, optional assets + features, and the
//! framework installs:
//!
//! * a `tracing-subscriber` registry (pretty fmt on TTY, JSON
//!   otherwise or when `--log-format json` is set),
//! * the `miette` diagnostic + panic hooks (via [`rtb_error::hook`]),
//! * a [`tokio_util::sync::CancellationToken`] bound to `SIGINT` and
//!   Unix `SIGTERM`,
//! * clap-based command parsing with built-in subcommands filtered by
//!   the runtime [`rtb_app::features::Features`] set.
//!
//! ```ignore
//! use rtb_cli::prelude::*;
//!
//! #[tokio::main]
//! async fn main() -> miette::Result<()> {
//!     Application::builder()
//!         .metadata(ToolMetadata::builder().name("mytool").summary("a tool").build())
//!         .version(VersionInfo::from_env())
//!         .build()?
//!         .run()
//!         .await
//! }
//! ```
//!
//! See `docs/development/specs/2026-04-22-rtb-cli-v0.1.md` for the
//! authoritative contract.

// `deny` (not `forbid`) so submodules registering into
// `linkme::distributed_slice` (the `credentials` and similar
// subtrees) can `#![allow(unsafe_code)]` for the
// `#[link_section]` attribute the macro emits. No hand-rolled
// `unsafe` blocks exist in this crate.
#![deny(unsafe_code)]

pub mod application;
pub mod builtins;
pub mod credentials;
pub mod health;
pub mod init;
pub mod render;
pub mod runtime;
pub mod telemetry;

pub use application::{Application, ApplicationBuilder};
pub use health::{HealthCheck, HealthReport, HealthStatus};
pub use init::Initialiser;
pub use render::{output, OutputMode};

/// Glob-importable convenience prelude for downstream `fn main()`.
pub mod prelude {
    pub use crate::application::Application;
    pub use rtb_app::prelude::*;
    pub use rtb_error::{Error as RtbError, Result as RtbResult};
}
