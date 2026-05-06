# Changelog

All notable changes to the Rust Tool Base (RTB) workspace are
documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and the version numbers adhere to [Semantic Versioning](https://semver.org/).

RTB is pre-1.0; the 0.x line treats minor version bumps as
potentially breaking. See `docs/development/specs/rust-tool-base.md`
§ API Stability for the full policy.

## [Unreleased]

### Added — `rtb-cli` ops subtree v0.1 (slice 2 of v0.4, in progress)

- **`CredentialBearing` trait** in `rtb-credentials`. Downstream
  tools implement it on their typed config in five lines; `rtb-cli`'s
  upcoming `credentials list / test / doctor` subcommands walk the
  resulting `Vec` to enumerate credentials. Object-safe; blanket
  impl for `()` keeps existing `App<()>` sites compiling unchanged.
  See [v0.4 scope §4.1](docs/development/specs/2026-05-06-v0.4-scope.md)
  for the design rationale and the alternatives that were rejected
  (serde visitor, schemars-driven walk).

### Added — `rtb-tui` v0.1 (slice 1 of v0.4)

- **`rtb-tui`** flips from a stub to a real crate. Three building
  blocks: `Wizard<S>` + `WizardStep<S>` (multi-step interactive form
  with escape-to-back navigation, Ctrl+C handling, state threading
  via `&mut S`), `render_table` / `render_json` (uniform `tabled` +
  `serde_json` helpers for the upcoming `--output text|json` flag),
  and a TTY-aware `Spinner` that no-ops when stderr isn't a terminal.
- **`tui` Cargo feature on the `rtb` umbrella** flips to default-on.
  Tools that compile-out `tui` explicitly via `default-features =
  false, features = ["cli", ...]` are unaffected.
- **`WizardError`** + **`RenderError`** — both `#[non_exhaustive]`,
  both `Clone`, both `miette::Diagnostic`-deriving.

### Added — `rtb-mcp` v0.1 (slice 2 of v0.3)

- **`rtb-mcp`** flips from `McpStub` to a working MCP server. Walks
  `BUILTIN_COMMANDS` for entries marked `mcp_exposed = true` and
  registers each as an MCP tool whose `tools/call` invokes the
  underlying `Command::run`. Stdio is the working transport; SSE
  and streamable-HTTP variants are present on the `Transport` enum
  but stubbed for v0.3.x.
- **`mcp` CLI subcommand** — `mcp serve [--transport stdio|sse|http]
  [--bind ADDR]` and `mcp list` (prints every exposed tool's name +
  description + JSON schema, one JSON object per line).
- **`Command::mcp_exposed`** + **`Command::mcp_input_schema`**
  default trait methods on `rtb_app::command::Command`. Existing
  impls inherit `false` / `None` defaults — no migration needed.
- **`McpServer`** + **`McpError`** public types in `rtb-mcp`.
  `McpServer::dispatch(name)` exposes the same dispatch path the
  rmcp service loop uses, for unit-test reach.

### Removed

- **`McpStub`** in `rtb-cli::builtins` — superseded by the real
  `McpCmd` registered from `rtb-mcp`.

## [0.2.0] — 2026-05-01

The "v0.2 mandatory crates" release. Four new shipped crates plus
two extensions to existing crates, all behind opt-in features
where they introduce dep weight. CLI dispatch wired for the two
v0.2 commands that previously shipped as discoverability shims.

### Added — new shipped crates

- **`rtb-redact`** — free-form secret redaction helper.
  `redact::string(input)` strips URL userinfo, common credential
  query parameters, `Authorization` header values, well-known
  provider prefixes (`sk-`, `ghp_`, `AIza`, `AKIA`, Slack,
  `sk-ant-…`), and very long opaque tokens. Conservative by
  design — false positives preferred over a leak.
- **`rtb-vcs`** v0.1 release-provider slice.
  `ReleaseProvider` trait + `ReleaseSourceConfig` enum + six
  built-in backends (GitHub / GitLab / Bitbucket / Gitea /
  Codeberg / Direct). Streaming asset downloads via reqwest +
  tokio `AsyncRead`. Rate-limit-aware error mapping. Backends
  registered via `linkme::distributed_slice` so downstream tools
  can plug in custom ones. **The git-operations slice
  (`Repo` type, gix/git2) remains targeted for v0.5** — see the
  v0.2 scope addendum.
- **`rtb-update`** — self-update with Ed25519 signature
  verification, atomic-swap via `self-replace`, dry-run + force
  modes, progress callbacks, `Updater` typestate builder. Uses
  `rtb-vcs` to fetch the configured release source.
- **`rtb-docs`** — `DocsBrowser` two-pane ratatui TUI,
  `DocsServer` loopback HTTP server (axum 0.8), tantivy
  full-text + fuzzy-title search, markdown rendering via
  `tui-markdown` + `pulldown-cmark`. AI Q&A trait seam gated on
  the `ai` Cargo feature (real impl ships with rtb-ai v0.3).

### Added — extensions to v0.1 crates

- **`rtb-config`**:
  - `Config::subscribe()` returns a `tokio::sync::watch::Receiver`
    that wakes on every successful `reload()`. Always-on
    (`tokio::sync::watch` is already in the dep graph).
  - `Config::watch_files()` behind the new `hot-reload` Cargo
    feature: a debounced (250ms) background watcher that calls
    `reload()` on filesystem change and returns a `WatchHandle`
    whose `Drop` stops the worker.
  - `ConfigError::Watch(String)` additive variant.
- **`rtb-telemetry`**:
  - `Event` gains optional `args` and `err_msg` fields; both run
    through `rtb_redact::string` automatically inside every
    out-of-process sink (see `Event::redacted`).
  - New `HttpSink` and `OtlpSink` behind the `remote-sinks`
    Cargo feature. `HttpSink` POSTs JSON; `OtlpSink` exports
    OTLP/gRPC or OTLP/HTTP-protobuf depending on the endpoint
    scheme. Severity is derived from `err_msg.is_some()` so
    downstream alerting can filter without post-processing.
    `TelemetryError::Http` and `TelemetryError::Otlp` additive
    variants.

### Added — CLI dispatch (post-v0.2 follow-ups, also in 0.2.0)

- **`docs`** subcommand:
  `docs list` / `docs show <path> [--format plain|html]` /
  `docs browse` (full TUI event loop) /
  `docs serve [--bind addr]` / `docs ask` (errors when the `ai`
  feature is off).
- **`update`** subcommand:
  `update check` (default) /
  `update run [--target] [--force] [--include-prereleases] [--dry-run] [--progress]`.
- **`Command::subcommand_passthrough(&self) -> bool`** —
  default-method addition on `rtb_app::command::Command`. When
  `true`, `rtb-cli`'s top-level clap parser captures every arg
  after `<name>` verbatim so the command's own clap subtree owns
  parsing/help/error rendering. Backwards-compatible — existing
  `Command` impls inherit the `false` default unchanged.
- **`UpdaterBuilder::cache_dir(...)`** — staging-dir override
  for tools honouring a `--cache-dir` flag (and to isolate
  parallel test processes).

### Changed

- **`rtb-vcs::github`** consolidated onto the shared
  `rtb_vcs::http` helpers — `check_status` shrinks to a
  four-line shim around `http::map_status_to_error` that
  preserves GitHub's `403 + X-RateLimit-Remaining: 0`
  rate-limit hint. Same wire behaviour, less duplicated code.
- **`opentelemetry-otlp` workspace dep** moved to
  `default-features = false` with an explicit feature set
  (`grpc-tonic` + `http-proto` + `reqwest-client` + `logs` +
  `trace`) so OTLP's HTTP transport actually picks up a client.

### Fixed

- **`rtb-update` test cache races** — every test that drives
  `Updater::run` now passes a per-test `tempfile::TempDir` via
  the new `UpdaterBuilder::cache_dir(...)`. Resolves the
  intermittent `t13_self_test_failed` / `t14_dry_run_does_not_swap`
  flakes seen on prior PRs (every test wrote into the shared
  `<project-cache>/widget/update/v1.2.3/` path under nextest's
  one-process-per-test execution).
- **`rtb-config::reload`** uses `watch::Sender::send_replace`
  (not `send`) so a late `subscribe()` after the last receiver
  was dropped still observes the newest value.

### Known issues / deferred

- `rtb-app::ReleaseSource` only carries `Github` / `Gitlab` /
  `Direct` variants. The full six-variant expansion to match
  `rtb-vcs::ReleaseSourceConfig` (Bitbucket / Gitea / Codeberg)
  is queued for a future release; `update`'s mapper errors
  cleanly on unmapped variants.
- `update rollback` and `--channel` deferred — both need new
  metadata or `self-replace` features that aren't in the v0.2
  surface.
- PAT auth via `rtb-credentials` lands with rtb-ai's
  credential-resolution work in v0.3.

## [0.1.1] — 2026-04-23

Housekeeping release. No behavioural changes to shipped crates.

### Changed

- **`rtb-core` renamed to `rtb-app`.** The crate's primary export is
  the `App` context, and its contents (metadata, version, features,
  the `Command` trait) all orbit tool identity — "core" was an
  abstract name that invited mis-reading. `rtb-app` makes the scope
  explicit. All downstream imports updated; no API changes.
- **`rtb-credentials`:** `Resolver` now clones `SecretString` directly
  on the literal-credential path instead of round-tripping through
  `expose_secret().to_string()`. Behavioural no-op — keeps the secret
  inside the zeroize-on-drop container for the full copy. Caught by
  the v0.1 secondary review.

### Migration

Rename every `use rtb_core::…` to `use rtb_app::…`, and every
`rtb-core = { … }` Cargo dependency to `rtb-app = { … }`. The
`prelude` re-export list is unchanged.

## [0.1.0] — 2026-04-23

Initial workspace release. Eight shipped crates (seven feat + the
`rtb-test-support` dev helper), 154+ acceptance criteria across
unit + BDD + trybuild fixtures, a fully-wired Zensical
documentation microsite, and an `assert_cmd`-validated reference
example.

### Added — per crate

- **rtb-error** — typed `Error` enum + `miette` hook installation
  (report handler, panic hook, tool-specific footer). Footer
  closures run inside `catch_unwind` + a thread-local re-entry
  guard so a panicking closure can't poison the render pipeline.
- **rtb-app** — `App` context, `ToolMetadata` + `bon::Builder`,
  `VersionInfo`, `Features`/`FeaturesBuilder`, `Command` trait,
  `BUILTIN_COMMANDS` `linkme` distributed slice.
- **rtb-config** — `Config<C = ()>` layered over `figment`, with
  `ConfigBuilder` for embedded / user-file / env-prefixed sources
  and atomic `reload` via `arc_swap`.
- **rtb-assets** — overlay filesystem over `rust-embed` + physical
  dirs + in-memory fixtures. Binary last-wins shadowing, YAML/JSON
  deep-merge via `json-patch`. `DirectorySource` rejects path
  traversal lexically via `safe_join`.
- **rtb-cli** — `Application::builder` (hand-rolled typestate),
  clap integration, built-in commands (`version`, `doctor`, `init`,
  `config`), feature-gated placeholders for `update`/`docs`/`mcp`.
  `HealthCheck` and `Initialiser` traits with distributed-slice
  registration. `BUILTIN_COMMANDS` is deduplicated by name so
  downstream crates can replace framework stubs cleanly.
  `--help`/`--version` return `Ok(())` rather than producing a
  trailing empty diagnostic.
- **rtb-credentials** — `CredentialStore` async trait +
  `KeyringStore` / `EnvStore` / `LiteralStore` / `MemoryStore`,
  precedence-aware `Resolver` (`env > keychain > literal >
  fallback_env`), `SecretString` end-to-end, `CI=true` literal
  refusal. `CredentialError` derives `Clone` with `Arc<io::Error>`
  in the `Io` variant.
- **rtb-telemetry** — opt-in `TelemetryContext` + `TelemetrySink`
  async trait + `NoopSink` / `MemorySink` / `FileSink` (JSONL),
  salted SHA-256 machine ID, two-level opt-in policy. `FileSink`
  serialises concurrent writes so JSONL lines can't interleave
  for events above POSIX `PIPE_BUF`.
- **rtb-test-support** — sealed-trait `TestAppBuilder` for
  constructing `App` in downstream tests without the full
  `rtb-cli` wiring. Dev-dependency only.

### Added — reference example

- `examples/minimal` — a working, buildable reference tool that
  matches the README quick-start pattern. Registers a custom
  `Greet` command via `linkme`. Smoke-tested via
  `examples/minimal/tests/smoke.rs` with `assert_cmd` so any
  drift between README contract and runtime behaviour fails
  `cargo test`.

### Added — workspace infrastructure

- Cargo workspace with 16 crates (8 shipped + 7 stubs + umbrella);
  shared `[workspace.package]` metadata, pinned stable toolchain.
- CI workflows: rustfmt, clippy (`-D warnings`), nextest (Linux /
  macOS / Windows), cargo-deny, cargo-doc (`-D warnings`),
  cargo-llvm-cov (≥70% line coverage gate).
- BDD harness: `cucumber-rs` wired into `cargo test` per crate,
  `tests/features/` + `tests/steps/` convention documented in
  `docs/development/bdd-pattern.md`.
- `just ci` / `just ci-full` local gates.
- Keyring Linux backend defaults to pure-Rust `linux-native`
  (keyutils); reboot-persistent Secret Service storage is an
  opt-in feature (`credentials-linux-persistent`) to keep hermetic
  local dev builds.

### Added — documentation

- Framework-level spec `docs/development/specs/rust-tool-base.md`
  covering every subsystem, with shipped-vs-deferred annotations
  at each forward-looking section (§8, §9, §10, §12.1, §15).
- Per-crate v0.1 specs under
  `docs/development/specs/2026-04-22-*.md`, all marked
  `IMPLEMENTED`.
- `docs/development/engineering-standards.md` — standing
  requirements for security, concurrency, documentation, and
  testing discipline. Referenced from `CLAUDE.md` so agents
  picking up the project inherit the rules.
- Concept pages (`docs/concepts/`): app-context, configuration,
  error-diagnostics.
- Per-crate component pages (`docs/components/`) for all eight
  shipped crates, styled for the Zensical microsite.
- `CLAUDE.md` — agent onboarding + workflow + anti-patterns.
- `docs/about/why-rtb.md`, `docs/about/ecosystem-survey.md`.

### Added — documentation pipeline

- `zensical.toml` at repo root with theme + palette + status
  taxonomy matching `go-tool-base`.
- `requirements-lock.txt` hash-pinning the Python toolchain
  (zensical 0.0.33 + transitives) for reproducible CI builds.
- `.github/workflows/docs.yaml` builds the microsite on every PR
  (verify, no deploy) and deploys to GitHub Pages on push-to-main
  via `actions/deploy-pages`. SHA-pinned actions;
  `persist-credentials: false` on checkout.
- Local preview via `just site-build` / `just site-serve`
  (assumes `zensical` is on PATH, e.g. via `pipx install`).

### Not in 0.1.0 — deferred

- `rtb-update` — v0.2 target. `rtb-cli` ships an `update`
  command stub returning `FeatureDisabled`.
- `rtb-docs` — v0.2 target. `docs` subcommand is a stub.
- `rtb-mcp` — v0.3 target. `mcp` subcommand is a stub.
- `rtb-ai` — v0.3 target.
- `rtb-tui` — v0.4 target.
- `rtb-vcs` — v0.5 target. `rtb-update` will use a hardcoded
  GitHub path until this crate ships.
- `rtb-cli-bin` scaffolder (`rtb new`, `rtb generate`) — v0.6
  target. The binary exists in 0.1.0 to reserve the command
  name.

See `docs/development/specs/rust-tool-base.md` §16 for the full
roadmap.

[Unreleased]: https://github.com/phpboyscout/rust-tool-base/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/phpboyscout/rust-tool-base/compare/v0.1.1...v0.2.0
[0.1.1]: https://github.com/phpboyscout/rust-tool-base/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/phpboyscout/rust-tool-base/releases/tag/v0.1.0
