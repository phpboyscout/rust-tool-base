//! Built-in commands shipped with `rtb-cli`.
//!
//! Every built-in is an `impl Command` registered via
//! [`rtb_app::command::BUILTIN_COMMANDS`]. `Application::run`
//! filters them by the runtime [`Features`](rtb_app::features::Features)
//! set before handing them to `clap`.

use async_trait::async_trait;
use linkme::distributed_slice;
use rtb_app::app::App;
use rtb_app::command::{Command, CommandSpec, BUILTIN_COMMANDS};
use rtb_app::features::Feature;

use crate::health;
use crate::init::INITIALISERS;

// =====================================================================
// version — prints version + commit + date.
// =====================================================================

/// The `version` subcommand.
pub struct VersionCmd;

#[async_trait]
impl Command for VersionCmd {
    fn spec(&self) -> &CommandSpec {
        static SPEC: CommandSpec = CommandSpec {
            name: "version",
            about: "Print tool version information",
            aliases: &[],
            feature: Some(Feature::Version),
        };
        &SPEC
    }

    async fn run(&self, app: App) -> miette::Result<()> {
        let v = &app.version;
        println!("{} {}", app.metadata.name, v.version);
        if let Some(commit) = v.commit.as_deref() {
            println!("  commit: {commit}");
        }
        if let Some(date) = v.date.as_deref() {
            println!("  built:  {date}");
        }
        println!("  target: {}-{}", std::env::consts::ARCH, std::env::consts::OS);
        Ok(())
    }
}

#[distributed_slice(BUILTIN_COMMANDS)]
fn __register_version() -> Box<dyn Command> {
    Box::new(VersionCmd)
}

// =====================================================================
// doctor — runs HEALTH_CHECKS and reports.
// =====================================================================

/// The `doctor` subcommand.
pub struct DoctorCmd;

#[async_trait]
impl Command for DoctorCmd {
    fn spec(&self) -> &CommandSpec {
        static SPEC: CommandSpec = CommandSpec {
            name: "doctor",
            about: "Run diagnostic health checks",
            aliases: &[],
            feature: Some(Feature::Doctor),
        };
        &SPEC
    }

    async fn run(&self, app: App) -> miette::Result<()> {
        let report = health::run_all(&app).await;
        print!("{}", report.render());
        if report.is_ok() {
            Ok(())
        } else {
            Err(miette::miette!(
                code = "rtb::doctor::failed",
                help = "see the report above for which checks failed",
                "one or more health checks failed"
            ))
        }
    }
}

#[distributed_slice(BUILTIN_COMMANDS)]
fn __register_doctor() -> Box<dyn Command> {
    Box::new(DoctorCmd)
}

// =====================================================================
// init — iterates INITIALISERS.
// =====================================================================

/// The `init` subcommand.
pub struct InitCmd;

#[async_trait]
impl Command for InitCmd {
    fn spec(&self) -> &CommandSpec {
        static SPEC: CommandSpec = CommandSpec {
            name: "init",
            about: "Run first-time bootstrap and setup",
            aliases: &[],
            feature: Some(Feature::Init),
        };
        &SPEC
    }

    async fn run(&self, app: App) -> miette::Result<()> {
        if INITIALISERS.is_empty() {
            println!("no initialisers registered — nothing to do");
            return Ok(());
        }
        for factory in INITIALISERS {
            let init = factory();
            if init.is_configured(&app).await {
                println!("  [SKIP] {} — already configured", init.name());
                continue;
            }
            println!("  [RUN]  {}", init.name());
            init.configure(&app).await?;
        }
        Ok(())
    }
}

#[distributed_slice(BUILTIN_COMMANDS)]
fn __register_init() -> Box<dyn Command> {
    Box::new(InitCmd)
}

// =====================================================================
// config — show merged configuration as YAML.
// =====================================================================

/// The `config show` subcommand (v0.1 — only the `show` leaf).
pub struct ConfigShowCmd;

#[async_trait]
impl Command for ConfigShowCmd {
    fn spec(&self) -> &CommandSpec {
        static SPEC: CommandSpec = CommandSpec {
            name: "config",
            about: "Show the currently-resolved configuration",
            aliases: &[],
            feature: Some(Feature::Config),
        };
        &SPEC
    }

    async fn run(&self, _app: App) -> miette::Result<()> {
        // rtb-config v0.1 holds a parsed value behind `Arc<C>`. The
        // framework-level App currently carries `Arc<Config<()>>`, so
        // the only thing we can show here is "no typed config
        // registered". Downstream tools with a real C will override
        // this command by registering their own impl with the same
        // name later in registration order.
        println!("# no typed configuration is installed on this App");
        println!("# (rtb-app's App.config is Config<()> until App<C> lands)");
        Ok(())
    }
}

#[distributed_slice(BUILTIN_COMMANDS)]
fn __register_config() -> Box<dyn Command> {
    Box::new(ConfigShowCmd)
}

// The `update`, `docs`, and `mcp` stubs have been removed —
// `rtb-update`, `rtb-docs`, and `rtb-mcp` register the real
// commands. Downstream tools that disable the corresponding rtb
// features still get the stub-equivalent behaviour: no command
// registered, clap reports "unknown subcommand" if invoked.
