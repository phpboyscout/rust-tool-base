//! The [`Command`] trait, its descriptor, and the link-time registration slice.
//!
//! A `Command` is an opinionated `async fn(App) -> Result<()>` bundled
//! with a small static descriptor ([`CommandSpec`]). Commands self-
//! register via a [`linkme`] distributed slice so no manual wiring is
//! needed when authoring new commands.
//!
//! # Registration pattern
//!
//! ```ignore
//! use rtb_core::command::{BUILTIN_COMMANDS, Command, CommandSpec};
//! use rtb_core::linkme::distributed_slice;
//!
//! pub struct Deploy;
//!
//! #[async_trait::async_trait]
//! impl Command for Deploy {
//!     fn spec(&self) -> &CommandSpec {
//!         static SPEC: CommandSpec = CommandSpec {
//!             name: "deploy",
//!             about: "Deploy the thing",
//!             aliases: &[],
//!             feature: None,
//!         };
//!         &SPEC
//!     }
//!
//!     async fn run(&self, _app: rtb_core::app::App) -> miette::Result<()> {
//!         Ok(())
//!     }
//! }
//!
//! #[distributed_slice(BUILTIN_COMMANDS)]
//! fn __register_deploy() -> Box<dyn Command> { Box::new(Deploy) }
//! ```
//!
//! `rtb-cli::Application::run` iterates `BUILTIN_COMMANDS` at startup,
//! filters by the runtime `Features` set, and registers each remaining
//! command with clap.

use async_trait::async_trait;
use linkme::distributed_slice;

use crate::app::App;
use crate::features::Feature;

/// Static descriptor of a [`Command`].
///
/// Every field is `'static` because commands are compile-time entities —
/// runtime-generated subcommands are a separate (unimplemented) concern.
#[derive(Debug, Clone)]
pub struct CommandSpec {
    /// The subcommand name as it appears on the CLI (`mytool deploy`).
    pub name: &'static str,

    /// One-line summary shown in `--help`.
    pub about: &'static str,

    /// Alternative names accepted on the CLI. Displayed in help text.
    pub aliases: &'static [&'static str],

    /// If `Some`, the command is only visible when the runtime
    /// [`Features`](crate::features::Features) set has this feature
    /// enabled. Unconditional commands leave this `None`.
    pub feature: Option<Feature>,
}

/// The contract every CLI subcommand implements.
///
/// Implementations are typically registered via the
/// [`BUILTIN_COMMANDS`] distributed slice. `rtb-cli` provides a
/// `#[rtb::command]` attribute macro that derives the boilerplate for
/// downstream tools; hand-written impls follow the example in the
/// module docs.
#[async_trait]
pub trait Command: Send + Sync + 'static {
    /// The command's static descriptor.
    fn spec(&self) -> &CommandSpec;

    /// Execute the command. `app` is taken by value — `Clone` on `App`
    /// is O(1) so subcommands that fan out can `.clone()` freely.
    async fn run(&self, app: App) -> miette::Result<()>;
}

/// Link-time registry of [`Command`] factory functions.
///
/// The factories are thin — each produces a fresh `Box<dyn Command>`
/// when invoked by `rtb-cli::Application`. They are expected to be
/// cheap (no I/O, no allocation beyond the box). Heavy work belongs in
/// `Command::run`.
///
/// See the module docs for the registration pattern.
#[distributed_slice]
pub static BUILTIN_COMMANDS: [fn() -> Box<dyn Command>];
