---
title: rtb-config — subscribe() + hot reload (v0.2 addendum)
status: IMPLEMENTED
date: 2026-04-24
authors: [Matt Cockayne]
crate: rtb-config
supersedes: null
---

# `rtb-config` — `subscribe()` + hot reload (v0.2 addendum)

**Status:** DRAFT — awaiting review before implementation.
**Parent spec:** [`2026-04-22-rtb-config-v0.1.md`](2026-04-22-rtb-config-v0.1.md) § 2.1 "Deferred to v0.2" explicitly parks both items here.
**Scope gate:** [`2026-04-23-v0.2-scope.md`](2026-04-23-v0.2-scope.md) lists `rtb-config::subscribe()` + hot reload as v0.2 mandatory.

---

## 1. Motivation

`rtb-config` v0.1 shipped `Config::reload()` — callers who want a fresh view must re-call it explicitly. v0.2 closes two obvious follow-ups:

1. **Hot reload** — watch registered user files with `notify-debouncer-full` and call `reload()` automatically on change. Users edit their config, the running tool sees the new value without a restart.
2. **`subscribe()`** — callers that need to *react* to a value change (cache invalidation, re-bind a server port, reconnect a provider) get a pull-based `tokio::sync::watch::Receiver<Arc<C>>` instead of polling `get()` on a timer.

Both are flagged in the v0.1 spec as "once values actually change, the reactive API becomes useful" — this is that moment.

## 2. API surface

### 2.1 `Config::subscribe()`

```rust
impl<C> Config<C>
where
    C: DeserializeOwned + Send + Sync + 'static,
{
    /// Subscribe to configuration changes. The returned
    /// `Receiver` yields the current value immediately (via
    /// `borrow()`) and resolves `.changed().await` each time
    /// `reload()` produces a new value (success OR first-time
    /// hot-reload population).
    ///
    /// Subscribers are always notified; there is no diff-check.
    /// Callers who need "only on change" semantics can compare
    /// against their previous snapshot.
    pub fn subscribe(&self) -> tokio::sync::watch::Receiver<Arc<C>>;
}
```

Backed by a `watch::Sender<Arc<C>>` stored alongside the `ArcSwap<C>`. Every successful `reload()` calls both `self.current.store(Arc::new(parsed))` and `self.tx.send(Arc::new(parsed))`.

**Why `watch` and not `broadcast`:**
- Config is a *state*, not a *stream of events*. Late subscribers want the current value, not an empty queue.
- `watch` is lossless for state — the newest value is always observable on the next `.borrow()` or `.changed().await`.
- `broadcast` would force every subscriber to process every intermediate value; `watch` coalesces naturally.

### 2.2 `Config::watch_files()` — opt-in hot reload

```rust
impl<C> Config<C>
where
    C: DeserializeOwned + Send + Sync + 'static,
{
    /// Start a background `notify-debouncer-full` watcher for every
    /// file path the builder registered via `user_file(...)`. On
    /// debounced file-change events (250ms default), calls
    /// `self.reload()`.
    ///
    /// Returns a [`WatchHandle`] whose `Drop` stops the watcher.
    /// Dropping the handle does *not* affect already-emitted
    /// `subscribe()` values.
    ///
    /// # Errors
    ///
    /// [`ConfigError::Watch`] when `notify` can't create a watcher
    /// for the platform, or when no user-file paths were registered.
    pub fn watch_files(&self) -> Result<WatchHandle, ConfigError>;
}

/// Owns the watcher thread. Drop to stop.
#[must_use = "dropping `WatchHandle` immediately stops the watcher"]
pub struct WatchHandle { /* private */ }
```

**Debounce window:** 250 ms — same order of magnitude as editors flushing saves, small enough that users perceive the reload as instantaneous. Expose via a `ConfigBuilder::watch_debounce(Duration)` escape hatch only if a user asks; default is unconfigurable for v0.2.

**Scope of watched paths:** only the `user_file(...)` paths. `embedded_default` is compile-time-constant; `env_prefixed` changes don't fire file-system events. If a user wants env-change hot reload, they can call `Config::reload()` on a signal (outside v0.2 scope).

**Platform support:** `notify` covers Linux (inotify), macOS (FSEvents), Windows (ReadDirectoryChangesW). All three are tier-1 CI targets already.

### 2.3 `ConfigError::Watch`

Additive to the existing `#[non_exhaustive]` enum:

```rust
#[error("config watcher error: {0}")]
#[diagnostic(code(rtb::config::watch))]
Watch(String),
```

### 2.4 Cargo feature

Hot reload pulls in `notify` + `notify-debouncer-full`. Both are already workspace-pinned but not yet active deps of `rtb-config`. Two options:

- **(a) Unconditional** — add the deps to `rtb-config` directly. Simplest; every downstream tool pays the dep weight.
- **(b) Behind `hot-reload` feature** — mirrors the `remote-sinks` precedent on `rtb-telemetry`. Tool authors who don't want hot-reload don't pay for `notify`.

**Recommendation: (b).** `subscribe()` is unconditional (no new deps — `tokio::sync::watch` is already available via the existing `tokio` dep). `watch_files()` is gated on `hot-reload`, with `Config::watch_files` simply missing when the feature is off (not a compile-error stub).

## 3. Implementation sketch

```rust
// config.rs

pub struct Config<C = ()>
where
    C: DeserializeOwned + Send + Sync + 'static,
{
    current: Arc<ArcSwap<C>>,
    tx: Arc<watch::Sender<Arc<C>>>,   // NEW
    sources: Arc<Sources>,
}

impl<C> Config<C> { ... }

impl<C> ConfigBuilder<C> {
    pub fn build(self) -> Result<Config<C>, ConfigError> {
        let parsed = Arc::new(self.sources.parse::<C>()?);
        let (tx, _rx) = watch::channel(Arc::clone(&parsed));
        Ok(Config {
            current: Arc::new(ArcSwap::from(parsed)),
            tx: Arc::new(tx),
            sources: Arc::new(self.sources),
        })
    }
}

impl<C> Config<C> {
    pub fn subscribe(&self) -> watch::Receiver<Arc<C>> {
        self.tx.subscribe()
    }

    pub fn reload(&self) -> Result<(), ConfigError> {
        let parsed = Arc::new(self.sources.parse::<C>()?);
        self.current.store(Arc::clone(&parsed));
        // `send` ignores return — all receivers dropped is fine.
        let _ = self.tx.send(parsed);
        Ok(())
    }
}

#[cfg(feature = "hot-reload")]
impl<C> Config<C> {
    pub fn watch_files(&self) -> Result<WatchHandle, ConfigError> {
        let paths: Vec<PathBuf> = self.sources.files.clone();
        if paths.is_empty() {
            return Err(ConfigError::Watch("no user files registered".into()));
        }
        let this = self.clone();
        // notify-debouncer-full channel
        let (tx, rx) = std::sync::mpsc::channel();
        let mut debouncer = new_debouncer(Duration::from_millis(250), None, tx)
            .map_err(|e| ConfigError::Watch(e.to_string()))?;
        for p in &paths {
            debouncer
                .watcher()
                .watch(p, RecursiveMode::NonRecursive)
                .map_err(|e| ConfigError::Watch(format!("{}: {e}", p.display())))?;
        }
        let stop = Arc::new(AtomicBool::new(false));
        let stop_thread = Arc::clone(&stop);
        let handle = std::thread::spawn(move || {
            while !stop_thread.load(Ordering::Relaxed) {
                match rx.recv_timeout(Duration::from_millis(100)) {
                    Ok(Ok(_events)) => {
                        let _ = this.reload();
                    }
                    Ok(Err(_e)) => { /* debouncer error — ignore, keep watching */ }
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                    Err(_) => break,
                }
            }
            drop(debouncer);
        });
        Ok(WatchHandle { stop, thread: Some(handle) })
    }
}

#[cfg(feature = "hot-reload")]
#[must_use = "dropping `WatchHandle` immediately stops the watcher"]
pub struct WatchHandle {
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

#[cfg(feature = "hot-reload")]
impl Drop for WatchHandle {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.thread.take() {
            let _ = h.join();
        }
    }
}
```

## 4. Test plan (TDD)

Existing config tests are labelled T1–Tn. New ones continue that numbering.

Unit tests (always-on):

- **T_n+1** — `Config::subscribe()` yields the current value on initial `borrow()`.
- **T_n+2** — After `reload()`, subscribers see the new value via `.borrow_and_update()` or `.changed().await`.
- **T_n+3** — `reload()` that fails leaves the stored value AND the subscriber snapshot unchanged.
- **T_n+4** — Late subscribers get the current value, not a historic one.
- **T_n+5** — Dropping all subscribers doesn't error subsequent `reload()` calls (`watch::Sender::send` returning `SendError` is not a `reload()` failure mode).

Unit tests (`hot-reload` feature):

- **T_n+6** — `watch_files()` with no user-file paths surfaces `ConfigError::Watch`.
- **T_n+7** — Writing new content to a watched file triggers `reload()` within ~500ms (250ms debounce + slack).
- **T_n+8** — Dropping the `WatchHandle` stops the watcher — subsequent file writes don't trigger reloads. Verified via a subscriber that counts `changed()` events.

BDD (cucumber):

- **S_n+1** — "Given a Config with a file source, When the file is rewritten, Then a subscriber observes the new value within 500ms" (behind `@hot-reload` tag, filtered when feature is off).

## 5. Security / correctness

- File-watching paths are already under the user's own control (they registered them). No new trust boundary.
- The watcher thread holds no secrets and reads no free-form user input — just dispatches on filesystem events.
- `reload()` failures during hot reload swallow the error and keep serving the old value, matching the v0.1 `reload()` contract. An `error!` tracing log is emitted so operators can investigate.

## 6. Non-goals for v0.2

- **Env-var hot reload.** Requires process-wide signal handling outside the config crate's remit; surface via a user-driven `reload()` call.
- **Debounce tunability.** 250ms is fine for the 99% case; escape hatch added only on request.
- **Per-path watches.** `watch_files()` watches all registered user files as a single atomic set.
- **Back-pressure on subscribers.** `watch` naturally drops intermediate values; `broadcast` semantics (every event delivered) are out of scope.

## 7. Open questions — resolved

- **O1** — `subscribe()` ships **always on**. `tokio::sync::watch` is already in the dep graph via the existing `tokio` dep; there's no cost to downstream tools.
- **O2** — The `watch_files()` lifecycle is owned by a `WatchHandle` (drop-to-stop). A future `watch_files_with_cancel(&self, token: CancellationToken)` can land in a follow-up if downstream callers need external cancellation.
- **O3** — `watch_files()` does **not** call `reload()` synchronously at start. The current value is already fresh from `build()`; a redundant reload would wake every existing subscriber for a no-op.

## 8. Approval gate

Implemented when **(a)** status flips to `APPROVED`, **(b)** T_n+1 through T_n+8 + BDD scenario land green, **(c)** `docs/components/rtb-config.md` gains "Hot reload" + "Subscriptions" subsections, **(d)** `examples/minimal` grows a `--watch-config` flag wiring both.
