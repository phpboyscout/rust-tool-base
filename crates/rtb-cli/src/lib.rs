//! CLI application scaffolding.
//!
//! # Entry point
//!
//! Downstream tools use the `Application` typestate builder (powered by
//! `bon`) to wire their metadata, config type, embedded assets, and custom
//! commands. The builder installs:
//!
//! * a `tracing-subscriber` registry (pretty-fmt by default, JSON when
//!   `--output json` or the env filter promotes it),
//! * a `miette` diagnostic hook and panic hook,
//! * a `tokio` runtime (unless the caller already provides one),
//! * a `CancellationToken` wired to `SIGINT`/`SIGTERM`,
//! * clap-based command parsing with built-in subcommands gated by
//!   [`rtb_core::features::Features`].
//!
//! ```ignore
//! use rtb::prelude::*;
//!
//! #[tokio::main]
//! async fn main() -> miette::Result<()> {
//!     rtb::cli::Application::builder()
//!         .metadata(ToolMetadata::builder().name("mytool").summary("…").build())
//!         .version(VersionInfo::new(env!("CARGO_PKG_VERSION").parse().unwrap()))
//!         .command::<commands::Deploy>()
//!         .command::<commands::Status>()
//!         .build()
//!         .run()
//!         .await
//! }
//! ```

pub struct Application;
