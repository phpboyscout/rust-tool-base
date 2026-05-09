---
title: rtb-cli ops subtree v0.1 (slice 2 of v0.4)
status: IMPLEMENTED
date: 2026-05-06
authors: [Matt Cockayne]
crate: rtb-cli
supersedes: null
---

# `rtb-cli` ops subtree v0.1 — `credentials` / `telemetry` / `config` extension / `--output`

**Status:** IMPLEMENTED — landed on `feat/rtb-cli-ops-v0.1` over seven internal commits.

**Caveats vs the spec body:**

- ~~`config get / set / validate` operate against the canonical user-file (`<config_dir>/<tool>/config.yaml`) as a `serde_json::Value` rather than through `Config<C>`. `config schema` errors with help-laden text until `App<C>` lands.~~ **Closed in v0.4.1** ([2026-05-09-v0.4.1-scope.md](2026-05-09-v0.4.1-scope.md)). Tools that opt into `Application::builder().config<C>(...)` get the schema-aware paths: `show` renders the merged typed value as YAML, `get` reads JSON-pointer paths against the merged value, `schema` prints the JSON Schema for `C`, `validate` validates the merged value (or a `--config-file` override) against the schema, and `set` validates the post-write merged value before persisting. The v0.4 untyped fallback path still works unchanged for tools that don't call `.config(...)`.
- `credentials add` / `remove` interact with the OS keychain via `KeyringStore::new()` directly rather than through a `dyn CredentialStore` injected on `App`. Sufficient for v0.4; store-injection is a v0.5+ ergonomic enhancement.
- The `Application::builder().read_telemetry_consent()` builder step the spec sketched is implemented inside the `telemetry` subtree itself (the resolution chain runs at `telemetry status` time) rather than being threaded through `TelemetryContext` at construction. Tools wiring telemetry collection re-read the consent file via `rtb_telemetry::consent::read` at startup.
**Parent contract:** v0.4 scope addendum [`2026-05-06-v0.4-scope.md`](2026-05-06-v0.4-scope.md), §2.2 – §2.5 and §4.1.
**Depends on:** [`rtb-tui` v0.1](2026-05-06-rtb-tui-v0.1.md) — `Wizard`, `render_table`, `render_json`, `Spinner`. Slice 2 is gated on slice 1 landing first.

---

## 1. Goal

Close the day-to-day operations loop for a tool's *users*: add and inspect credentials, opt into telemetry, read and mutate config — without leaving the CLI. v0.3 made every RTB tool an MCP server and an AI client; v0.4 makes the operator surface match.

Concretely, this slice ships:

1. **`credentials`** subtree — `list / add / remove / test / doctor`. Backed by `rtb-credentials::CredentialStore` and a new `CredentialBearing` trait downstream tools implement on their config.
2. **`telemetry`** subtree — `status / enable / disable / reset`. Backed by a new persisted-consent file at `<config_dir>/<tool>/consent.toml`. `Application::builder` reads it at startup and threads the resulting `CollectionPolicy` into the `TelemetryContext` it builds.
3. **`config`** subtree extension — `get / set / schema / validate`. `config show` already ships at v0.1; the new leaves need `Config::schema()` and `Config::write()` on the `rtb-config` side, gated behind a new `mutable` feature.
4. **Global `--output text|json` flag** — declared once at the top of the clap tree with `Arg::global(true)`, propagating to every subcommand. A new `rtb_cli::render` module wraps `rtb_tui::render_table` / `render_json` so every consumer goes through one path.
5. **`CredentialBearing` trait** — the introspection seam (`§4.1`) that lets `credentials list / test / doctor` enumerate the `CredentialRef` fields in a downstream tool's config without runtime schema-walking.

What this slice explicitly does **not** ship: a `#[derive(CredentialBearing)]` proc-macro (deferred to v0.5 per O1 resolution), a `config edit` subcommand (deferred per v0.4 scope §3), telemetry retroactive backfill, exit-code conventions overhaul.

## 2. Public API surface

### 2.1 `Feature::Credentials`

A new runtime feature variant on `rtb_app::Features`:

```rust
#[non_exhaustive]
pub enum Feature {
    // … existing …
    Credentials,
}
```

Default-enabled set gains `Credentials`. The `credentials` subtree registers when the runtime flag is on AND the existing `credentials` Cargo feature on `rtb` is compiled in.

### 2.2 `CredentialBearing` (in `rtb-credentials`)

```rust
/// Downstream tools implement this on their `Config<C>` type so
/// `rtb-cli`'s credentials subtree can enumerate the configured
/// `CredentialRef`s without schema-walking.
pub trait CredentialBearing {
    /// Yield `(name, &CredentialRef)` pairs for every credential
    /// the merged config knows about. The `name` is the
    /// human-friendly identifier surfaced by `credentials list`
    /// and accepted as the argument to `credentials add / remove
    /// / test`.
    fn credentials(&self) -> Vec<(&'static str, &CredentialRef)>;
}

/// Blanket impl for `()` — tools that haven't typed their config
/// yet still build. `credentials list` reports an empty set.
impl CredentialBearing for () {
    fn credentials(&self) -> Vec<(&'static str, &CredentialRef)> {
        Vec::new()
    }
}
```

`App<C: CredentialBearing>` exposes `App::credentials() -> Vec<(&'static str, &CredentialRef)>` that delegates. The trait is `pub` from `rtb-credentials`; `rtb-app` re-exports it from its prelude.

### 2.3 `OutputMode` and `rtb_cli::render`

```rust
/// Output rendering mode for any subcommand that prints structured
/// data. Parsed from the global `--output text|json` flag.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum, Default)]
pub enum OutputMode {
    #[default]
    Text,
    Json,
}

/// Render `rows` per `mode` and write to stdout. Wraps
/// [`rtb_tui::render_table`] and [`rtb_tui::render_json`] so every
/// rendering site goes through one path.
///
/// # Errors
///
/// Surfaces [`rtb_tui::RenderError`] in JSON mode; text mode is
/// infallible.
pub fn output<R>(mode: OutputMode, rows: &[R]) -> Result<(), RenderError>
where
    R: tabled::Tabled + serde::Serialize;
```

The flag is declared once at the top-level clap with `global = true`:

```rust
clap::Arg::new("output")
    .long("output")
    .global(true)
    .value_parser(clap::value_parser!(OutputMode))
    .default_value("text")
```

clap propagates it to every subcommand automatically — both `mytool --output json subcommand` and `mytool subcommand --output json` parse identically.

### 2.4 `credentials` subtree

```text
mytool credentials list
mytool credentials add    <ref-name>
mytool credentials remove <ref-name>
mytool credentials test   <ref-name>
mytool credentials doctor
```

| Subcommand | Outcome |
|---|---|
| `list` | Walk `App::credentials()`. For each ref, report `service / account / mode / status`. `mode` is one of `env / keychain / literal / fallback-env`; `status` is one of `resolved / missing / refused-in-ci`. Dual-mode (text/JSON). |
| `add <ref-name>` | Refuses unknown names. Refuses refs whose `CredentialRef` declares only a literal layer — adding a layer the config doesn't declare invites resolve-time surprises. Otherwise drives a 2-step `Wizard` (storage mode `env` / `keychain` → secret). Writes to the underlying store; never echoes the secret. Idempotent — re-adding overwrites. |
| `remove <ref-name>` | Refuses unknown names. Removes from the underlying store. Keychain-only; on a literal-mode credential, **exits non-zero** with a clear "edit your config file" diagnostic — operators should explicitly know the literal isn't being touched (no silent skip). |
| `test <ref-name>` | Refuses unknown names. Calls `Resolver::resolve` and reports which precedence step succeeded (`env > keychain > literal > fallback_env`). Never prints the secret. |
| `doctor` | Aggregates per-credential `test` calls into a `tabled` summary. Also exposed as a `HealthCheck` (`credentials::resolve`) so `mytool doctor` picks it up. |

`add` is the only subcommand that drives a `Wizard`. The other four are non-interactive — they print and exit.

### 2.5 `telemetry` subtree

```text
mytool telemetry status
mytool telemetry enable
mytool telemetry disable
mytool telemetry reset
```

The persisted consent file lives at `<ProjectDirs::config_dir()>/<tool>/consent.toml`:

```toml
# Schema version for forward-compatibility.
version = 1
# One of: "enabled" | "disabled" | "unset".
state = "enabled"
# Optional ISO-8601 timestamp; written on every state change.
decided_at = "2026-05-08T12:34:56Z"
```

| Subcommand | Outcome |
|---|---|
| `status` | Print state + decided-at + consent-file path. Dual-mode. |
| `enable` | Refuses under `CI=true` — operators enabling telemetry interactively want a real prompt; a build pipeline silently flipping it on is the wrong default (mirrors the existing literal-credential CI guard). Otherwise writes `state = "enabled"` and prints [`ToolMetadata::telemetry_notice`] verbatim if set; otherwise a generic "telemetry enabled" line. |
| `disable` | Write `state = "disabled"`. |
| `reset` | Remove the consent file. State reverts to `unset`. |

`Application::builder` reads the file at startup, parses it with `figment`, and threads the resulting `CollectionPolicy` into the `TelemetryContext` it builds. When the file is `unset` or unreadable, the policy is `Disabled` — opt-in remains the default.

A new optional field on `ToolMetadata`:

```rust
pub struct ToolMetadata {
    // … existing …
    /// Privacy notice printed when the user runs `telemetry enable`.
    /// `None` falls back to a generic message.
    #[serde(default)]
    #[builder(default)]
    pub telemetry_notice: Option<&'static str>,
}
```

Additive — existing builders inherit `None`.

### 2.6 `config` subtree extension

`config show` already ships. v0.4 adds:

```text
mytool config get      <jsonpath>
mytool config set      <jsonpath> <value>
mytool config schema
mytool config validate [--file PATH]
```

| Subcommand | Outcome |
|---|---|
| `get <jsonpath>` | Resolve a JSON-pointer path against the merged typed config and print the value. Dual-mode. Refuses paths the schema doesn't know about (early "no such field" diagnostic rather than `null`). |
| `set <jsonpath> <value>` | Parse `<value>` as JSON (with a string-fallback for bare strings), write it to the canonical user-file path `<config_dir>/<tool>/config.yaml`, with `--config-file PATH` to override. **Accepts full subtree replacements** at any path — symmetric with `config get` and the schema validation runs either way. Validates the merged result against `Config::schema()` before writing; a write that would invalidate the config is refused with a structured error pointing at the offending field. |
| `schema` | Print `Config::schema()` (the `serde_json::Value` produced by `schemars::schema_for!(C)`). Dual-mode. |
| `validate [--file PATH]` | Validate a candidate config — defaults to the merged result; with `--file`, validates the file contents only. Exits non-zero on any violation. |

The `set` and `schema` subcommands need new APIs on `rtb-config`:

```rust
impl<C: schemars::JsonSchema + Serialize + DeserializeOwned> Config<C> {
    /// Return the JSON Schema for `C`. Used by `config schema` and
    /// `config get / set` for path validation.
    #[must_use]
    pub fn schema() -> serde_json::Value;

    /// Write the merged value back to the canonical user-file path,
    /// or to `path` when supplied. The serialised form is YAML when
    /// the path ends in `.yml` / `.yaml`, TOML when `.toml`, JSON
    /// otherwise.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::Write`] on I/O or serialisation failure;
    /// [`ConfigError::Schema`] when the candidate value fails to
    /// validate against `Config::schema()`.
    pub fn write(&self, path: Option<&Path>) -> Result<(), ConfigError>;
}
```

Both methods live behind a new `mutable` Cargo feature on `rtb-config` so tools that don't need `config set` (the most common case at v0.1) don't pull in `schemars`'s runtime weight.

The `mutable` feature is default-on for the `rtb` umbrella's `cli` feature so `rtb-cli`'s `config set` works out of the box.

## 3. Behavioural contract

### 3.1 Credential listing precedence

`credentials list` reports the precedence layer the resolver *would* hit, not the raw `CredentialRef` shape. For a ref like:

```yaml
anthropic:
  api:
    env: MYTOOL_ANTHROPIC_API_KEY
    keychain:
      service: mytool
      account: anthropic
    fallback_env: ANTHROPIC_API_KEY
```

…the listing reflects current resolution: if `MYTOOL_ANTHROPIC_API_KEY` is set, mode `env` and status `resolved`. If not, but the keychain has a secret, mode `keychain` and status `resolved`. If both are empty but `ANTHROPIC_API_KEY` is set, mode `fallback-env` and status `resolved`. If nothing matches, mode `<first-configured-layer>` and status `missing`. Under `CI=true`, a literal layer reports status `refused-in-ci` regardless of presence.

This matches the resolver's actual behaviour and gives operators a one-liner answer to "where is this credential coming from?"

### 3.2 Telemetry consent precedence

The runtime resolution chain for `CollectionPolicy`:

1. Hardcoded compile-time disable: when the `telemetry` Cargo feature on `rtb` is off, the policy is unconditionally `Disabled`. The subcommand exits with a `FeatureDisabled` diagnostic.
2. Consent file: `<config_dir>/<tool>/consent.toml`. State `enabled` → `Enabled`; state `disabled` → `Disabled`; state `unset` or file missing → step 3.
3. `MYTOOL_TELEMETRY` env var: `1` / `true` / `on` → `Enabled`; `0` / `false` / `off` → `Disabled`; absent → step 4.
4. Default: `Disabled`.

`telemetry status` reports which step decided the current state.

### 3.3 `--output` honour list

Subcommands that produce structured data:

- `version`, `doctor`, `config show / get / schema`, `update check`, `docs list`, `mcp list`, `credentials list / test / doctor`, `telemetry status`.

Subcommands that ignore `--output`:

- `init`, `update run`, `docs show / browse / serve / ask`, `mcp serve`, `credentials add / remove`, `telemetry enable / disable / reset`, `config set / validate`.

A subcommand that ignores `--output` does so silently — the flag parses successfully and is just unused.

**`mcp list` normalisation.** v0.3's `mcp list` shipped a one-JSON-object-per-line (NDJSON-style) form before the global flag existed. Slice 2 normalises it onto the global `--output` contract:

- `--output text` (default) — `tabled` summary, one row per registered tool.
- `--output json` — single JSON array, pretty-printed; consistent with every other dual-mode subcommand.

The NDJSON behaviour was a v0.3 expedient. Operators that scripted against it migrate by piping through `jq -s` (slurp-array) — a one-liner. The v0.4 release notes call this out as a behaviour change.

### 3.4 `Application::builder` glue

Two new builder steps in `rtb-cli`, both opt-in:

```rust
impl<M, V, A, F> ApplicationBuilder<M, V, A, F> {
    /// Read the persisted consent file and thread the resulting
    /// `CollectionPolicy` into the `TelemetryContext`. No-op when
    /// the `telemetry` Cargo feature is off. Default: not called
    /// (consent is `Disabled` until a tool opts in).
    pub fn read_telemetry_consent(self) -> Self;

    /// Register the credentials/telemetry/config subtree commands.
    /// Default: called automatically when the relevant runtime
    /// `Feature` is enabled. Tools that disable a runtime feature
    /// see no subtree.
    pub fn ops_subtrees(self) -> Self;
}
```

`Application::run` wires both automatically; tools that want to disable a subtree do so via the existing `Features::disable(Feature::X)` pattern.

## 4. Cross-cutting changes

- **`rtb-app`** — adds `Feature::Credentials`, `ToolMetadata::telemetry_notice`, re-exports `CredentialBearing` from the prelude.
- **`rtb-credentials`** — adds the `CredentialBearing` trait. No change to `CredentialStore`.
- **`rtb-config`** — adds `Config::schema()` and `Config::write()`, both behind the new `mutable` Cargo feature. Adds `ConfigError::Write` and `ConfigError::Schema` variants.
- **`rtb-telemetry`** — adds a `consent` module: `read(path) -> Option<CollectionPolicy>`, `write(path, state)`, `Consent` struct with `state` + `decided_at`. The `Application::builder` glue lives in `rtb-cli`.
- **`rtb-cli`** — three new modules (`credentials.rs`, `telemetry.rs`, extended `config.rs`), one new utility module (`render.rs`), the global `--output` flag declaration. Subcommands register via `BUILTIN_COMMANDS`.
- **`rtb`** — `mutable` flips on as part of the `cli` feature. No new umbrella features.
- **Examples.** `examples/minimal` gains:
  - A `MyConfig` struct that implements `CredentialBearing` for an `anthropic.api: CredentialRef` field.
  - `MemoryStore` injection via a test-only `App::for_testing` extension so `credentials add` round-trips without touching the OS keychain.
  - Smoke tests covering `credentials --help`, `credentials list`, `credentials add anthropic` (under `MemoryStore`), `telemetry --help`, `telemetry status` / `enable` / `disable`, `config get / schema / validate`, and the global `--output` flag.

## 5. Acceptance criteria (TDD)

Per-subtree, with shared cross-cutting checks at the bottom.

### 5.1 `--output` flag and `rtb_cli::render`

- **T1** — Top-level `--output text` is the default; explicit construction via `OutputMode::Text` and `OutputMode::Json` parses round-trip.
- **T2** — `mytool --output json subcommand` and `mytool subcommand --output json` both parse to `OutputMode::Json` (clap `global = true` propagation).
- **T3** — `rtb_cli::render::output(mode, rows)` writes `render_table(rows)` for `Text`, `render_json(rows)?` for `Json`. Trailing newline preserved.

### 5.2 `credentials` subtree

- **T4** — `credentials list` walks `App::credentials()` and emits one row per ref, mode + status reflecting current resolver behaviour. Dual-mode round-trips: same row count text vs JSON.
- **T5** — `credentials list` against a config with no credentials emits an empty table (text) or `[]\n` (JSON) — exit code 0, no error.
- **T6** — `credentials add <ref>` for an unknown ref name returns `CredentialError::UnknownRef` with the name attached.
- **T7** — `credentials add <known-ref>` drives a 2-step `Wizard` (mode → secret) and writes to the configured store. Re-adding overwrites without prompting for confirmation.
- **T8** — `credentials remove <known-ref>` deletes from the store. Calling `remove` on a literal-mode ref returns a clear "edit your config file" error.
- **T9** — `credentials test <known-ref>` reports the precedence layer that resolved (or `missing` when none does). Never prints the secret.
- **T10** — `credentials doctor` aggregates per-ref `test` results into a single `tabled` summary; exit non-zero when any ref is `missing`.

### 5.3 `telemetry` subtree

- **T11** — `telemetry status` against a missing consent file reports `state = unset, decided_at = -, source = default`.
- **T12** — `telemetry enable` writes the consent file, prints the `telemetry_notice` from `ToolMetadata` verbatim when set; otherwise a generic line.
- **T13** — `telemetry disable` writes the consent file; subsequent `status` reports `state = disabled, source = consent-file`.
- **T14** — `telemetry reset` removes the file; subsequent `status` reports `state = unset`.
- **T15** — `Application::builder().read_telemetry_consent()` threads the file's state into `TelemetryContext::policy`. `MYTOOL_TELEMETRY=1` env override beats a `disabled` file (precedence chain §3.2).

### 5.4 `config` subtree extension

- **T16** — `config get .anthropic.api.env` against a config with that field set returns the value (text and JSON forms).
- **T17** — `config get .does.not.exist` errors out with a "no such field" diagnostic (path-validates against `Config::schema()`).
- **T18** — `config set .anthropic.timeout 30` writes the merged result to the canonical user-file path; subsequent `config get` returns `30`.
- **T19** — `config set .anthropic.timeout "abc"` errors out — schema validation refuses the write (timeout is `u32`).
- **T20** — `config schema` prints valid JSON that round-trips through `serde_json::from_str`.
- **T21** — `config validate --file=<bad>` exits non-zero with a structured error pointing at the offending field.

### 5.5 Cross-cutting

- **T22** — `Feature::Credentials` defaults to enabled; `Features::disable(Feature::Credentials)` removes every `credentials *` command from the clap tree.
- **T23** — `ToolMetadata::telemetry_notice` defaults to `None`; existing `ToolMetadata::builder` chains compile unchanged.
- **T24** — `CredentialBearing` blanket-impl for `()` makes `App<()>` compile without any per-tool work.
- **T25** — `ConfigError::Write` and `ConfigError::Schema` are `Clone`, `thiserror::Error`, `miette::Diagnostic` — same shape as every other RTB error enum.

BDD scenarios:

- **S1** — *Given* a tool with one credential ref configured for env-var resolution, *When* the user runs `credentials add` and writes to the keychain instead, *Then* the next `credentials test` reports the keychain layer as the resolver source.
- **S2** — *Given* a fresh tool install, *When* the user runs `telemetry enable`, *Then* a subsequent process that calls `App::telemetry().record(...)` actually emits an event (i.e. the consent file flowed through to the runtime policy).
- **S3** — *Given* a config with a typed `timeout: u32` field, *When* the user runs `config set .timeout 30` then restarts the tool, *Then* the merged `Config::timeout` is `30`.

## 6. Resolutions

All five open questions resolved 2026-05-06. Recorded here for the
audit trail — the spec body above carries the live behaviour.

- **C1 — `credentials remove` on a literal-mode credential.** Resolved
  as **hard failure** (exit non-zero with an "edit your config file"
  diagnostic). Silent skip would let operators believe the literal
  was removed when it wasn't — a worse failure mode than the loud
  refusal.
- **C2 — `config set` value shape.** Resolved as **accept full
  subtree replacements** at any path. Symmetric with `config get`,
  and refusing it adds complexity for limited safety gain — schema
  validation runs either way.
- **C3 — `telemetry enable` under `CI=true`.** Resolved as **refuse**.
  Mirrors the literal-credential CI guard. Operators enabling
  telemetry interactively want a real prompt; a build pipeline
  silently flipping it on is the wrong default.
- **C4 — `mcp list` JSON shape under the new global flag.** Resolved
  as **normalise to JSON array**. The v0.3 NDJSON form was an
  expedient before the global `--output` flag existed. Consumers
  scripting against the old form migrate by piping through `jq -s`.
- **C5 — `credentials add` for a literal-only `CredentialRef`.**
  Resolved as **error** — adding a layer the config doesn't declare
  invites surprises at resolution time. Operators that want a
  keychain override edit the config to declare the layer first.

## 7. Slicing

Single PR for the whole subtree (per v0.4 scope §7). Internal commit ordering on the branch:

1. `feat(credentials): CredentialBearing trait` — pure addition, builds independently.
2. `feat(config): mutable feature with Config::schema and Config::write` — same.
3. `feat(app): Feature::Credentials + ToolMetadata::telemetry_notice` — additive trait/struct changes.
4. `feat(telemetry): consent module + read/write` — same.
5. `feat(cli): render module + global --output flag` — depends on rtb-tui.
6. `feat(cli): credentials/telemetry/config subtrees` — the user-facing slice. Pulls in everything above.
7. `test(minimal): smoke coverage for new subtrees` — example wiring + tests.

Each commit ships green on its own (`cargo test --workspace`).

## 8. Approval gate

This spec is **APPROVED** as of 2026-05-06. The slice is *implemented*
when **(a)** T1–T25 + S1–S3 land green with **≥ 90% line coverage**
on the touched crates, **(b)** `examples/minimal` smoke gains the
cases in §4, **(c)** §16 of the framework spec gains an "0.4 (slice 2)
— `rtb-cli` ops subtree" entry once the PR merges, **(d)** the spec
status above flips to `IMPLEMENTED`.
