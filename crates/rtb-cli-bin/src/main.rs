//! `rtb` — the Rust Tool Base companion CLI (scaffolder / regenerator).
//!
//! Analogous to `gtb` in the Go project. Provides:
//!
//! * `rtb new <name>` — scaffold a new tool (minijinja templates).
//! * `rtb generate command <name>` — add a new subcommand module.
//! * `rtb regenerate` — reconcile `.rtb/manifest.toml` with the source tree.
//! * `rtb doctor` — sanity-check the workspace.
//!
//! This binary is intentionally separate from `rtb-cli` (the library) so
//! downstream tools don't pull in `minijinja`, prompt libraries, etc.

fn main() -> miette::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    miette::set_panic_hook();
    // TODO: wire clap parser + subcommand dispatch per docs/development/specs/rust-tool-base.md
    println!("rtb {} — scaffolder stub", env!("CARGO_PKG_VERSION"));
    Ok(())
}
