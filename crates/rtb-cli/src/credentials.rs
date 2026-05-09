//! `credentials` CLI subtree — `list / add / remove / test / doctor`.
//!
//! Backed by `App::credentials_provider` (see
//! [`rtb_app::credentials::CredentialProvider`]) and
//! `rtb-credentials`'s [`Resolver`] / [`KeyringStore`]. Honours the
//! global `--output text|json` flag for the structured-data leaves
//! (`list`, `test`, `doctor`); `add` and `remove` are interactive
//! and ignore it.
//!
//! # Lint exception
//!
//! `linkme::distributed_slice` emits `#[link_section]` which Rust
//! 1.95+ flags under `unsafe_code`. Allowed at module level — no
//! hand-rolled `unsafe` blocks anywhere in the module.

#![allow(unsafe_code)]

use std::ffi::OsString;
use std::sync::Arc;

use async_trait::async_trait;
use clap::{Parser, Subcommand};
use linkme::distributed_slice;
use miette::miette;
use rtb_app::app::App;
use rtb_app::command::{Command, CommandSpec, BUILTIN_COMMANDS};
use rtb_app::features::Feature;
use rtb_credentials::{
    CredentialError, CredentialRef, CredentialStore, KeyringStore, ResolutionOutcome,
    ResolutionSource, Resolver, SecretString,
};
use serde::Serialize;
use tabled::Tabled;

use crate::render::{output, strip_global_output, OutputMode};

/// The `credentials` subcommand.
pub struct CredentialsCmd;

#[async_trait]
impl Command for CredentialsCmd {
    fn spec(&self) -> &CommandSpec {
        static SPEC: CommandSpec = CommandSpec {
            name: "credentials",
            about: "Manage credential storage (list / add / remove / test / doctor)",
            aliases: &[],
            feature: Some(Feature::Credentials),
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
        args.insert(0, OsString::from("credentials"));
        // The global `--output` flag is parsed once via
        // `OutputMode::from_args_os` above; strip it here so the
        // inner clap parser doesn't reject it as unknown. clap's
        // outer `global = true` propagation works for normal
        // subcommands, but `subcommand_passthrough = true` captures
        // post-name tokens as `trailing_var_arg`, so the global
        // never reaches the outer parser for this subtree.
        args = strip_global_output(args);

        let cli = match CredentialsCli::try_parse_from(args) {
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
            CredentialsSub::List => run_list(&app, mode),
            CredentialsSub::Add { name } => run_add(&app, &name).await,
            CredentialsSub::Remove { name } => run_remove(&app, &name).await,
            CredentialsSub::Test { name } => run_test(&app, &name, mode).await,
            CredentialsSub::Doctor => run_doctor(&app, mode).await,
        }
    }
}

#[distributed_slice(BUILTIN_COMMANDS)]
fn __register_credentials() -> Box<dyn Command> {
    Box::new(CredentialsCmd)
}

// ---------------------------------------------------------------------
// clap surface
// ---------------------------------------------------------------------

#[derive(Debug, Parser)]
#[command(name = "credentials", about = "Manage credential storage")]
struct CredentialsCli {
    #[command(subcommand)]
    command: CredentialsSub,
}

#[derive(Debug, Subcommand)]
enum CredentialsSub {
    /// List every declared credential and the resolver's view of it.
    List,
    /// Add or overwrite a credential through an interactive wizard.
    Add {
        /// Name of the credential as registered via the
        /// `CredentialBearing` impl on the tool's typed config.
        name: String,
    },
    /// Remove a keychain-stored credential.
    Remove {
        /// Name of the credential.
        name: String,
    },
    /// Probe a credential's resolution and print what hit.
    Test {
        /// Name of the credential.
        name: String,
    },
    /// Aggregate `test` calls for every declared credential.
    Doctor,
}

// ---------------------------------------------------------------------
// `list`
// ---------------------------------------------------------------------

#[derive(Tabled, Serialize)]
struct ListRow {
    name: String,
    service: String,
    account: String,
    mode: &'static str,
    status: &'static str,
}

fn run_list(app: &App, mode: OutputMode) -> miette::Result<()> {
    let creds = app.credentials();
    let rows: Vec<ListRow> = creds
        .iter()
        .map(|(name, cref)| {
            let (service, account) = cref.keychain.as_ref().map_or_else(
                || ("-".to_string(), "-".to_string()),
                |k| (k.service.clone(), k.account.clone()),
            );
            // `list` is a config-shape inspection; report the
            // first-configured layer rather than running the full
            // resolver probe (which can hit the keychain). `doctor`
            // is the leaf that does I/O.
            let mode_str = first_configured_layer(cref);
            ListRow { name: name.clone(), service, account, mode: mode_str, status: "-" }
        })
        .collect();
    output(mode, &rows).map_err(|e| miette!("render: {e}"))
}

const fn first_configured_layer(cref: &CredentialRef) -> &'static str {
    if cref.env.is_some() {
        "env"
    } else if cref.keychain.is_some() {
        "keychain"
    } else if cref.literal.is_some() {
        "literal"
    } else if cref.fallback_env.is_some() {
        "fallback-env"
    } else {
        "unconfigured"
    }
}

// ---------------------------------------------------------------------
// `add`
// ---------------------------------------------------------------------

async fn run_add(app: &App, name: &str) -> miette::Result<()> {
    let cred = lookup_credential(app, name)?;

    // C5 resolution — refuse a literal-only ref. Adding a layer the
    // config doesn't declare invites resolve-time surprises.
    let only_literal = cred.literal.is_some()
        && cred.env.is_none()
        && cred.keychain.is_none()
        && cred.fallback_env.is_none();
    if only_literal {
        return Err(miette!(
            help = "edit your config to add an `env` or `keychain` layer first",
            "credential `{name}` declares only a literal layer; refusing to add an undeclared override"
        ));
    }

    // Interactive wizard: pick storage mode, then capture the secret.
    let mode = match cred.keychain.as_ref() {
        Some(_) => Some(StorageChoice::Keychain),
        None => Some(StorageChoice::Env),
    };
    let storage = mode.ok_or_else(|| miette!("credential `{name}` declares no settable layer"))?;

    match storage {
        StorageChoice::Env => {
            let var = cred.env.as_deref().ok_or_else(|| {
                miette!(
                    "credential `{name}` has no env layer to populate; declare it in config first"
                )
            })?;
            let secret = prompt_secret(name).map_err(|e| miette!("prompt: {e}"))?;
            // SAFETY: This is the operator's interactive shell — we
            // surface the right env-var name and exit. Storing the
            // secret in the running process's environment would make
            // it visible to child processes; instead, instruct the
            // operator to export the var themselves.
            let _ = secret; // The wizard captured it; we don't echo it.
            println!("set the secret in your shell:");
            println!("    export {var}=...");
            println!("(rtb-cli does not write to the calling shell's environment.)");
            Ok(())
        }
        StorageChoice::Keychain => {
            let keyref = cred.keychain.as_ref().ok_or_else(|| {
                miette!("credential `{name}` has no keychain layer; declare it in config first")
            })?;
            let secret = prompt_secret(name).map_err(|e| miette!("prompt: {e}"))?;
            let store = KeyringStore::new();
            store
                .set(&keyref.service, &keyref.account, secret)
                .await
                .map_err(|e| miette!("keychain set: {e}"))?;
            println!(
                "stored credential `{name}` in keychain (service=`{}`, account=`{}`)",
                keyref.service, keyref.account
            );
            Ok(())
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum StorageChoice {
    Env,
    Keychain,
}

/// Capture a secret without echoing. Uses `inquire::Password` —
/// already a transitive dep through `rtb-tui`.
fn prompt_secret(name: &str) -> Result<SecretString, inquire::InquireError> {
    let raw = inquire::Password::new(&format!("Secret for `{name}`:"))
        .with_display_mode(inquire::PasswordDisplayMode::Masked)
        .without_confirmation()
        .prompt()?;
    Ok(SecretString::from(raw))
}

// ---------------------------------------------------------------------
// `remove`
// ---------------------------------------------------------------------

async fn run_remove(app: &App, name: &str) -> miette::Result<()> {
    let cred = lookup_credential(app, name)?;
    if cred.literal.is_some() && cred.keychain.is_none() {
        // C1 resolution — hard failure on literal-only refs.
        return Err(miette!(
            help = "edit your config file to remove the literal value",
            "credential `{name}` is a literal in config; rtb-cli refuses to silently leave it in place"
        ));
    }
    let keyref = cred
        .keychain
        .as_ref()
        .ok_or_else(|| miette!("credential `{name}` has no keychain layer to remove from"))?;
    let store = KeyringStore::new();
    match store.delete(&keyref.service, &keyref.account).await {
        Ok(()) => {
            println!("removed `{name}` from keychain");
            Ok(())
        }
        Err(CredentialError::NotFound { .. }) => {
            println!("credential `{name}` not present in keychain (nothing to remove)");
            Ok(())
        }
        Err(e) => Err(miette!("keychain delete: {e}")),
    }
}

// ---------------------------------------------------------------------
// `test`
// ---------------------------------------------------------------------

#[derive(Tabled, Serialize)]
struct TestRow {
    name: String,
    source: &'static str,
    status: &'static str,
}

async fn run_test(app: &App, name: &str, mode: OutputMode) -> miette::Result<()> {
    let cred = lookup_credential(app, name)?;
    let resolver = Resolver::with_platform_default();
    let row = probe_to_row(name, &cred, &resolver).await;
    let failed = row.status != "resolved";
    output(mode, &[row]).map_err(|e| miette!("render: {e}"))?;
    if failed {
        return Err(miette!("credential `{name}` did not resolve"));
    }
    Ok(())
}

// ---------------------------------------------------------------------
// `doctor`
// ---------------------------------------------------------------------

async fn run_doctor(app: &App, mode: OutputMode) -> miette::Result<()> {
    let resolver = Resolver::with_platform_default();
    let mut rows = Vec::new();
    let mut any_failed = false;
    for (name, cref) in app.credentials() {
        let row = probe_to_row(&name, &cref, &resolver).await;
        if row.status != "resolved" {
            any_failed = true;
        }
        rows.push(row);
    }
    output(mode, &rows).map_err(|e| miette!("render: {e}"))?;
    if any_failed {
        return Err(miette!("one or more credentials did not resolve"));
    }
    Ok(())
}

async fn probe_to_row(name: &str, cref: &CredentialRef, resolver: &Resolver) -> TestRow {
    match resolver.probe(cref).await {
        Ok(ResolutionOutcome::Resolved(source)) => {
            TestRow { name: name.to_string(), source: source_label(source), status: "resolved" }
        }
        Ok(ResolutionOutcome::LiteralRefusedInCi) => {
            TestRow { name: name.to_string(), source: "literal", status: "refused-in-ci" }
        }
        Ok(ResolutionOutcome::Missing) => {
            TestRow { name: name.to_string(), source: "-", status: "missing" }
        }
        // `Resolver::probe` returns `Err` only for keychain backend
        // failures (locked store, OS-level error). `ResolutionOutcome`
        // is `#[non_exhaustive]` so `_` covers any future variant
        // additions.
        Ok(_) => TestRow { name: name.to_string(), source: "-", status: "unknown" },
        Err(_) => TestRow { name: name.to_string(), source: "-", status: "error" },
    }
}

const fn source_label(s: ResolutionSource) -> &'static str {
    match s {
        ResolutionSource::Env => "env",
        ResolutionSource::Keychain => "keychain",
        ResolutionSource::Literal => "literal",
        ResolutionSource::FallbackEnv => "fallback-env",
        // `ResolutionSource` is `#[non_exhaustive]`; defensive arm
        // for future additions.
        _ => "unknown",
    }
}

// ---------------------------------------------------------------------
// shared
// ---------------------------------------------------------------------

fn lookup_credential(app: &App, name: &str) -> miette::Result<Arc<CredentialRef>> {
    app.credentials().into_iter().find(|(n, _)| n == name).map(|(_, c)| Arc::new(c)).ok_or_else(
        || {
            let known: Vec<String> = app.credentials().into_iter().map(|(n, _)| n).collect();
            miette!(
                help = if known.is_empty() {
                    "no credentials are configured for this tool".to_string()
                } else {
                    format!("known: {}", known.join(", "))
                },
                "no credential named `{name}`"
            )
        },
    )
}
