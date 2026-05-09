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
//! $ cargo run -p rtb-example-minimal -- config schema
//! ```
//!
//! This exercises the v0.1–v0.4 built-in commands plus a custom
//! `greet` command registered via the `linkme` distributed slice
//! pattern. The v0.4 `credentials` subtree is wired against a tiny
//! `MyCredentials` struct that declares two credential refs
//! (anthropic, github), demonstrating the
//! `Application::builder().credentials_from(...)` opt-in. The v0.4.1
//! `App<C>` typed-config integration is wired via
//! `Application::builder().config(...)` against an `AppConfig`
//! struct, which lights up the schema-aware paths in
//! `config show / get / schema / validate`.

use std::sync::Arc;

use async_trait::async_trait;
use linkme::distributed_slice;
use rtb::config::Config;
use rtb::core::app::App;
use rtb::core::command::{Command, CommandSpec, BUILTIN_COMMANDS};
use rtb::prelude::*;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

// =====================================================================
// Sample tool credentials — the `credentials list / test / doctor`
// subtree consumes this through `Application::builder()
// .credentials_from(Arc::new(MyCredentials { ... }))`.
//
// Kept separate from the typed `AppConfig` below because
// `CredentialRef` deliberately does not derive `Serialize` (it
// carries `SecretString`s) and so cannot live inside a
// `JsonSchema + Serialize`-bound typed config without a wrapping /
// redaction layer. Real downstream tools either go the same split
// route or layer a `Serialize`-safe wrapper on top of
// `CredentialRef` themselves.
// =====================================================================

/// Tool credentials. Holds two [`CredentialRef`]s so `credentials list`
/// has something to show.
#[derive(Default, Clone)]
struct MyCredentials {
    anthropic: CredentialRef,
    github: CredentialRef,
}

impl CredentialBearing for MyCredentials {
    fn credentials(&self) -> Vec<(&'static str, &CredentialRef)> {
        vec![("anthropic", &self.anthropic), ("github", &self.github)]
    }
}

fn sample_credentials() -> MyCredentials {
    MyCredentials {
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
// Sample typed config — what the v0.4.1
// `Application::builder().config<C>(...)` step expects. Drives
// `config show`, `config get`, `config schema`, and
// `config validate`.
// =====================================================================

/// Typed application config. The `Serialize + Deserialize +
/// JsonSchema` derives are required by the
/// `Application::builder().config<C>(...)` bound.
#[derive(Default, Clone, Serialize, Deserialize, JsonSchema)]
struct AppConfig {
    /// Greeting prefix used by the `greet` command. A trivial field
    /// so the schema surface has something visible.
    #[serde(default = "default_greeting")]
    greeting: String,
    /// Maximum number of times to repeat the greeting. Demonstrates a
    /// numeric field for `config get`/`config validate`.
    #[serde(default = "default_repeat")]
    repeat: u8,
}

fn default_greeting() -> String {
    "hello".to_string()
}

const fn default_repeat() -> u8 {
    1
}

fn sample_config() -> AppConfig {
    AppConfig { greeting: default_greeting(), repeat: default_repeat() }
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
        .credentials_from(Arc::new(sample_credentials()))
        .config(Config::<AppConfig>::with_value(sample_config()))
        .build()?
        .run()
        .await
}
