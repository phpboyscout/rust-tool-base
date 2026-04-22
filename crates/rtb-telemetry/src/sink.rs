//! [`TelemetrySink`] trait and the three built-in implementations.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use tokio::io::AsyncWriteExt;

use crate::error::TelemetryError;
use crate::event::Event;

/// Backend-agnostic sink for [`Event`] values.
#[async_trait]
pub trait TelemetrySink: Send + Sync + 'static {
    /// Emit a single event. Sinks SHOULD be idempotent on the event —
    /// retries or batching live in wrapper sinks (v0.2).
    async fn emit(&self, event: &Event) -> Result<(), TelemetryError>;

    /// Flush any buffered events. Default impl is a no-op.
    async fn flush(&self) -> Result<(), TelemetryError> {
        Ok(())
    }
}

// =====================================================================
// NoopSink — drops everything silently.
// =====================================================================

/// Drops every event silently. Used when collection is disabled and
/// as the default when a tool ships telemetry support but hasn't
/// configured a real sink.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopSink;

#[async_trait]
impl TelemetrySink for NoopSink {
    async fn emit(&self, _event: &Event) -> Result<(), TelemetryError> {
        Ok(())
    }
}

// =====================================================================
// MemorySink — in-memory Vec, useful for tests.
// =====================================================================

/// In-memory sink for tests. Events are appended in emit order and
/// inspectable via [`MemorySink::snapshot`].
#[derive(Debug, Default, Clone)]
pub struct MemorySink {
    inner: Arc<Mutex<Vec<Event>>>,
}

impl MemorySink {
    /// Construct a fresh empty sink.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Return a clone of every recorded event.
    #[must_use]
    pub fn snapshot(&self) -> Vec<Event> {
        self.inner.lock().map(|v| v.clone()).unwrap_or_default()
    }

    /// How many events have been recorded so far.
    #[must_use]
    pub fn len(&self) -> usize {
        self.inner.lock().map_or(0, |v| v.len())
    }

    /// `true` when no events have been recorded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[async_trait]
impl TelemetrySink for MemorySink {
    async fn emit(&self, event: &Event) -> Result<(), TelemetryError> {
        if let Ok(mut v) = self.inner.lock() {
            v.push(event.clone());
        }
        Ok(())
    }
}

// =====================================================================
// FileSink — newline-delimited JSON to disk.
// =====================================================================

/// Appends events as JSON Lines to a file. Parent directories are
/// created on demand.
///
/// Each `emit` opens the file in append mode, writes the JSON line,
/// and closes. Batching lives in a wrapper sink when we need it.
#[derive(Debug, Clone)]
pub struct FileSink {
    path: PathBuf,
}

impl FileSink {
    /// Construct a sink targeting `path`. The file is not touched
    /// until the first `emit`; parent directories are created on
    /// demand at that point.
    #[must_use]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
}

#[async_trait]
impl TelemetrySink for FileSink {
    async fn emit(&self, event: &Event) -> Result<(), TelemetryError> {
        let mut line =
            serde_json::to_string(event).map_err(|e| TelemetryError::Serde(e.to_string()))?;
        line.push('\n');

        if let Some(parent) = self.path.parent() {
            if !parent.as_os_str().is_empty() {
                tokio::fs::create_dir_all(parent).await?;
            }
        }

        let mut f =
            tokio::fs::OpenOptions::new().create(true).append(true).open(&self.path).await?;
        f.write_all(line.as_bytes()).await?;
        f.flush().await?;
        Ok(())
    }
}
