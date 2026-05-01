//! `update` CLI subcommand — `check | run`.
//!
//! Wires the [`crate::Updater`] library API to the user-facing CLI.
//! Subcommands:
//!
//! - `update check` — print whether a newer version is available.
//! - `update run [--target X.Y.Z] [--force] [--include-prereleases] [--dry-run]`
//!   — execute the full self-update flow.
//!
//! No subcommand defaults to `check` (cheapest, most common).
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
use miette::{miette, IntoDiagnostic};
use rtb_app::app::App;
use rtb_app::command::{Command, CommandSpec, BUILTIN_COMMANDS};
use rtb_app::features::Feature;
use rtb_app::metadata::ReleaseSource;
use rtb_vcs::{config::ReleaseSourceConfig, ReleaseProvider};

use crate::options::{CheckOutcome, ProgressEvent, RunOptions};
use crate::updater::Updater;

/// The `update` subcommand.
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

    /// `update` owns its inner clap subtree (`check / run`).
    fn subcommand_passthrough(&self) -> bool {
        true
    }

    async fn run(&self, app: App) -> miette::Result<()> {
        let mut args: Vec<OsString> = std::env::args_os().collect();
        if args.len() >= 2 {
            args.drain(..2);
        }
        args.insert(0, OsString::from("update"));
        let cli = match UpdateCli::try_parse_from(args) {
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

        // Default subcommand is `check` — cheapest, most-common path.
        let sub = cli.command.unwrap_or_else(|| UpdateSub::Check(CheckOpts {}));
        match sub {
            UpdateSub::Check(_) => run_check(&app).await,
            UpdateSub::Run(opts) => run_run(&app, opts).await,
        }
    }
}

#[distributed_slice(BUILTIN_COMMANDS)]
fn __register_update() -> Box<dyn Command> {
    Box::new(UpdateCmd)
}

// ---------------------------------------------------------------------
// clap surface
// ---------------------------------------------------------------------

#[derive(Debug, Parser)]
#[command(name = "update", about = "Self-update from the configured release source")]
struct UpdateCli {
    #[command(subcommand)]
    command: Option<UpdateSub>,
}

#[derive(Debug, Subcommand)]
enum UpdateSub {
    /// Print whether a newer version is available; no download.
    Check(CheckOpts),
    /// Run the full self-update flow (download + verify + swap).
    Run(RunOpts),
}

#[derive(Debug, clap::Args)]
struct CheckOpts {}

#[derive(Debug, clap::Args)]
#[allow(clippy::struct_excessive_bools)] // CLI flags, not state.
struct RunOpts {
    /// Pin to a specific version (semver, no `v` prefix).
    /// Downgrades require `--force`.
    #[arg(long, value_name = "VERSION")]
    target: Option<semver::Version>,
    /// Re-install even when already up to date. Repairs corrupted
    /// binaries and bypasses the downgrade check.
    #[arg(long)]
    force: bool,
    /// Allow prereleases when picking "latest".
    #[arg(long)]
    include_prereleases: bool,
    /// Verify + stage but do not swap. Leaves the staged binary
    /// in the cache dir and prints its path.
    #[arg(long)]
    dry_run: bool,
    /// Print progress events to stderr as the flow runs.
    #[arg(long)]
    progress: bool,
}

// ---------------------------------------------------------------------
// Subcommand bodies
// ---------------------------------------------------------------------

async fn run_check(app: &App) -> miette::Result<()> {
    let provider = build_provider(app)?;
    let updater = Updater::builder().app(app).provider(provider).build();
    match updater.check().await.into_diagnostic()? {
        CheckOutcome::UpToDate { current } => {
            println!("up to date — running version {current}");
        }
        CheckOutcome::Newer { current, latest, .. } => {
            println!("new version available: {current} -> {latest}");
            println!("run `{} update run` to install", app.metadata.name);
        }
        CheckOutcome::Older { current, latest } => {
            println!(
                "running newer than the upstream report: \
                 current {current} > latest {latest} (likely tool-author misconfiguration)",
            );
        }
    }
    Ok(())
}

async fn run_run(app: &App, opts: RunOpts) -> miette::Result<()> {
    let provider = build_provider(app)?;
    let progress = if opts.progress { Some(progress_sink()) } else { None };
    let updater = Updater::builder().app(app).provider(provider).build();
    let outcome = updater
        .run(RunOptions {
            target: opts.target,
            force: opts.force,
            include_prereleases: opts.include_prereleases,
            dry_run: opts.dry_run,
            progress,
        })
        .await
        .into_diagnostic()?;

    if outcome.swapped {
        println!("updated: {} -> {}", outcome.from_version, outcome.to_version);
    } else if let Some(staged) = outcome.staged_at {
        println!(
            "dry run: staged {} -> {} at {}",
            outcome.from_version,
            outcome.to_version,
            staged.display(),
        );
    } else {
        println!("already at {}", outcome.to_version);
    }
    Ok(())
}

// ---------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------

/// Build a [`ReleaseProvider`] from `app.metadata.release_source`.
/// Matches each [`ReleaseSource`] variant to the corresponding
/// [`ReleaseSourceConfig`] and dispatches via [`rtb_vcs::lookup`].
fn build_provider(app: &App) -> miette::Result<Arc<dyn ReleaseProvider>> {
    let source = app
        .metadata
        .release_source
        .as_ref()
        .ok_or_else(|| miette!("update: no `release_source` configured on ToolMetadata"))?;
    let config = release_source_to_config(source)?;
    let factory = rtb_vcs::lookup(config.source_type()).ok_or_else(|| {
        miette!(
            "update: no provider registered for source_type={:?}; \
             rtb-vcs may have been compiled without that backend feature",
            config.source_type(),
        )
    })?;
    // PAT plumbing arrives with rtb-credentials integration in
    // v0.3 — for now, run unauthenticated (rate-limited but
    // correct for public releases).
    factory(&config, None).into_diagnostic()
}

fn release_source_to_config(source: &ReleaseSource) -> miette::Result<ReleaseSourceConfig> {
    use rtb_vcs::config::{
        BitbucketParams, CodebergParams, DirectParams, GiteaParams, GithubParams, GitlabParams,
    };
    match source {
        ReleaseSource::Github { owner, repo, host } => {
            Ok(ReleaseSourceConfig::Github(GithubParams {
                host: host.clone(),
                owner: owner.clone(),
                repo: repo.clone(),
                private: false,
                timeout_seconds: 30,
                allow_insecure_base_url: false,
            }))
        }
        ReleaseSource::Gitlab { project, host } => {
            // The rtb-app v0.1 ReleaseSource collapses owner/repo into a
            // single `project` slug (e.g. `myorg/group/project`).
            // rtb-vcs's GitlabParams splits them — derive owner/repo by
            // splitting on the last `/`.
            let (owner, repo) = project.rsplit_once('/').ok_or_else(|| {
                miette!(
                    "update: gitlab `project` must include the owner (`<owner>/<repo>`); \
                     got {project:?}",
                )
            })?;
            Ok(ReleaseSourceConfig::Gitlab(GitlabParams {
                host: host.clone(),
                owner: owner.to_string(),
                repo: repo.to_string(),
                private: false,
                timeout_seconds: 30,
                allow_insecure_base_url: false,
            }))
        }
        ReleaseSource::Bitbucket { workspace, repo_slug, host } => {
            Ok(ReleaseSourceConfig::Bitbucket(BitbucketParams {
                host: host.clone(),
                workspace: workspace.clone(),
                repo_slug: repo_slug.clone(),
                username: None,
                private: false,
                timeout_seconds: 30,
                allow_insecure_base_url: false,
            }))
        }
        ReleaseSource::Gitea { owner, repo, host } => Ok(ReleaseSourceConfig::Gitea(GiteaParams {
            host: host.clone(),
            owner: owner.clone(),
            repo: repo.clone(),
            private: false,
            timeout_seconds: 30,
            allow_insecure_base_url: false,
        })),
        ReleaseSource::Codeberg { owner, repo } => {
            Ok(ReleaseSourceConfig::Codeberg(CodebergParams {
                owner: owner.clone(),
                repo: repo.clone(),
                private: false,
                timeout_seconds: 30,
                allow_insecure_base_url: false,
            }))
        }
        ReleaseSource::Direct { url_template } => Ok(ReleaseSourceConfig::Direct(DirectParams {
            version_url: url_template.clone(),
            asset_url_template: url_template.clone(),
            pinned_version: None,
            timeout_seconds: 30,
            allow_insecure_base_url: false,
        })),
        // `ReleaseSource` is `#[non_exhaustive]`; defensive arm for
        // any future variant added without updating this mapper.
        other => {
            Err(miette!("update: release source {other:?} not yet wired through the update CLI"))
        }
    }
}

fn progress_sink() -> crate::ProgressSink {
    Arc::new(|event: ProgressEvent| match event {
        ProgressEvent::Checking => eprintln!("update: checking…"),
        ProgressEvent::Downloading { bytes_done, bytes_total } => {
            if bytes_total > 0 {
                eprintln!("update: downloading {bytes_done}/{bytes_total}");
            } else {
                eprintln!("update: downloading {bytes_done} bytes");
            }
        }
        ProgressEvent::Verifying => eprintln!("update: verifying signature…"),
        ProgressEvent::SelfTesting => eprintln!("update: self-testing staged binary…"),
        ProgressEvent::Swapping => eprintln!("update: swapping running binary…"),
        ProgressEvent::Done { version } => eprintln!("update: done — now at {version}"),
    })
}
