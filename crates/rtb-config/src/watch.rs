//! File-system hot reload for [`crate::Config`].
//!
//! Opt-in via the `hot-reload` Cargo feature. See the v0.2 addendum:
//! `docs/development/specs/2026-04-24-rtb-config-hot-reload.md`.

#![cfg(feature = "hot-reload")]

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use notify::RecursiveMode;
use notify_debouncer_full::new_debouncer;
use serde::de::DeserializeOwned;

use crate::config::Config;
use crate::error::ConfigError;

/// Debounce window for coalescing file-system events. 250 ms is slow
/// enough for editors that save via rename-and-replace (vim, VS Code)
/// and fast enough that users perceive reload as instantaneous.
const DEBOUNCE_MS: u64 = 250;

/// Owns the background watcher for a [`Config`]. Dropping the handle
/// stops the watcher; the thread exits on its next tick.
#[must_use = "dropping `WatchHandle` immediately stops the file watcher"]
pub struct WatchHandle {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl std::fmt::Debug for WatchHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WatchHandle")
            .field("stopped", &self.stop.load(Ordering::Relaxed))
            .finish_non_exhaustive()
    }
}

impl Drop for WatchHandle {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.thread.take() {
            // Best-effort join. Worker thread exits within one
            // debouncer tick (~100ms) once `stop` is set.
            let _ = h.join();
        }
    }
}

impl<C> Config<C>
where
    C: DeserializeOwned + Send + Sync + 'static,
{
    /// Start a background watcher for every path registered via
    /// [`crate::ConfigBuilder::user_file`]. Debounced change events
    /// call [`Config::reload`] — successful reloads wake every
    /// [`Config::subscribe`] receiver.
    ///
    /// # Errors
    ///
    /// - [`ConfigError::Watch`] when no user-file paths were
    ///   registered, or when the underlying `notify` backend fails
    ///   to create a watcher.
    pub fn watch_files(&self) -> Result<WatchHandle, ConfigError> {
        let paths: Vec<PathBuf> = self.sources.files.clone();
        if paths.is_empty() {
            return Err(ConfigError::Watch("no user files registered".into()));
        }

        let (tx, rx) = mpsc::channel();
        let mut debouncer = new_debouncer(Duration::from_millis(DEBOUNCE_MS), None, tx)
            .map_err(|e| ConfigError::Watch(format!("debouncer: {e}")))?;
        for p in &paths {
            debouncer
                .watch(p, RecursiveMode::NonRecursive)
                .map_err(|e| ConfigError::Watch(format!("watch {}: {e}", p.display())))?;
        }

        let this = self.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let stop_thread = Arc::clone(&stop);
        let thread = std::thread::Builder::new()
            .name("rtb-config-watcher".into())
            .spawn(move || {
                // Keep the debouncer alive for the thread's lifetime —
                // it owns the OS watcher handle and the channel sender.
                let _debouncer = debouncer;
                while !stop_thread.load(Ordering::Relaxed) {
                    match rx.recv_timeout(Duration::from_millis(100)) {
                        Ok(Ok(_events)) => {
                            // Failures are logged by reload's caller
                            // contract but not surfaced here — a bad
                            // write shouldn't kill the watcher.
                            let _ = this.reload();
                        }
                        Ok(Err(_notify_errs)) => { /* transient notify errors — keep watching */ }
                        Err(mpsc::RecvTimeoutError::Timeout) => {}
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    }
                }
            })
            .map_err(|e| ConfigError::Watch(format!("spawn watcher thread: {e}")))?;

        Ok(WatchHandle { stop, thread: Some(thread) })
    }
}
