# Changelog

All notable changes to the Rust Tool Base (RTB) workspace are
documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and the version numbers adhere to [Semantic Versioning](https://semver.org/).

RTB is pre-1.0; the 0.x line treats minor version bumps as
potentially breaking. See `docs/development/specs/rust-tool-base.md`
§ API Stability for the full policy.

## [Unreleased]

### Added
- `docs/development/engineering-standards.md` — standing requirements
  for security, concurrency, documentation, and testing discipline.
  Referenced from `CLAUDE.md` so agents picking up the project
  inherit the rules.
- `examples/minimal/tests/smoke.rs` — `assert_cmd` smoke test for
  the reference example. Every README/quick-start contract (greet
  output, version shape, doctor exit status, help listing, unknown-
  subcommand error, update-stub diagnostic) is now rustc+runtime
  validated via `cargo test`. Prevents silent docs/code drift.
- **Zensical microsite infrastructure.** `zensical.toml` at repo
  root carries the theme + status taxonomy; `requirements-lock.txt`
  hash-pins the Python toolchain (zensical 0.0.33 + transitives)
  for reproducible CI builds, matching the go-tool-base versions.
  `.github/workflows/docs.yaml` builds on every PR (no deploy) and
  deploys to GitHub Pages on push-to-main via
  `actions/deploy-pages`. Hash-pinned actions and
  `persist-credentials: false` on checkout mirror the go-tool-base
  security posture. Local contributors run `just site-build` /
  `just site-serve` assuming `zensical` is globally installed
  (e.g. `pipx install zensical`).

### Fixed
- Path-traversal vulnerability in `rtb-assets::DirectorySource`
  (`../` escapes now rejected via lexical `safe_join`).
- Footer-closure panics in `rtb-error` no longer poison the miette
  hook; render pipeline is re-entry-safe.
- `rtb-telemetry::FileSink` serialises concurrent writes so JSONL
  lines can't interleave for events above `PIPE_BUF`.
- `rtb-cli` deduplicates `BUILTIN_COMMANDS` by name so downstream
  crates can register real commands over framework stubs without
  clap collision.
- `rtb-cli` `--help` / `--version` no longer print a trailing empty
  diagnostic.

### Changed
- `rtb-core::Feature::all()` now returns `&'static [Self]` instead of
  a fixed-size array, consistent with `#[non_exhaustive]`.
- `rtb-credentials::LiteralStore::get` uses `SecretString::clone`
  instead of bouncing through a bare `String`.
- `rtb-credentials::CredentialError` derives `Clone`; the `Io`
  variant wraps its `std::io::Error` in `Arc`.
- `just ci` now runs `cargo doc` with `RUSTDOCFLAGS="-D warnings"`
  so broken intra-doc links fail the local gate.

### Documentation
- README quick-start rewritten against the shipped API with a
  working, executable pattern.
- `examples/minimal` is a real reference tool (was a stub).
- Concept pages added for `app-context`, `configuration`,
  `error-diagnostics`.
- **Per-crate component pages** in `docs/components/` for every
  shipped crate (rtb-error, rtb-core, rtb-config, rtb-assets,
  rtb-cli, rtb-credentials, rtb-telemetry, rtb-test-support).
  Matches the go-tool-base documentation style; ready for Zensical
  microsite generation.
- `docs/index.md` rewritten as a landing page for the docs tree.
- **Framework spec sections annotated with shipped-vs-deferred
  status.** §8 (built-in commands) calls out which built-ins are
  real and which are `FeatureDisabled` stubs; §9 (VCS), §10 (AI),
  §12.1 (`#[rtb::command]` macro) now carry explicit "deferred to
  v0.X" callouts so readers dropping into the middle of the spec
  see what ships in v0.1 without scrolling to §16. §15 (0.1
  acceptance criteria) marks every bullet ✅ shipped / ⏳ deferred.
- **Stub-crate doc headers normalised.** All seven stubs
  (`rtb-update`, `rtb-vcs`, `rtb-ai`, `rtb-mcp`, `rtb-docs`,
  `rtb-tui`, `rtb-cli-bin`) now lead with `//!` module docs, carry
  an explicit `**Status:** stub awaiting v0.X` line, and share a
  common `#![allow(missing_docs)]` pointer to the framework-spec
  roadmap.
- `rtb-telemetry::Event::attrs` docstring lists explicit
  don't-pass-here categories.
- `rtb-telemetry::TelemetryContextBuilder::salt` docstring
  prescribes the `concat!(CARGO_PKG_NAME, ".telemetry.v1")` pattern.

## [0.1.0] — 2026-04-22

Initial workspace release with seven shipped crates, 151 acceptance
criteria across unit + BDD + trybuild fixtures.

### Added — per crate

- **rtb-error** — typed `Error` enum + `miette` hook installation
  (report handler, panic hook, tool-specific footer).
- **rtb-core** — `App` context, `ToolMetadata` + `bon::Builder`,
  `VersionInfo`, `Features`/`FeaturesBuilder`, `Command` trait,
  `BUILTIN_COMMANDS` `linkme` distributed slice.
- **rtb-config** — `Config<C = ()>` layered over `figment`, with
  `ConfigBuilder` for embedded / user-file / env-prefixed sources
  and atomic `reload` via `arc_swap`.
- **rtb-assets** — overlay filesystem over `rust-embed` + physical
  dirs + in-memory fixtures. Binary last-wins shadowing, YAML/JSON
  deep-merge via `json-patch`.
- **rtb-cli** — `Application::builder` (hand-rolled typestate),
  clap integration, built-in commands (`version`, `doctor`, `init`,
  `config`), feature-gated placeholders for `update`/`docs`/`mcp`.
  `HealthCheck` and `Initialiser` traits with distributed-slice
  registration.
- **rtb-credentials** — `CredentialStore` async trait +
  `KeyringStore` / `EnvStore` / `LiteralStore` / `MemoryStore`,
  precedence-aware `Resolver` (`env > keychain > literal >
  fallback_env`), `SecretString` end-to-end, `CI=true` literal
  refusal.
- **rtb-telemetry** — opt-in `TelemetryContext` + `TelemetrySink`
  async trait + `NoopSink` / `MemorySink` / `FileSink` (JSONL),
  salted SHA-256 machine ID, two-level opt-in policy.

### Added — workspace infrastructure
- Cargo workspace with 15 crates; shared `[workspace.package]`
  metadata, pinned stable toolchain.
- CI workflows: rustfmt, clippy (`-D warnings`), nextest (Linux /
  macOS / Windows), cargo-deny, cargo-doc, cargo-llvm-cov (≥70%
  line coverage gate).
- BDD harness: `cucumber-rs` wired into `cargo test` per crate,
  `tests/features/` + `tests/steps/` convention documented in
  `docs/development/bdd-pattern.md`.
- `just ci` / `just ci-full` local gates.
- Keyring Linux backend defaults to pure-Rust `linux-native`
  (keyutils); reboot-persistent Secret Service storage is an opt-in
  feature (`credentials-linux-persistent`) to keep hermetic
  local dev builds.

### Documented
- Framework-level spec `docs/development/specs/rust-tool-base.md`
  covering every subsystem.
- Per-crate v0.1 specs under `docs/development/specs/2026-04-22-*.md`,
  all marked `IMPLEMENTED`.
- `CLAUDE.md` — agent onboarding + workflow + anti-patterns.
- `docs/about/why-rtb.md`, `docs/about/ecosystem-survey.md`.

[Unreleased]: https://github.com/phpboyscout/rust-tool-base/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/phpboyscout/rust-tool-base/releases/tag/v0.1.0
