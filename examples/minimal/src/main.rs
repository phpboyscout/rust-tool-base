//! Minimal example — the smallest real tool built on `rtb`.
//!
//! Run as:
//!
//! ```console
//! $ cargo run -p rtb-example-minimal -- version
//! $ cargo run -p rtb-example-minimal -- greet
//! $ cargo run -p rtb-example-minimal -- greet --name Alice
//! $ cargo run -p rtb-example-minimal -- doctor
//! ```
//!
//! This exercises the four built-in commands that ship with
//! `rtb-cli` v0.1 (`version`, `doctor`, `init`, `config`) plus a
//! custom `greet` command registered via the `linkme` distributed
//! slice pattern.

use async_trait::async_trait;
use linkme::distributed_slice;
use rtb::core::app::App;
use rtb::core::command::{Command, CommandSpec, BUILTIN_COMMANDS};
use rtb::prelude::*;

// =====================================================================
// A trivial custom command.
// =====================================================================

/// Prints "hello, {name}". Demonstrates a hand-written `Command`
/// implementation registered into the framework's `BUILTIN_COMMANDS`
/// slice via `linkme`.
struct GreetCmd;

#[async_trait]
impl Command for GreetCmd {
    fn spec(&self) -> &CommandSpec {
        static SPEC: CommandSpec = CommandSpec {
            name: "greet",
            about: "Print a friendly greeting",
            aliases: &["hi"],
            feature: None, // always visible (no runtime Feature gate)
        };
        &SPEC
    }

    async fn run(&self, app: App) -> miette::Result<()> {
        println!("hello from {} v{}", app.metadata.name, app.version.version);
        Ok(())
    }
}

#[distributed_slice(BUILTIN_COMMANDS)]
fn __register_greet() -> Box<dyn Command> {
    Box::new(GreetCmd)
}

// =====================================================================
// Entry point.
// =====================================================================

#[tokio::main]
async fn main() -> miette::Result<()> {
    rtb::cli::Application::builder()
        .metadata(
            ToolMetadata::builder()
                .name("minimal")
                .summary("the smallest rtb-powered CLI")
                .description("A one-command reference tool demonstrating the rtb framework.")
                .build(),
        )
        .version(VersionInfo::from_env())
        .build()?
        .run()
        .await
}
