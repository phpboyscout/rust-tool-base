# Changelog

All notable changes to the Rust Tool Base (RTB) workspace are
documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and the version numbers adhere to [Semantic Versioning](https://semver.org/).

RTB is pre-1.0; the 0.x line treats minor version bumps as
potentially breaking. See `docs/development/specs/rust-tool-base.md`
§ API Stability for the full policy.

## [Unreleased]

Nothing yet.

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
- **rtb-core** — `App` context, `ToolMetadata` + `bon::Builder`,
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

[Unreleased]: https://github.com/phpboyscout/rust-tool-base/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/phpboyscout/rust-tool-base/releases/tag/v0.1.0
