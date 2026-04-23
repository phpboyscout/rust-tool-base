//! `docs` CLI command — discoverability shim at v0.1.
//!
//! The full clap dispatch for `browse` / `serve` / `list` / `show` /
//! `ask` subcommands lands in the v0.2.x follow-up alongside
//! `rtb-cli`'s command-authoring-ergonomics work. The library API
//! ([`crate::DocsBrowser`], [`crate::DocsServer`]) is already
//! complete; this shim just makes the command discoverable via
//! `--help`.
//!
//! # Lint exception
//!
//! `linkme::distributed_slice` emits `#[link_section]` which Rust
//! 1.95+ flags under `unsafe_code`. Allowed at module level with no
//! hand-rolled `unsafe` blocks anywhere in the module.

#![allow(unsafe_code)]

use async_trait::async_trait;
use linkme::distributed_slice;
use rtb_app::app::App;
use rtb_app::command::{Command, CommandSpec, BUILTIN_COMMANDS};
use rtb_app::features::Feature;

/// The `docs` subcommand shim.
pub struct DocsCmd;

#[async_trait]
impl Command for DocsCmd {
    fn spec(&self) -> &CommandSpec {
        static SPEC: CommandSpec = CommandSpec {
            name: "docs",
            about: "Browse the embedded documentation",
            aliases: &[],
            feature: Some(Feature::Docs),
        };
        &SPEC
    }

    async fn run(&self, _app: App) -> miette::Result<()> {
        println!("docs: programmatic API available via `rtb_docs::DocsBrowser`");
        println!("       and `rtb_docs::DocsServer`");
        println!("       (CLI dispatch layer ships in v0.2.x follow-up)");
        Ok(())
    }
}

#[distributed_slice(BUILTIN_COMMANDS)]
fn __register_docs() -> Box<dyn Command> {
    Box::new(DocsCmd)
}
