//! `telemetry` CLI subtree — `status / enable / disable / reset`.
//!
//! Backed by [`rtb_telemetry::consent`]'s persisted-consent file
//! primitives. The file lives at
//! `<ProjectDirs::config_dir()>/<tool>/consent.toml`.
//!
//! # Runtime policy resolution
//!
//! Per the v0.4 scope addendum §3.2, the runtime [`CollectionPolicy`]
//! resolves from this chain (each step short-circuits):
//!
//! 1. **Hardcoded compile-time disable** — when the `telemetry`
//!    Cargo feature on `rtb` is off, the policy is unconditionally
//!    `Disabled`. The subtree is not registered; `telemetry status`
//!    is unreachable from the CLI.
//! 2. **Consent file** — `<config_dir>/<tool>/consent.toml`. State
//!    `enabled` → `Enabled`; `disabled` → `Disabled`; `unset` or
//!    file missing → step 3.
//! 3. **`MYTOOL_TELEMETRY` env var** — `1` / `true` / `on` →
//!    `Enabled`; `0` / `false` / `off` → `Disabled`; absent → step 4.
//! 4. **Default** — `Disabled`. Opt-in is the standing rule.
//!
//! `telemetry status` reports which step decided the current state.
//!
//! [`CollectionPolicy`]: rtb_telemetry::CollectionPolicy
//!
//! # Lint exception
//!
//! `linkme::distributed_slice` emits `#[link_section]` which Rust
//! 1.95+ flags under `unsafe_code`. Allowed at module level — no
//! hand-rolled `unsafe` blocks anywhere in the module.

#![allow(unsafe_code)]

use std::ffi::OsString;
use std::path::PathBuf;

use async_trait::async_trait;
use clap::{Parser, Subcommand};
use directories::ProjectDirs;
use linkme::distributed_slice;
use miette::miette;
use rtb_app::app::App;
use rtb_app::command::{Command, CommandSpec, BUILTIN_COMMANDS};
use rtb_app::features::Feature;
use rtb_telemetry::consent::{self, Consent, ConsentState};
use rtb_telemetry::CollectionPolicy;
use serde::Serialize;
use tabled::Tabled;

use crate::render::{output, strip_global_output, OutputMode};

/// The `telemetry` subcommand.
pub struct TelemetryCmd;

#[async_trait]
impl Command for TelemetryCmd {
    fn spec(&self) -> &CommandSpec {
        static SPEC: CommandSpec = CommandSpec {
            name: "telemetry",
            about: "Manage opt-in telemetry consent (status / enable / disable / reset)",
            aliases: &[],
            feature: Some(Feature::Telemetry),
        };
        &SPEC
    }

    fn subcommand_passthrough(&self) -> bool {
        true
    }

    async fn run(&self, app: App) -> miette::Result<()> {
        let mut args: Vec<OsString> = std::env::args_os().collect();
        if args.len() >= 2 {
            args.drain(..2);
        }
        args.insert(0, OsString::from("telemetry"));
        args = strip_global_output(args);

        let cli = match TelemetryCli::try_parse_from(args) {
            Ok(c) => c,
            Err(e) => {
                use clap::error::ErrorKind;
                if matches!(e.kind(), ErrorKind::DisplayHelp | ErrorKind::DisplayVersion) {
                    print!("{e}");
                    return Ok(());
                }
                return Err(miette!("{e}"));
            }
        };

        let mode = OutputMode::from_args_os();
        match cli.command {
            TelemetrySub::Status => run_status(&app, mode),
            TelemetrySub::Enable => run_enable(&app),
            TelemetrySub::Disable => run_disable(&app),
            TelemetrySub::Reset => run_reset(&app),
        }
    }
}

#[distributed_slice(BUILTIN_COMMANDS)]
fn __register_telemetry() -> Box<dyn Command> {
    Box::new(TelemetryCmd)
}

// ---------------------------------------------------------------------
// clap surface
// ---------------------------------------------------------------------

#[derive(Debug, Parser)]
#[command(name = "telemetry", about = "Manage opt-in telemetry consent")]
struct TelemetryCli {
    #[command(subcommand)]
    command: TelemetrySub,
}

#[derive(Debug, Subcommand)]
enum TelemetrySub {
    /// Print current state, decision timestamp, and consent-file path.
    Status,
    /// Opt in. Refused under `CI=true` (per C3 resolution).
    Enable,
    /// Opt out.
    Disable,
    /// Remove the consent file. Idempotent.
    Reset,
}

// ---------------------------------------------------------------------
// `status`
// ---------------------------------------------------------------------

#[derive(Tabled, Serialize)]
struct StatusRow {
    state: &'static str,
    source: &'static str,
    decided_at: String,
    policy: &'static str,
    consent_file: String,
}

fn run_status(app: &App, mode: OutputMode) -> miette::Result<()> {
    let path = consent_path(app)?;

    // Step 2: consent file.
    if let Some(consent) = consent::read(&path).map_err(|e| miette!("read consent: {e}"))? {
        let row = StatusRow {
            state: state_label(consent.state),
            source: "consent-file",
            decided_at: consent.decided_at.unwrap_or_else(|| "-".to_string()),
            policy: policy_label(CollectionPolicy::from(consent.state)),
            consent_file: path.display().to_string(),
        };
        return output(mode, &[row]).map_err(|e| miette!("render: {e}"));
    }

    // Step 3: MYTOOL_TELEMETRY env override.
    let env_var = format!("{}_TELEMETRY", app.metadata.name.to_uppercase());
    if let Ok(raw) = std::env::var(&env_var) {
        if let Some(state) = parse_env_override(&raw) {
            let row = StatusRow {
                state: state_label(state),
                source: "env-override",
                decided_at: format!("via {env_var}"),
                policy: policy_label(CollectionPolicy::from(state)),
                consent_file: path.display().to_string(),
            };
            return output(mode, &[row]).map_err(|e| miette!("render: {e}"));
        }
    }

    // Step 4: default Disabled.
    let row = StatusRow {
        state: state_label(ConsentState::Unset),
        source: "default",
        decided_at: "-".to_string(),
        policy: policy_label(CollectionPolicy::Disabled),
        consent_file: path.display().to_string(),
    };
    output(mode, &[row]).map_err(|e| miette!("render: {e}"))
}

fn parse_env_override(raw: &str) -> Option<ConsentState> {
    match raw.to_ascii_lowercase().as_str() {
        "1" | "true" | "on" => Some(ConsentState::Enabled),
        "0" | "false" | "off" => Some(ConsentState::Disabled),
        _ => None,
    }
}

const fn state_label(state: ConsentState) -> &'static str {
    match state {
        ConsentState::Enabled => "enabled",
        ConsentState::Disabled => "disabled",
        ConsentState::Unset => "unset",
    }
}

const fn policy_label(policy: CollectionPolicy) -> &'static str {
    match policy {
        CollectionPolicy::Enabled => "Enabled",
        CollectionPolicy::Disabled => "Disabled",
    }
}

// ---------------------------------------------------------------------
// `enable`
// ---------------------------------------------------------------------

fn run_enable(app: &App) -> miette::Result<()> {
    // C3 resolution — refuse under CI. Operators enabling telemetry
    // interactively want a real prompt; a build pipeline silently
    // flipping it on is the wrong default.
    if is_ci() {
        return Err(miette!(
            help = "interactive opt-in only — a CI=true environment may not flip it on",
            "telemetry enable refused under CI=true"
        ));
    }

    let path = consent_path(app)?;
    consent::write(&path, &Consent::enabled_now()).map_err(|e| miette!("write consent: {e}"))?;

    if let Some(notice) = app.metadata.telemetry_notice {
        println!("{notice}");
    } else {
        println!("telemetry enabled. {} now records anonymised usage events.", app.metadata.name);
        println!("(no privacy notice configured; set ToolMetadata::telemetry_notice for a tool-specific message.)");
    }
    println!("consent file: {}", path.display());
    Ok(())
}

// ---------------------------------------------------------------------
// `disable`
// ---------------------------------------------------------------------

fn run_disable(app: &App) -> miette::Result<()> {
    let path = consent_path(app)?;
    consent::write(&path, &Consent::disabled_now()).map_err(|e| miette!("write consent: {e}"))?;
    println!("telemetry disabled. consent file: {}", path.display());
    Ok(())
}

// ---------------------------------------------------------------------
// `reset`
// ---------------------------------------------------------------------

fn run_reset(app: &App) -> miette::Result<()> {
    let path = consent_path(app)?;
    consent::reset(&path).map_err(|e| miette!("reset consent: {e}"))?;
    println!("telemetry consent reset (state → unset). path: {}", path.display());
    Ok(())
}

// ---------------------------------------------------------------------
// shared
// ---------------------------------------------------------------------

fn consent_path(app: &App) -> miette::Result<PathBuf> {
    // Match the existing rtb-cli pattern: ProjectDirs("dev", "", &name).
    // Tools that override the qualifier reach down through their own
    // builder; v0.4 keeps the default consistent.
    let dirs = ProjectDirs::from("dev", "", &app.metadata.name).ok_or_else(|| {
        miette!(
            help = "rtb-cli could not derive a config directory; HOME may be unset",
            "no config directory available for tool `{}`",
            app.metadata.name
        )
    })?;
    Ok(dirs.config_dir().join("consent.toml"))
}

fn is_ci() -> bool {
    std::env::var("CI").as_deref() == Ok("true")
}
