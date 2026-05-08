//! `config` CLI subtree — `show / get / set / schema / validate`.
//!
//! Operates on the canonical user-file path
//! `<ProjectDirs::config_dir()>/<tool>/config.yaml` (overridable via
//! `--config-file PATH`). Format is determined by the file's
//! extension on read and kept consistent on write:
//!
//! | Extension | Format |
//! |---|---|
//! | `.yml`, `.yaml` (or no extension) | YAML |
//! | `.toml` | TOML |
//! | `.json` | JSON |
//!
//! # Design note (v0.4)
//!
//! The framework's `App` currently carries `Arc<Config<()>>` — the
//! typed-config generic `App<C>` is post-v0.1 work. So the v0.4
//! subtree operates on the file directly rather than the typed
//! `Config<C>`. `get` / `set` walk the parsed value as a
//! `serde_yaml::Value`; `schema` errors with a helpful message
//! pointing at the typed-config integration gap. The behaviour
//! upgrades non-disruptively when `App<C>` lands.
//!
//! # Lint exception
//!
//! `linkme::distributed_slice` emits `#[link_section]` which Rust
//! 1.95+ flags under `unsafe_code`. Allowed at module level — no
//! hand-rolled `unsafe` blocks anywhere in the module.

#![allow(unsafe_code)]

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use clap::{Parser, Subcommand};
use directories::ProjectDirs;
use linkme::distributed_slice;
use miette::miette;
use rtb_app::app::App;
use rtb_app::command::{Command, CommandSpec, BUILTIN_COMMANDS};
use rtb_app::features::Feature;

use crate::render::{strip_global_output, OutputMode};

/// The `config` subcommand.
pub struct ConfigCmd;

#[async_trait]
impl Command for ConfigCmd {
    fn spec(&self) -> &CommandSpec {
        static SPEC: CommandSpec = CommandSpec {
            name: "config",
            about: "Show, query, mutate, and validate the user config (show / get / set / schema / validate)",
            aliases: &[],
            feature: Some(Feature::Config),
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
        args.insert(0, OsString::from("config"));
        args = strip_global_output(args);

        let cli = match ConfigCli::try_parse_from(args) {
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
        let sub = cli.command.unwrap_or(ConfigSub::Show(ShowOpts {}));
        match sub {
            ConfigSub::Show(_) => {
                run_show(&app);
                Ok(())
            }
            ConfigSub::Get { path } => run_get(&app, &path, mode),
            ConfigSub::Set { path, value, config_file } => {
                run_set(&app, &path, &value, config_file.as_deref())
            }
            ConfigSub::Schema => run_schema(&app),
            ConfigSub::Validate { config_file } => run_validate(&app, config_file.as_deref()),
        }
    }
}

#[distributed_slice(BUILTIN_COMMANDS)]
fn __register_config() -> Box<dyn Command> {
    Box::new(ConfigCmd)
}

// ---------------------------------------------------------------------
// clap surface
// ---------------------------------------------------------------------

#[derive(Debug, Parser)]
#[command(name = "config", about = "Inspect, mutate, and validate config")]
struct ConfigCli {
    #[command(subcommand)]
    command: Option<ConfigSub>,
}

#[derive(Debug, Subcommand)]
enum ConfigSub {
    /// Print the currently-resolved configuration. Default subcommand.
    Show(ShowOpts),
    /// Read a JSON-pointer path against the user config file.
    Get {
        /// JSON-pointer path (`/foo/bar` style). Leading `/` optional.
        path: String,
    },
    /// Write a value to a JSON-pointer path in the user config file.
    Set {
        /// JSON-pointer path to write to.
        path: String,
        /// Value to write. Parsed as JSON; falls back to a string.
        value: String,
        /// Override the canonical user-file path.
        #[arg(long, value_name = "PATH")]
        config_file: Option<PathBuf>,
    },
    /// Print the JSON Schema for the tool's typed config.
    Schema,
    /// Validate a candidate config file. Defaults to the merged result.
    Validate {
        /// Validate this file instead of the merged config.
        #[arg(long, value_name = "PATH")]
        config_file: Option<PathBuf>,
    },
}

#[derive(Debug, clap::Args)]
struct ShowOpts {}

// ---------------------------------------------------------------------
// `show`
// ---------------------------------------------------------------------

fn run_show(_app: &App) {
    // App.config is currently Config<()> — the typed-config generic
    // App<C> is post-v0.1 work. Keep the v0.1 placeholder text until
    // it lands; downstream tools with a real `C` register their own
    // impl of this command (last-in-slice-order wins) just like in
    // the v0.1 baseline.
    println!("# no typed configuration is installed on this App");
    println!("# (rtb-app's App.config is Config<()> until App<C> lands)");
}

// ---------------------------------------------------------------------
// `get`
// ---------------------------------------------------------------------

fn run_get(app: &App, path: &str, mode: OutputMode) -> miette::Result<()> {
    let file = canonical_path(app)?;
    let value = read_value(&file)?;
    let pointer = json_pointer(path);
    let resolved = value
        .pointer(&pointer)
        .ok_or_else(|| miette!("path `{path}` not found in `{}`", file.display()))?;
    match mode {
        OutputMode::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(resolved).map_err(|e| miette!("serialise: {e}"))?,
            );
        }
        OutputMode::Text => match resolved {
            serde_json::Value::String(s) => println!("{s}"),
            other => println!("{other}"),
        },
    }
    Ok(())
}

// ---------------------------------------------------------------------
// `set`
// ---------------------------------------------------------------------

fn run_set(app: &App, path: &str, value: &str, override_path: Option<&Path>) -> miette::Result<()> {
    let file = match override_path {
        Some(p) => p.to_path_buf(),
        None => canonical_path(app)?,
    };
    // Parse `<value>` as JSON; fall back to a string literal so the
    // common case of a bare scalar (`config set .timeout 30`,
    // `config set .name alice`) does not require quoting.
    let parsed: serde_json::Value = serde_json::from_str(value)
        .unwrap_or_else(|_| serde_json::Value::String(value.to_string()));

    let mut current = if file.exists() {
        read_value(&file)?
    } else {
        serde_json::Value::Object(serde_json::Map::new())
    };
    let pointer = json_pointer(path);
    set_pointer(&mut current, &pointer, parsed).map_err(|msg| miette!("{msg}"))?;

    write_value(&file, &current)?;
    println!("set `{path}` in `{}`", file.display());
    Ok(())
}

// ---------------------------------------------------------------------
// `schema`
// ---------------------------------------------------------------------

fn run_schema(_app: &App) -> miette::Result<()> {
    // Without `App<C>` we have no typed schema to emit. Tools with
    // their own typed config can override this subcommand by
    // registering a `Command` with the same name later in slice
    // order — the same replacement pattern documented for `show`.
    Err(miette!(
        help = "wire your typed config via Application::builder().config(...) once App<C> lands; \
                until then, override the `config` command with your own impl that calls \
                rtb_config::Config::schema()",
        "config schema is not available without a typed-config integration"
    ))
}

// ---------------------------------------------------------------------
// `validate`
// ---------------------------------------------------------------------

fn run_validate(app: &App, override_path: Option<&Path>) -> miette::Result<()> {
    let file = match override_path {
        Some(p) => p.to_path_buf(),
        None => canonical_path(app)?,
    };
    if !file.exists() {
        return Err(miette!("config file `{}` does not exist", file.display()));
    }
    // v0.4 contract without `App<C>`: format-parse only. Schema
    // validation lands when typed-config integration is wired.
    let _ = read_value(&file)?;
    println!("ok: `{}` parses cleanly", file.display());
    Ok(())
}

// ---------------------------------------------------------------------
// shared
// ---------------------------------------------------------------------

fn canonical_path(app: &App) -> miette::Result<PathBuf> {
    let dirs = ProjectDirs::from("dev", "", &app.metadata.name).ok_or_else(|| {
        miette!(
            help = "rtb-cli could not derive a config directory; HOME may be unset",
            "no config directory available for tool `{}`",
            app.metadata.name
        )
    })?;
    Ok(dirs.config_dir().join("config.yaml"))
}

fn read_value(path: &Path) -> miette::Result<serde_json::Value> {
    let body =
        std::fs::read_to_string(path).map_err(|e| miette!("read {}: {e}", path.display()))?;
    let format = file_format(path);
    match format {
        Format::Yaml => {
            let yaml: serde_yaml::Value = serde_yaml::from_str(&body)
                .map_err(|e| miette!("parse yaml `{}`: {e}", path.display()))?;
            // YAML → JSON Value: serde-cross-format. `serde_json::to_value`
            // works for any `Serialize` source.
            serde_json::to_value(yaml).map_err(|e| miette!("yaml→json: {e}"))
        }
        Format::Json => {
            serde_json::from_str(&body).map_err(|e| miette!("parse json `{}`: {e}", path.display()))
        }
        Format::Toml => {
            let raw: toml::Value = toml::from_str(&body)
                .map_err(|e| miette!("parse toml `{}`: {e}", path.display()))?;
            serde_json::to_value(raw).map_err(|e| miette!("toml→json: {e}"))
        }
    }
}

fn write_value(path: &Path, value: &serde_json::Value) -> miette::Result<()> {
    let format = file_format(path);
    let serialised = match format {
        Format::Yaml => serde_yaml::to_string(value).map_err(|e| miette!("yaml: {e}"))?,
        Format::Json => serde_json::to_string_pretty(value).map_err(|e| miette!("json: {e}"))?,
        Format::Toml => {
            // Round-trip through `toml::Value` so non-table tops get a
            // proper error rather than a panic.
            let toml_value: toml::Value =
                toml::Value::try_from(value).map_err(|e| miette!("json→toml: {e}"))?;
            toml::to_string_pretty(&toml_value).map_err(|e| miette!("toml: {e}"))?
        }
    };
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .map_err(|e| miette!("create parent {}: {e}", parent.display()))?;
        }
    }
    std::fs::write(path, serialised).map_err(|e| miette!("write {}: {e}", path.display()))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Format {
    Yaml,
    Toml,
    Json,
}

fn file_format(path: &Path) -> Format {
    match path.extension().and_then(|e| e.to_str()) {
        Some("toml") => Format::Toml,
        Some("json") => Format::Json,
        _ => Format::Yaml,
    }
}

/// Convert a user-supplied path into a `serde_json` JSON-pointer
/// string. Accepts both `/foo/bar` (canonical) and `.foo.bar`
/// (UX-friendly) forms; the canonical form is what
/// `serde_json::Value::pointer` expects.
fn json_pointer(path: &str) -> String {
    if path.starts_with('/') {
        path.to_string()
    } else if let Some(rest) = path.strip_prefix('.') {
        format!("/{}", rest.replace('.', "/"))
    } else {
        format!("/{}", path.replace('.', "/"))
    }
}

/// Write `value` at the given JSON pointer. Creates intermediate
/// `Object` containers as needed. Returns an error string when an
/// intermediate is a non-object (`/foo/bar` against a value where
/// `foo` is a number).
fn set_pointer(
    target: &mut serde_json::Value,
    pointer: &str,
    value: serde_json::Value,
) -> Result<(), String> {
    if pointer.is_empty() || pointer == "/" {
        *target = value;
        return Ok(());
    }
    let segments: Vec<&str> = pointer.trim_start_matches('/').split('/').collect();
    let mut cursor = target;
    for (i, segment) in segments.iter().enumerate() {
        let last = i == segments.len() - 1;
        if last {
            if let serde_json::Value::Object(map) = cursor {
                map.insert((*segment).to_string(), value);
                return Ok(());
            }
            return Err(format!("cannot set `{segment}` on a non-object value"));
        }
        if !cursor.is_object() {
            *cursor = serde_json::Value::Object(serde_json::Map::new());
        }
        let map = cursor.as_object_mut().expect("just-set object");
        cursor = map
            .entry((*segment).to_string())
            .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
    }
    Ok(())
}
