//! The [`Application`] entry-point type and its hand-rolled typestate
//! builder.

use std::ffi::OsString;
use std::sync::Arc;

use clap::Command as ClapCommand;
use rtb_app::app::App;
use rtb_app::command::{Command as RtbCommand, BUILTIN_COMMANDS};
use rtb_app::features::Features;
use rtb_app::metadata::ToolMetadata;
use rtb_app::version::VersionInfo;
use rtb_assets::Assets;
use rtb_config::Config;
use tokio_util::sync::CancellationToken;

use crate::runtime::{self, LogFormat};

/// A fully-configured application ready to dispatch.
pub struct Application {
    app: App,
    commands: Vec<Box<dyn RtbCommand>>,
    install_hooks: bool,
}

impl Application {
    /// Start building a new application. `metadata` and `version`
    /// must be provided before [`ApplicationBuilder::build`] will
    /// compile — enforced by the phantom-typed typestate below.
    pub const fn builder() -> ApplicationBuilder<NoMetadata, NoVersion> {
        ApplicationBuilder::new()
    }

    /// Parse CLI arguments from `std::env::args_os()`, dispatch,
    /// return.
    pub async fn run(self) -> miette::Result<()> {
        // Collect eagerly — `std::env::ArgsOs` is not `Send`, which
        // would poison the returned Future for multi-thread runtimes.
        let args: Vec<OsString> = std::env::args_os().collect();
        self.run_with_args(args).await
    }

    /// Programmatic dispatch. Useful in tests.
    pub async fn run_with_args<I, S>(self, args: I) -> miette::Result<()>
    where
        I: IntoIterator<Item = S>,
        S: Into<OsString> + Clone,
    {
        if self.install_hooks {
            rtb_error::hook::install_report_handler();
            rtb_error::hook::install_panic_hook();
            if let Some(footer) = self.app.metadata.help.footer() {
                rtb_error::hook::install_with_footer(move || footer.clone());
            }
        }

        runtime::install_tracing(LogFormat::auto());
        runtime::bind_shutdown_signals(self.app.shutdown.clone());

        let clap_cmd = build_clap_tree(&self.app.metadata, &self.commands);
        let matches = match clap_cmd.try_get_matches_from(args) {
            Ok(m) => m,
            Err(e) if is_help_or_version(&e) => {
                // clap already printed help/version to stdout; exit
                // successfully rather than bubble a neutral error up
                // through the diagnostic pipeline.
                print!("{e}");
                return Ok(());
            }
            Err(e) => return Err(map_clap_error(&e)),
        };

        let Some((sub, _sub_matches)) = matches.subcommand() else {
            // No subcommand — clap's `arg_required_else_help` makes
            // the parser itself error in this case, but guard
            // defensively in case a downstream override disables it.
            return Err(rtb_error::Error::CommandNotFound("<none>".into()).into());
        };

        let cmd = self
            .commands
            .iter()
            .find(|c| c.spec().name == sub)
            .ok_or_else(|| rtb_error::Error::CommandNotFound(sub.to_string()))?;

        cmd.run(self.app.clone()).await
    }
}

// -----------------------------------------------------------------
// Typestate builder
// -----------------------------------------------------------------

/// Phantom marker: metadata has not been set.
pub struct NoMetadata;
/// Phantom marker: metadata has been set.
pub struct HasMetadata(ToolMetadata);
/// Phantom marker: version has not been set.
pub struct NoVersion;
/// Phantom marker: version has been set.
pub struct HasVersion(VersionInfo);

/// Typestate-guarded builder. `metadata` and `version` are required;
/// omitting either is a compile error (the `build()` method is only
/// implemented on `ApplicationBuilder<HasMetadata, HasVersion>`).
#[must_use]
pub struct ApplicationBuilder<M, V> {
    metadata: M,
    version: V,
    assets: Option<Assets>,
    features: Option<Features>,
    install_hooks: bool,
}

impl ApplicationBuilder<NoMetadata, NoVersion> {
    /// Construct an empty builder.
    pub const fn new() -> Self {
        Self {
            metadata: NoMetadata,
            version: NoVersion,
            assets: None,
            features: None,
            install_hooks: true,
        }
    }
}

impl Default for ApplicationBuilder<NoMetadata, NoVersion> {
    fn default() -> Self {
        Self::new()
    }
}

impl<V> ApplicationBuilder<NoMetadata, V> {
    /// Set the static tool metadata. Required.
    pub fn metadata(self, m: ToolMetadata) -> ApplicationBuilder<HasMetadata, V> {
        ApplicationBuilder {
            metadata: HasMetadata(m),
            version: self.version,
            assets: self.assets,
            features: self.features,
            install_hooks: self.install_hooks,
        }
    }
}

impl<M> ApplicationBuilder<M, NoVersion> {
    /// Set the build-time version info. Required.
    pub fn version(self, v: VersionInfo) -> ApplicationBuilder<M, HasVersion> {
        ApplicationBuilder {
            metadata: self.metadata,
            version: HasVersion(v),
            assets: self.assets,
            features: self.features,
            install_hooks: self.install_hooks,
        }
    }
}

impl<M, V> ApplicationBuilder<M, V> {
    /// Override the embedded-assets overlay. Defaults to an empty
    /// [`Assets`].
    pub fn assets(mut self, a: Assets) -> Self {
        self.assets = Some(a);
        self
    }

    /// Override the runtime feature set. Defaults to
    /// [`Features::default`].
    pub fn features(mut self, f: Features) -> Self {
        self.features = Some(f);
        self
    }

    /// Control installation of the `miette` report/panic hooks.
    /// `true` by default. Pass `false` from tests that want to
    /// manage hooks themselves.
    pub const fn install_hooks(mut self, yes: bool) -> Self {
        self.install_hooks = yes;
        self
    }
}

impl ApplicationBuilder<HasMetadata, HasVersion> {
    /// Finalise the builder. Only compiles when both
    /// [`ApplicationBuilder::metadata`] and
    /// [`ApplicationBuilder::version`] have been supplied.
    pub fn build(self) -> miette::Result<Application> {
        let HasMetadata(metadata) = self.metadata;
        let HasVersion(version) = self.version;

        let features = self.features.unwrap_or_default();
        let assets = self.assets.unwrap_or_default();

        let app = App {
            metadata: Arc::new(metadata),
            version: Arc::new(version),
            config: Arc::new(Config::<()>::default()),
            assets: Arc::new(assets),
            shutdown: CancellationToken::new(),
        };

        // Materialise BUILTIN_COMMANDS filtered by the runtime
        // Features set.
        let mut commands: Vec<Box<dyn RtbCommand>> = Vec::new();
        for factory in BUILTIN_COMMANDS {
            let cmd = factory();
            let enabled_here = cmd.spec().feature.is_none_or(|f| features.is_enabled(f));
            if enabled_here {
                commands.push(cmd);
            }
        }

        // Deduplicate by command name. `linkme`'s slice order is
        // link-time-determined and not stable across compiler
        // versions or dep graph changes, so we cannot rely on
        // "last-registered wins". Instead, the LAST entry in slice
        // order for each name wins — which matches the intuition
        // that a downstream crate's real command overrides the
        // rtb-cli stub of the same name.
        let mut seen: std::collections::HashMap<&'static str, usize> =
            std::collections::HashMap::new();
        for (idx, cmd) in commands.iter().enumerate() {
            seen.insert(cmd.spec().name, idx);
        }
        let keep: std::collections::HashSet<usize> = seen.values().copied().collect();
        let mut i = 0usize;
        commands.retain(|_| {
            let keep_this = keep.contains(&i);
            i += 1;
            keep_this
        });

        // Stable order by command name keeps `--help` output
        // deterministic regardless of link-time slice ordering.
        commands.sort_by(|a, b| a.spec().name.cmp(b.spec().name));

        Ok(Application { app, commands, install_hooks: self.install_hooks })
    }
}

// -----------------------------------------------------------------
// clap glue
// -----------------------------------------------------------------

fn build_clap_tree(metadata: &ToolMetadata, commands: &[Box<dyn RtbCommand>]) -> ClapCommand {
    let mut root = ClapCommand::new(metadata.name.clone())
        .about(metadata.summary.clone())
        .arg_required_else_help(true)
        .subcommand_required(true)
        // Global `--output text|json` flag. Declared once at the
        // root with `global = true`; clap propagates it onto every
        // subcommand automatically. Subcommands that print
        // structured data honour it via [`crate::render::output`];
        // interactive ones (init, mcp serve, update run) ignore it.
        // See v0.4 scope addendum §2.5 / O5.
        .arg(
            clap::Arg::new("output")
                .long("output")
                .global(true)
                .value_parser(clap::value_parser!(crate::render::OutputMode))
                .default_value("text")
                .help("Output rendering mode for structured-output subcommands"),
        );

    if !metadata.description.is_empty() {
        root = root.long_about(metadata.description.clone());
    }

    for cmd in commands {
        let spec = cmd.spec();
        let mut sub = ClapCommand::new(spec.name).about(spec.about);
        for alias in spec.aliases {
            sub = sub.alias(*alias);
        }
        if cmd.subcommand_passthrough() {
            // Let the command own its inner clap subtree. The
            // `trailing_var_arg` arg captures every token after
            // `<name>` (including `--help`, `--flag value`, sub-sub-
            // commands) without further validation — the command
            // re-parses `std::env::args_os()` itself.
            sub = sub.arg(
                clap::Arg::new("rest")
                    .num_args(0..)
                    .trailing_var_arg(true)
                    .allow_hyphen_values(true),
            );
            // Drop the auto-injected `--help` so it reaches the inner
            // parser instead of clap's default help screen at the
            // outer layer.
            sub = sub.disable_help_flag(true);
        }
        root = root.subcommand(sub);
    }

    root
}

/// `true` when the clap error is a "successful" user-facing output
/// (help or version) that should return `Ok(())` rather than bubble
/// up through the diagnostic pipeline.
fn is_help_or_version(err: &clap::Error) -> bool {
    use clap::error::ErrorKind;
    matches!(err.kind(), ErrorKind::DisplayHelp | ErrorKind::DisplayVersion)
}

fn map_clap_error(err: &clap::Error) -> miette::Report {
    use clap::error::ErrorKind;
    match err.kind() {
        ErrorKind::InvalidSubcommand | ErrorKind::UnknownArgument => {
            let name = err
                .get(clap::error::ContextKind::InvalidSubcommand)
                .or_else(|| err.get(clap::error::ContextKind::InvalidArg))
                .map_or_else(|| err.to_string(), |v| format!("{v}"));
            rtb_error::Error::CommandNotFound(name).into()
        }
        _ => miette::miette!("{}", err),
    }
}
