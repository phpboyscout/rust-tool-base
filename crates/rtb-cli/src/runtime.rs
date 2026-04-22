//! Runtime wiring helpers: tracing registry install, signal binding.

use std::io::IsTerminal;
use std::sync::Once;

use tokio_util::sync::CancellationToken;
use tracing_subscriber::prelude::*;
use tracing_subscriber::EnvFilter;

/// Ensure the tracing subscriber is installed exactly once per process.
static TRACING_INIT: Once = Once::new();

/// Log format selector — driven by the `--log-format` flag or the
/// `log.format` config key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogFormat {
    /// Pretty, colourised, human-readable. Default on TTY stderr.
    Pretty,
    /// Newline-delimited JSON. Default when stderr is not a TTY.
    Json,
}

impl LogFormat {
    /// Auto-select based on stderr TTY detection.
    #[must_use]
    pub fn auto() -> Self {
        if std::io::stderr().is_terminal() {
            Self::Pretty
        } else {
            Self::Json
        }
    }
}

/// Install the framework's `tracing-subscriber` registry. Idempotent —
/// a second call is a no-op (respecting the `Once`-gated install).
pub fn install_tracing(format: LogFormat) {
    TRACING_INIT.call_once(|| {
        let env_filter =
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

        let registry = tracing_subscriber::registry().with(env_filter);

        match format {
            LogFormat::Pretty => {
                let layer = tracing_subscriber::fmt::layer()
                    .with_target(false)
                    .compact()
                    .with_writer(std::io::stderr);
                let _ = registry.with(layer).try_init();
            }
            LogFormat::Json => {
                let layer = tracing_subscriber::fmt::layer()
                    .json()
                    .with_target(true)
                    .with_writer(std::io::stderr);
                let _ = registry.with(layer).try_init();
            }
        }
    });
}

/// Spawn a task that cancels `token` on `SIGINT` (and on Unix,
/// `SIGTERM`). Returns immediately; the spawned task lives until
/// either signal fires or the runtime shuts down.
pub fn bind_shutdown_signals(token: CancellationToken) {
    tokio::spawn(async move {
        let ctrl_c = async {
            if let Err(e) = tokio::signal::ctrl_c().await {
                tracing::warn!(error = %e, "failed to install Ctrl-C handler");
            }
        };

        #[cfg(unix)]
        {
            use tokio::signal::unix::{signal, SignalKind};
            let mut term = match signal(SignalKind::terminate()) {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!(error = %e, "failed to install SIGTERM handler");
                    ctrl_c.await;
                    token.cancel();
                    return;
                }
            };
            tokio::select! {
                () = ctrl_c => tracing::info!("received Ctrl-C — shutting down"),
                _ = term.recv() => tracing::info!("received SIGTERM — shutting down"),
            }
        }

        #[cfg(not(unix))]
        {
            ctrl_c.await;
            tracing::info!("received Ctrl-C — shutting down");
        }

        token.cancel();
    });
}
