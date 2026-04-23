# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Shape

**Rust Tool Base (RTB)** is a batteries-included CLI application framework for Rust, plus a companion `rtb` CLI that scaffolds and regenerates tools built on the framework. It is a sibling of [Go Tool Base (GTB)](https://github.com/phpboyscout/go-tool-base) — same ideology, different ecosystem. **It is not a Go-to-Rust port.** See [`docs/about/why-rtb.md`](docs/about/why-rtb.md) for the paradigm-swap table.

The authoritative contract for every subsystem is [`docs/development/specs/rust-tool-base.md`](docs/development/specs/rust-tool-base.md). When in doubt, that spec wins.

**Before writing any code, read [`docs/development/engineering-standards.md`](docs/development/engineering-standards.md).** It distils the security, correctness, concurrency, documentation, and testing requirements that every contribution — human or agent — follows. Rules in §1 (Security) are non-negotiable; rules in §4 (Documentation) are gated by `just ci`.

## Workflows

Skills under `.claude/skills/rtb-*` are planned but not yet authored. Until they exist, follow the development lifecycle below explicitly.

| Task | Mechanism (today → planned skill) |
|------|-----------------------------------|
| Any development work | Read `docs/development/specs/rust-tool-base.md` first |
| Drafting a new feature specification | Save to `docs/development/specs/YYYY-MM-DD-<feature>.md` with status `DRAFT` → `/rtb-spec` |
| Adding/modifying a library crate | Follow § Library-First below → `/rtb-library-contribution` |
| Defining a new CLI command | Follow § Command Authoring in the spec → `/rtb-command-generation` |
| Pre-commit verification | `just ci` → `/rtb-verify` |
| Resolving clippy findings | § Linting below → `/rtb-lint` |
| Docs-only changes | Touch `docs/` only → `/rtb-docs` |
| Release preparation | § Release below → `/rtb-release` |

## Development Lifecycle

### Step 0: Spec Check (Before Any Implementation)

**Do not write implementation code until this is complete.**

1. Check `docs/development/specs/` for an existing spec matching the feature. Ignore `rust-tool-base.md` for this check — that is the framework spec, not a per-feature spec.
2. Only proceed if the spec status is `APPROVED` or `IN PROGRESS`.
3. **Review open questions.** Every spec ends with an "Open questions" section. Surface unresolved items to the user before writing code. Do not begin implementation until each is answered or explicitly deferred.
4. For **non-trivial features** (new crate, public-API change to a stable crate, scaffolder-template change, architectural change) with no existing spec: draft one, save to `docs/development/specs/YYYY-MM-DD-<feature-name>.md` with status `DRAFT`, pause for human review.
5. For **quick fixes and minor changes** (bug fixes, small internal refactors that don't alter the public API): proceed directly.
6. Update spec status to `IN PROGRESS` when starting, `IMPLEMENTED` when done.

### Implementation (TDD)

- Write failing tests first, derived from the spec's public API, error cases, and edge cases.
- For features with **CLI commands, multi-step user workflows, or service lifecycle coordination**, also add integration tests under `crates/*/tests/` that drive the compiled binary via `assert_cmd` + `insta` snapshots. These are not optional for user-facing behaviour.
- Implement the minimum code to pass. Refactor. Re-run tests.
- **Error handling:** derive `thiserror::Error` + `miette::Diagnostic` on library error enums. Return `miette::Result<T>` from application/command code. Do **not** use `anyhow` inside framework crates (ambiguous provenance, no diagnostic surface). `anyhow` in tests or examples is acceptable.
- **`unsafe_code` is forbidden workspace-wide.** Enforced via `workspace.lints.rust`. If you believe you need `unsafe`, propose a spec amendment instead.
- New `crates/rtb-*` features must have **≥90% line coverage**. Use `cargo llvm-cov` locally.
- Never add `#[allow(clippy::…)]` as a shortcut — always address the root cause. Crate-level `allow`s must be justified in an inline comment and approved by a spec.

### Library-First

New features must be implemented in a `crates/rtb-*` library crate before being surfaced via the built-in CLI or the `rtb` scaffolder. When modifying public APIs that flow into scaffolded tools, also update the templates in `crates/rtb-cli-bin/templates/` and bump the template manifest.

### After Implementation

1. Run `just ci` (fmt-check, clippy, nextest, cargo-deny).
2. If scaffolder output was affected: `just rtb -- new --template default /tmp/smoke-tool`, run `cargo check` in the generated dir, then delete it.
3. Update `docs/components/` and `docs/concepts/` — any functional change **must** include a doc update, cross-referenced with the code.
4. Run `/simplify` on changed files before raising a PR (when the skill is available; until then, self-review with the guardrails in § Architecture).

## Commands

This project uses `just` as the task runner:

```bash
just              # Default: cargo check --workspace --all-targets
just build        # cargo build --workspace --all-targets
just fmt          # cargo fmt --all
just fmt-check    # cargo fmt --all --check
just lint         # cargo clippy --workspace --all-targets -- -D warnings
just test         # cargo nextest (falls back to cargo test) --workspace
just audit        # cargo deny check
just docs         # cargo doc --workspace --no-deps with RUSTDOCFLAGS=-D warnings
just docs-open    # ditto + opens in browser
just rtb -- …     # Run the scaffolder CLI locally (cargo run -p rtb-cli-bin)
just ci           # fmt-check + lint + docs + test + audit + coverage (default features)
just ci-full      # same with --all-features (requires libdbus on Linux)
```

Zensical microsite (docs/ → site/). Assumes `zensical` is on PATH
(`pipx install zensical`):

```bash
just site-build   # build the microsite into ./site/ (clean build)
just site-serve   # local preview at http://127.0.0.1:8000 with hot reload
```

The CI workflow (`.github/workflows/docs.yaml`) installs zensical
from `requirements-lock.txt` into the ephemeral runner per run; local
contributors only need it on PATH.

Run a single test:
```bash
cargo nextest run -p rtb-core -- app::tests::it_clones_cheaply
```

Useful one-offs:
```bash
cargo llvm-cov --workspace --all-features --html     # coverage HTML
cargo udeps --workspace --all-targets                # unused deps
cargo expand -p rtb-cli -- command::deploy           # see macro output
```

## Commit Conventions

All commits must follow [Conventional Commits](https://www.conventionalcommits.org/). Semantic-release uses these to determine version bumps.

**Do not commit without explicit user approval.** Present a summary of changes and a proposed message, then wait for confirmation.

**Do not add AI attribution** — no `Co-Authored-By:` trailers naming an AI, no references to AI assistance in commit messages. The committing developer owns the change entirely.

| Type | Release |
|------|---------|
| `feat(scope):` | Minor |
| `fix(scope):` / `perf(scope):` / `refactor(scope):` | Patch |
| `ci:` / `chore:` / `style:` / `docs:` / `test:` | None |
| `BREAKING CHANGE:` footer | Major |

**Scope is the crate short name**, e.g. `feat(config): add hot-reload subscription`. For repo-wide changes use `workspace`. Each commit represents one coherent change.

## Architecture

### Application Context: `App<C>`

The central pattern is the `App<C: AppConfig>` struct in `rtb-core`. Every command handler receives an `App` by value (cheap — every field is `Arc`-wrapped). Fields:

- `metadata: Arc<ToolMetadata>` — tool name, summary, release source, help channel
- `version: Arc<VersionInfo>` — build-time semver + commit + date
- `config: Arc<Config<C>>` — strongly-typed layered config (see § Configuration)
- `assets: Arc<Assets>` — overlay filesystem (embedded + user override)
- `shutdown: CancellationToken` — root cancellation propagated to every subsystem

`App` is **not** a DI container and is **not** a dynamic property bag. It is a plain struct. Services that need runtime polymorphism (`CredentialStore`, `ReleaseProvider`, `AiClient`) live inside their owning subsystem behind `Arc<dyn …>`, not on `App`.

**Anti-pattern:** porting GTB's `Props` struct verbatim, a `HashMap`-backed "service locator", or a framework-wide `dyn Any` container. See [Appendix B of the spec](docs/development/specs/rust-tool-base.md#appendix-b--explicit-anti-patterns).

### Command Architecture (clap)

Commands are `clap`-driven. `rtb-cli::Application::builder()` is a typestate builder (`bon`) that takes metadata, config type, assets, and a list of `Command` trait objects, installs `tracing`/`miette`/`tokio` plumbing, and returns an `Application::run().await` entry point.

**Runtime feature flags** (`rtb_core::Features`) control which built-in commands are active for a given invocation — orthogonal to Cargo features, which control what is compiled in:

```rust
Application::builder()
    .features(Features::builder()
        .disable(Feature::Init)
        .enable(Feature::Ai)
        .build())
```

Default-enabled runtime features: `Init`, `Version`, `Update`, `Docs`, `Mcp`, `Doctor`.
Default Cargo features on `rtb`: `cli`, `update`, `docs`, `mcp`, `credentials`.

### API Stability

Pre-1.0 (0.x), `rtb-*` APIs are not yet frozen. After 1.0:

- **No breaking changes** to the public API of any `rtb-*` crate without a major version bump.
- Before modifying any public type, trait, function signature, or exported const, check its stability tier (documented in each crate's `lib.rs`).
- Unavoidable breaking changes require: (1) justification in the commit body, (2) a `BREAKING CHANGE:` footer, (3) a migration entry in `docs/migration/`.
- Deprecations must use `#[deprecated(since = "x.y.z", note = "…")]` and survive at least one minor release before removal.
- Use `cargo public-api diff` to verify no unintended breaks before merging.
- Any item under a module named `internal` or behind a `#[doc(hidden)]` or `unstable-*` Cargo feature is exempt.

Binary entry points: `crates/rtb-cli-bin/src/main.rs` (the `rtb` scaffolder) and `examples/minimal/src/main.rs` (reference tool).

### Configuration

`rtb-config` wraps `figment` with strongly-typed, layered config. Precedence (last-wins): embedded defaults → user files → env vars (`<TOOL>_*`) → CLI flags.

**Typed, not dynamic.** Downstream tools declare a `#[derive(Deserialize, JsonSchema)]` struct and parameterise `App<C>` on it. There is no `GetString("foo.bar")` API. Hierarchical access uses nested structs; profile selection uses `figment::select`.

Hot reload: `notify-debouncer-full` watches files, `arc_swap::ArcSwap` swaps the parsed value, subscribers use `config.subscribe() -> watch::Receiver<Arc<C>>`.

### AI Chat Client

`rtb-ai` unifies providers (Claude, Claude Local, OpenAI, OpenAI-compatible, Gemini, Ollama) through `genai`. The `Claude` backend additionally drops down to direct `reqwest` calls against the Anthropic Messages API for features `genai` does not surface (prompt caching, extended thinking, managed agents, citations).

Rust code that imports `anthropic`/`async-openai`/`genai` should default to **Claude 4.7** models and include prompt caching at every stable point (system prompt, tools, static context). Where migration is needed: Opus 4.6 → Opus 4.7, Sonnet 4.5 → Sonnet 4.6, Haiku 4.5 → Haiku 4.5 (current).

Structured output uses `schemars`-generated JSON Schema in the request and `jsonschema` validation on the response before deserialising.

### Service Lifecycle

No `Controller`/`Services` type is defined. Use `tokio::task::JoinSet` with a `CancellationToken` (child of `App::shutdown`). For tiered shutdown, reach for the `tokio_graceful_shutdown` crate. See [spec §11](docs/development/specs/rust-tool-base.md#11-concurrency--lifecycle).

`rtb-cli::services::run_services(tasks, token)` is the only helper — it is a ten-line convenience, not an abstraction.

### Error Handling

Every public crate derives `thiserror::Error` + `miette::Diagnostic`. Application entry points return `miette::Result<()>`. `rtb-cli::Application::run()` installs `miette::set_hook` (with `ToolMetadata::help` appended) and `miette::set_panic_hook`. There is **no** `ErrorHandler.Check()` funnel — errors are values, propagated with `?`, reported once at the edge.

For ad-hoc hints: `miette::miette!(help = "…", code = "…", "{}", reason)`.

### Version Control (VCS)

`rtb-vcs` abstracts GitHub (`octocrab`, Enterprise-capable, GitHub-App-capable) and GitLab (`gitlab` crate, self-hosted + nested groups). Git operations use `gix` primary, `git2` as feature-gated fallback.

The `ReleaseProvider` trait is the pluggable interface; implementations are selected by `ToolMetadata::release_source` and wrapped in `Arc<dyn ReleaseProvider>`. Downstream tools never import `octocrab` or `gitlab` directly.

### Setup & Bootstrap

`rtb-cli`'s built-in `init` command invokes `Initialiser` trait objects registered via `linkme::distributed_slice(BUILTIN_INITIALISERS)`. Each initialiser declares `name()`, `is_configured(&config)`, and `configure(&app) -> Result<()>`. First-run bootstrap writes merged defaults, prompts for auth, optionally stores tokens in the OS keychain via `rtb-credentials`.

### TUI Components

`rtb-tui` provides reusable widgets — `Wizard` (multi-step form built on `inquire`, Escape-to-back via `InquireError::OperationCanceled`), tables (`tabled`), spinners (`console`). `rtb-docs` implements the interactive markdown browser (`ratatui` + `termimad`) with optional streaming AI Q&A (gated on the `ai` feature).

### Code Generation (scaffolder)

`crates/rtb-cli-bin/` produces the `rtb` binary — the Rust analogue of `gtb`. It uses `minijinja` templates (runtime, Jinja2-compatible) stored in `crates/rtb-cli-bin/templates/` and a `.rtb/manifest.toml` in generated projects as the source of truth for regeneration.

**Template-security guardrails** (mirrors GTB's `template_escape.go`): every user-influenced field that flows into a template is NFC-normalised, validated against a field-specific character class, and run through the appropriate escaper at non-code render sites (`escape_yaml`, `escape_toml`, `escape_markdown`, `escape_shell_arg`). When adding a new user-facing field, update `crates/rtb-cli-bin/src/validate.rs` and add an entry to `docs/development/template-security.md`.

### Testing

- **Test runner:** `cargo nextest` (faster, better isolation). Falls back to `cargo test`.
- **Snapshots:** `insta` for fixture-heavy assertions (CLI output, rendered markdown, generated scaffolds). Review snapshots with `cargo insta review`.
- **Test doubles:** prefer dependency injection via generics or `Arc<dyn Trait>` over macro-based mocks. Where a mock crate genuinely helps, use `mockall`.
- **No global mutable state for testing.** Racing `static mut` or `OnceLock`-reset tricks under nextest's parallel execution is forbidden. Inject dependencies through `App`, builders, or `Config` fields.
- **Integration tests** gate via Cargo features, not env vars. Add `required-features = ["integration"]` to `[[test]]` entries. CI runs a matrix with `--features integration` on the crates that need it.
- **E2E CLI tests** use `assert_cmd` + `insta` against the compiled `rtb` or example tool binary. Snapshots capture stdout, stderr, and exit code. Tests live in `crates/*/tests/e2e/`.
- **New CLI commands or lifecycle changes must include an `assert_cmd` scenario.**
- Test loggers: use `tracing_subscriber::fmt().with_test_writer()` or the `tracing-test` crate. Do not install the subscriber globally in tests.

### URL Opening

All URL-opening must route through `rtb-cli::browser::open_url`. Do not call `open::that`, `webbrowser::open`, or `std::process::Command::new("xdg-open")` directly. `rtb-cli::browser` enforces a scheme allowlist (`https`, `http`, `mailto`), a URL-length bound, and control-character rejection before invoking the OS handler. Callers constructing `mailto:` URLs from user-influenced data must additionally `urlencoding::encode` every parameter value.

### Regex Compilation

Any `regex::Regex::new` (or `regex::RegexBuilder`) call whose pattern originates outside the binary (config file, CLI flag, TUI input, HTTP payload, message queue) must route through `rtb_core::regex_util::compile_bounded`. The helper sets `RegexBuilder::size_limit(1 MiB)` and `dfa_size_limit(8 MiB)` to cap memory, and rejects patterns longer than 1 KiB. Rust's `regex` is already time-safe (Thompson NFA, linear time), so no compile timeout is needed — unlike GTB's Go counterpart.

Literal patterns known at build time may use `regex::Regex::new` directly or, preferably, `once_cell::sync::Lazy<Regex>`.

### AI Provider Endpoints

`AiClient::Config::base_url` values must pass `rtb_ai::validate_base_url`. The validator rejects non-HTTPS schemes, URLs containing userinfo (`user:pass@host`), and placeholder hosts (`example.com` and subdomains). Tests targeting a `wiremock` or `httpmock` server set `Config::allow_insecure_base_url: true`; that field is `#[serde(skip)]` so config files cannot downgrade HTTPS enforcement. Every successful `AiClient::new` call logs the endpoint hostname at INFO — never the path or query.

### Credential Redaction

Use `rtb_core::redact` for any free-form string written to telemetry, distributed logs, or a third-party observability surface. `redact::string` strips URL userinfo, common credential query parameters, Authorization headers, well-known provider prefixes (`sk-`, `ghp_`, `AIza`, `AKIA`, Slack, Anthropic `sk-ant-…`), and very long opaque tokens. `rtb-telemetry` applies it automatically to `args` and `err_msg`; `rtb-cli`'s HTTP middleware uses `redact::SENSITIVE_HEADERS` to redact headers at DEBUG.

### Credential Storage

User-supplied secrets (AI API keys, VCS tokens) are stored via one of three modes selected by the `init` wizard:

1. **Env-var reference** (recommended default) — config references `$MYTOOL_ANTHROPIC_API_KEY`.
2. **OS keychain** (opt-in) — `rtb-credentials` feature enabled in the tool's `Cargo.toml`.
3. **Literal in config** (legacy). Refused under `CI=true`.

Resolution precedence at runtime: `{provider}.api.env` → env var → `{provider}.api.keychain` → `{provider}.api.key` literal → well-known fallback env var (`ANTHROPIC_API_KEY`, `OPENAI_API_KEY`, `GITHUB_TOKEN`, …).

All resolved secrets cross the boundary as `secrecy::SecretString`. `Debug` renders `[REDACTED]`; memory is zeroed on drop via `zeroize`. Never log or format a `SecretString` without first calling `expose_secret()` — and never log the exposed form.

The `doctor` command's `credentials::no-literal` check warns when any literal credential is present in config.

Keychain support is activated by the downstream tool's `Cargo.toml` enabling the `credentials` feature on `rtb`; regulated downstreams omit it, and link-time dead-code elimination keeps `keyring` and its transitive deps out of the binary.

## Linting

Configuration is in the workspace root `Cargo.toml` under `[workspace.lints]`:

- `unsafe_code = "forbid"`
- `missing_docs = "warn"`
- `clippy::pedantic`, `clippy::nursery`, `clippy::cargo` at `warn`
- A small allow-list for pragmatism (`module_name_repetitions`, `missing_errors_doc`, `missing_panics_doc`, `multiple_crate_versions`)

`cargo clippy --workspace --all-targets --all-features -- -D warnings` in CI.

**Lint resolution order** (simplest to most complex): `unused_*` / `dead_code` → `clippy::correctness` → `clippy::perf` → `clippy::pedantic` → `clippy::nursery`. Run tests after every structural fix.

`cargo-deny` enforces the allow-listed licences and advisory-db checks (`deny.toml`). Run `just audit` before raising a PR.

## Release

Releases are driven by `cargo-dist` (now named `dist`) for artefact builds and `cargo-release` for version bumps + tagging. Do not manually edit crate versions mid-merge.

- Binaries ship for `darwin-{aarch64,x86_64}`, `linux-{aarch64,x86_64,musl}`, `windows-{aarch64,x86_64}`.
- Release archives are signed with an Ed25519 key and have accompanying SHA-256 `.sum` files. The `update` subsystem verifies both before calling `self-replace`.
- `CHANGELOG.md` follows [Keep a Changelog](https://keepachangelog.com/).
- Pre-release: `just ci`, then `dist build --target=<host-triple>` locally to verify artefact shape.

`crates.io` publication is dependency-ordered (`rtb-error` → `rtb-core` → leaves → `rtb` umbrella → `rtb-cli-bin`). The `release.yaml` workflow handles the ordering.

## Anti-patterns (quick reference)

If you find yourself reaching for any of the following, stop and consult [Appendix B of the spec](docs/development/specs/rust-tool-base.md#appendix-b--explicit-anti-patterns):

- `Props`-style grab-bag struct with `Box<dyn Any>` fields
- Functional options (`fn with_logger(&mut self, …)` variadic-style)
- Package-level `OnceLock`-guarded registries instead of `linkme::distributed_slice`
- `context.Context` threaded through APIs (use `CancellationToken`)
- `ErrorHandler.check(err)` calls (return `miette::Result`, propagate with `?`)
- `config.get_string("foo.bar")` dynamic accessors (declare a `serde::Deserialize` struct)
- `anyhow` in library crates (use `thiserror` + `miette::Diagnostic`)
- `unwrap()` / `expect()` on error paths outside tests and examples
- Two-file command splits (`command.rs` + `impl.rs`) — use one module, split by size
- `Arc<Mutex<T>>` for read-mostly state — use `Arc<T>` + `ArcSwap` or `watch::Receiver`
