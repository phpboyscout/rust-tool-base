//! Minimal example — the smallest real tool built on `rtb`.
//!
//! Run as:
//!
//! ```console
//! $ cargo run -p rtb-example-minimal -- version
//! $ cargo run -p rtb-example-minimal -- greet
//! $ cargo run -p rtb-example-minimal -- doctor
//! $ cargo run -p rtb-example-minimal -- credentials list
//! $ cargo run -p rtb-example-minimal -- telemetry status
//! $ cargo run -p rtb-example-minimal -- config show
//! ```
//!
//! This exercises the v0.1–v0.4 built-in commands plus a custom
//! `greet` command registered via the `linkme` distributed slice
//! pattern. The v0.4 `credentials` subtree is wired against a tiny
//! `MyConfig` struct that declares two credential refs (anthropic,
//! github), demonstrating the
//! `Application::builder().credentials_from(...)` opt-in.

use std::sync::Arc;

use async_trait::async_trait;
use linkme::distributed_slice;
use rtb::core::app::App;
use rtb::core::command::{Command, CommandSpec, BUILTIN_COMMANDS};
use rtb::prelude::*;

// =====================================================================
// Sample tool config — declares two credentials so `credentials list`
// has something to show.
// =====================================================================

/// Tool config. In a real downstream tool this is the
/// `serde::Deserialize` struct passed as `Config<MyConfig>`; here
/// we just declare its credentials directly.
#[derive(Default, Clone)]
struct MyConfig {
    anthropic: CredentialRef,
    github: CredentialRef,
}

impl CredentialBearing for MyConfig {
    fn credentials(&self) -> Vec<(&'static str, &CredentialRef)> {
        vec![("anthropic", &self.anthropic), ("github", &self.github)]
    }
}

fn sample_config() -> MyConfig {
    MyConfig {
        anthropic: CredentialRef {
            env: Some("MINIMAL_ANTHROPIC_API_KEY".to_string()),
            fallback_env: Some("ANTHROPIC_API_KEY".to_string()),
            ..Default::default()
        },
        github: CredentialRef {
            env: Some("MINIMAL_GITHUB_TOKEN".to_string()),
            fallback_env: Some("GITHUB_TOKEN".to_string()),
            ..Default::default()
        },
    }
}

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
                .telemetry_notice(
                    "`minimal` collects no telemetry — this notice is for demo wiring only.",
                )
                .build(),
        )
        .version(VersionInfo::from_env())
        .credentials_from(Arc::new(sample_config()))
        .build()?
        .run()
        .await
}
