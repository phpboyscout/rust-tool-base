//! The `update` CLI subcommand.
//!
//! Registers into [`rtb_app::command::BUILTIN_COMMANDS`] via
//! `linkme::distributed_slice`. Downstream tools that depend on
//! `rtb-update` get the command automatically; the stub in `rtb-cli`
//! has been removed and `Application::build` deduplicates by
//! command name so the real one always wins if somehow both linked.
//!
//! # Lint exception
//!
//! This module allows `unsafe_code` because
//! `linkme::distributed_slice`'s expansion emits a `#[link_section]`
//! attribute that Rust 1.95+ flags under the `unsafe_code` lint. No
//! hand-rolled `unsafe` blocks exist in this module. Same exception
//! rationale as `rtb-vcs`'s backend modules.

#![allow(unsafe_code)]

use async_trait::async_trait;
use linkme::distributed_slice;
use rtb_app::app::App;
use rtb_app::command::{Command, CommandSpec, BUILTIN_COMMANDS};
use rtb_app::features::Feature;

/// The `update` subcommand.
///
/// Per spec § 2.4, a thin shim — the real work lives in
/// [`crate::Updater`]. This struct exists to satisfy the `Command`
/// trait and provide the `spec()` metadata; the full clap dispatch +
/// flag parsing + JSON output layer ships in a v0.2.x follow-up
/// (needs the `rtb-cli` command-authoring ergonomics work which is
/// also targeted v0.2.x).
pub struct UpdateCmd;

#[async_trait]
impl Command for UpdateCmd {
    fn spec(&self) -> &CommandSpec {
        static SPEC: CommandSpec = CommandSpec {
            name: "update",
            about: "Update the tool to the latest available version",
            aliases: &[],
            feature: Some(Feature::Update),
        };
        &SPEC
    }

    async fn run(&self, _app: App) -> miette::Result<()> {
        println!("update: programmatic API available via `rtb_update::Updater`");
        println!("       (CLI dispatch layer ships in v0.2.x follow-up)");
        Ok(())
    }
}

#[distributed_slice(BUILTIN_COMMANDS)]
fn __register_update() -> Box<dyn Command> {
    Box::new(UpdateCmd)
}
