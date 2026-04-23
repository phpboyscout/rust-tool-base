---
title: Engineering standards
description: Standing security, correctness, documentation, and testing requirements for every RTB contribution.
status: active
authors: [Matt Cockayne]
---

# Engineering standards

These are the standing rules every contribution — human or agent —
follows. They distil lessons from the v0.1 critical review plus the
patterns the first seven crates validated.

**This document is normative.** CLAUDE.md links here; every new agent
context should load this before writing code. If a rule below
conflicts with something in a crate spec or `rust-tool-base.md`, the
more recent document wins and the other should be updated.

---

## 1. Security

### 1.1 Path handling

**Any function that joins a caller-supplied path onto a base MUST
reject traversal lexically.**

```rust
// WRONG — path may contain `..` and escape the base.
let resolved = base.join(user_path);
fs::read(resolved)

// RIGHT — lexical check rejects absolute paths, `..`, root
// components, and Windows prefixes. No `canonicalize()` (target may
// not exist; symlink-following is a caller concern).
let resolved = safe_join(&base, user_path).ok_or(SomeError::OutsideRoot)?;
fs::read(resolved)
```

Reference implementation: `rtb_assets::source::safe_join`. Copy the
helper into any crate that accepts user-configurable paths; do not
reinvent it ad-hoc.

### 1.2 Secret handling

Every crate that touches API tokens, passwords, or equivalent
secrets MUST:

1. Use `secrecy::SecretString` (re-exported from `rtb_credentials`)
   at every public API boundary. `&str` / `String` for a secret is a
   type error, not a style preference.
2. Never log secrets. If tracing instrumentation spans a block that
   carries a `SecretString`, instrument only on the surrounding
   non-secret context (service / account / duration / outcome).
3. Prefer `SecretString::clone()` over
   `SecretString::from(s.expose_secret().to_string())` — cloning
   keeps the value inside a zeroize-on-drop container the whole time.
4. `secrecy 0.10+` intentionally does not implement `Serialize` for
   `SecretString`. Do not try to route secrets through config
   serialisation; require a dedicated write path if they must
   persist.
5. Respect `CredentialError::LiteralRefusedInCi` — literal secrets
   in config are refused under `CI=true`. Tools wanting stricter
   enforcement set the var themselves before resolve.

### 1.3 Hook and panic-hook safety

Any `ReportHandler`, `panic_hook`, `tracing::Subscriber`, or
similar framework-global handler that calls into a user-supplied
closure MUST:

1. Wrap the closure call in `std::panic::catch_unwind(AssertUnwindSafe
   (...))` and suppress on panic.
2. Use a thread-local re-entry guard if the panic hook could recurse
   through the same handler (e.g. miette's panic hook renders through
   our `ReportHandler` which calls our footer closure).
3. Release any lock (`RwLock::read()`, `Mutex::lock()`) *before*
   writing to a formatter or I/O target. Locks released after a
   `writeln!` that itself panics would be poisoned.

Reference implementation: `rtb_error::hook::RtbReportHandler`.

### 1.4 Filesystem concurrency

`O_APPEND` on POSIX is atomic only up to `PIPE_BUF` (4 KiB on Linux).
Any sink that serialises structured records to disk (JSONL,
newline-delimited protobuf, etc.) MUST serialise concurrent writes
via an `Arc<tokio::sync::Mutex<()>>` shared across clones of the
same handle. Don't rely on O_APPEND atomicity alone.

Reference implementation: `rtb_telemetry::sink::FileSink`.

### 1.5 Environment variable mutation

`std::env::set_var` and `remove_var` are `unsafe` in Rust 2024
because they race with other threads reading env vars. The workspace
policy is:

- Production code: never mutate env vars. Read-only via
  `std::env::var` / `std::env::var_os` only.
- Test code: allowed via `#![allow(unsafe_code)]` at the test-file
  level, with a comment justifying why. Each test uses a **disjoint
  env-var name** (e.g. `RTBCFG_T6_PORT`, not `PORT`) and restores or
  removes its var at the end.
- `unsafe` MUST remain `#![forbid(unsafe_code)]` in every crate's
  `src/lib.rs`. The workspace-level lint is `deny` (not `forbid`) to
  let test files opt in with justification.

### 1.6 Regex from user input

Any regex compiled from a path that starts outside the binary
(config, flag, TUI field, HTTP payload) must be size- and
time-bounded. Rust's `regex` is already linear-time, but memory
bounds matter:

```rust
use regex::RegexBuilder;

let re = RegexBuilder::new(user_pattern)
    .size_limit(1 << 20)    // 1 MiB AST cap
    .dfa_size_limit(1 << 23) // 8 MiB DFA cap
    .build()?;
```

Also reject patterns > 1 KiB at the front door — they're almost
never legitimate. No time-limit is needed (Thompson NFA is
time-linear) — unlike Go's `regexp`.

### 1.7 TLS defaults

HTTP clients use `reqwest` with `default-features = false, features =
["rustls-tls", ...]`. `native-tls` is banned — it pulls OpenSSL as a
system dep and complicates cross-compile. HTTP servers use
`axum-server` with `rustls`.

### 1.8 Update verification

The `update` subsystem (pending v0.2) must verify both SHA-256 and
Ed25519 signatures of the release archive before calling
`self-replace`. SHA-only is **not** sufficient — a compromised
release host can rotate both the binary and its `.sha256` sidecar.

---

## 2. Rust idiomatic practice

### 2.1 Error handling

- `thiserror::Error + miette::Diagnostic` on every public enum.
- Every variant carries a `#[diagnostic(code(rtb::<crate>::<kind>))]`
  under the `rtb::` namespace.
- `#[non_exhaustive]` on every public error enum so variant additions
  are minor-version changes.
- Functions that take user-controlled input with non-trivial failure
  modes return `miette::Result<T>` or the crate's local `Result<T,
  LocalError>`.
- `anyhow` is not used in framework crates. Tests and examples may
  use it.
- Never `.unwrap()` or `.expect()` outside tests, examples, or
  documented "infallible in this context" situations.

### 2.2 Trait design

- Async traits use `#[async_trait]` until Rust's native async-fn-in-
  trait sprouts ergonomic `dyn` support. This is documented in the
  relevant module docs so contributors don't "modernise" into
  breakage.
- `#[non_exhaustive]` on every public enum. `#[must_use]` on every
  public fn that returns a non-`()` non-trivial type.
- Every trait that downstream crates will implement has an
  `#[async_trait]` annotation and the `Send + Sync + 'static` bound
  needed to cross `tokio::spawn`.

### 2.3 Typestate builders

For builders where certain fields are required before `.build()` is
callable, use hand-rolled typestate (phantom-typed `NoX` / `HasX`
markers). Reference: `rtb_cli::application::ApplicationBuilder`. Only
reach for `bon::Builder` when the build step is pure and needs no
custom validation.

### 2.4 Clone semantics

Structs threaded through command handlers (`App`,
`TelemetryContext`, `Assets`) MUST be cheap to `clone()` — every
field `Arc`-wrapped. Command handlers take context values by value;
clones are O(1).

### 2.5 `PhantomData` variance

For a generic `Marker<T>` that never holds a `T` directly, use
`PhantomData<fn() -> T>` — covariant in `T`, always `Send + Sync`
regardless of `T`'s auto-traits, no drop-check contamination. Annotate
inline if a reader might wonder why.

### 2.6 Plugin registration

Use `linkme::distributed_slice` for link-time plugin registration.
`linkme`'s attribute macro expands to `::linkme::…` paths, so every
consumer crate (not just the one defining the slice) needs `linkme`
as a **direct dependency**. Document this at the slice definition.

### 2.7 `#[doc(hidden)] pub` is not access control

Marking a `pub fn` as `#[doc(hidden)]` hides it from rustdoc but
does not restrict linkage. For "available to tests, hidden from
production code" use one of:

1. A separate `rtb-test-support` dev-dep crate that exposes the
   bypass constructor.
2. A sealed trait pattern — the bypass fn takes a value of a trait
   only implementable by `rtb-test-support`.

Prefer option 1 for ergonomics.

### 2.8 `Feature::all()`-style introspection

Returning a fixed-size array `[Self; N]` from an introspection
method on a `#[non_exhaustive]` enum creates a latent API break on
every new variant. Return `&'static [Self]` instead — length is a
value, not part of the type signature.

---

## 3. Concurrency

### 3.1 Cancellation

- `App::shutdown` is a `tokio_util::sync::CancellationToken`. Every
  long-running subsystem derives a child token via
  `shutdown.child_token()` and uses `tokio::select!` to race its
  work against `token.cancelled()`.
- No `Mutex::lock()` held across an `.await`. Use
  `tokio::sync::Mutex` when the critical section genuinely needs to
  span an await; otherwise release the guard before yielding.

### 3.2 Blocking calls

Platform APIs that are blocking (keyring on Windows/macOS, raw
filesystem on some paths, `std::fs::*`) run inside
`tokio::task::spawn_blocking`. Reference: `rtb_credentials::KeyringStore`.

### 3.3 Runtime choice

The workspace commits to `tokio`. `async-std` is deprecated; `smol`
is a fine runtime for library crates (smol-runnable code works under
tokio) but the framework itself assumes tokio primitives
(`CancellationToken`, `spawn_blocking`, `sync::watch`, `signal`).

---

## 4. Documentation

### 4.1 Intra-doc links

`just ci` runs `cargo doc` with `RUSTDOCFLAGS="-D warnings"`. Broken
intra-doc links fail the gate. When writing cross-crate references,
either use fully-qualified paths that resolve in scope, or wrap in
backticks so the ref is not treated as a link.

### 4.2 Per-crate rustdoc

Every public `struct`, `enum`, `fn`, `trait`, `type`, and `mod` has
a `///` or `//!` doc comment. Every public enum variant and struct
field has one. `missing_docs = "warn"` at workspace level; stub
crates opt out via a crate-level `#![allow(missing_docs)]` with a
TODO comment that names the owning spec.

### 4.3 Doc examples

Quick-start examples in a crate's `lib.rs` SHOULD be doctests (not
`ignore`) so rustc validates them. Where the example needs setup
that would bloat a doctest, the example MUST match `examples/minimal`
(or the relevant reference example) line-for-line so drift between
docs and reality is minimised.

### 4.4 Spec discipline

Every non-trivial change has a spec in
`docs/development/specs/YYYY-MM-DD-<feature>.md`. Spec lifecycle:

1. `DRAFT` — under review.
2. `IN PROGRESS` — implementation started.
3. `IMPLEMENTED` — feat commit landed; spec matches code.

**Before every `feat(...)` commit:** grep for spec status drift.
`rtb-credentials` was in the `IMPLEMENTED` feat commit but the
framework spec §16 still listed it under v0.2 — catch this at author
time, not review time.

Every spec has a §8 "Open questions" section. Every open question is
either resolved before the feat commit or explicitly deferred with a
version tag (e.g. `deferred to 0.2`).

### 4.5 Commit discipline per feat

Every `feat(<crate>): implement v0.1` commit body mentions at least
one of:
- `CHANGELOG.md` entry in `[Unreleased]`.
- Framework spec annotation change.
- `docs/concepts/*` update.

This keeps docs on the same cadence as code.

### 4.6 Safe attribute set for telemetry events

The `rtb_telemetry::Event::attrs` map is shipped verbatim to the
configured sink. Tool authors MUST NOT pass:

- Raw command-line arguments.
- File paths under the user's home directory.
- Error messages sourced from user input.
- Secrets (always — they shouldn't be on `Event` at all; `attrs` is
  not a secret-carrying field).
- Unstripped user-supplied strings.

Safe attrs: command name, enumerated outcome, duration bucket,
framework version. Planned `rtb-redact` helper in v0.2.

---

## 5. Testing

### 5.1 Test triad

Every `rtb-*` crate ships:

1. **Unit tests** under `tests/unit.rs` — one T# per acceptance
   criterion.
2. **BDD scenarios** via `cucumber-rs` under `tests/features/*.feature`
   + `tests/steps/`. See `docs/development/bdd-pattern.md`.
3. **Trybuild fixtures** for `#[non_exhaustive]` cross-crate matches
   and typestate-builder enforcement.

Acceptance criteria are numbered (T1…, S1…) and quoted verbatim in
the crate's v0.1 spec §4.

### 5.2 Coverage gate

Workspace gate is ≥70% line coverage via `cargo llvm-cov`. Lift as
maturity grows; don't lower.

### 5.3 Panic and error tests

When fixing a bug, add a test that would have caught it (T# mapped
to the spec section or an `Unreleased` changelog entry). The test
title encodes intent (`t14_directory_source_rejects_parent_traversal`
not `t14_traversal`).

### 5.4 Env-var-mutating tests

See § 1.5 above. Additional rules:

- Use a prefix unique to the test (e.g. `RTBCRED_T8_`), never a
  plain name (e.g. `PORT`).
- Restore the variable's prior value — capture with
  `std::env::var(...).ok()` before mutating.
- Document at the top of the test file that parallel test execution
  is NOT supported by these tests.

### 5.5 Insta snapshots

Prefer `insta::assert_json_snapshot!` for JSON-shaped assertions
(schema stability) over hand-written `assert_eq!` on serde values.
Review with `cargo insta review`.

---

## 6. For agents working on RTB

### 6.1 Standing prompt additions

When spawning a new agent for RTB work, include a block like:

> Before writing code, read:
> 1. `CLAUDE.md` (repo root) — workflow, commit conventions, anti-patterns.
> 2. `docs/development/engineering-standards.md` — standing security,
>    correctness, and documentation rules. Rules in §1 (Security)
>    are non-negotiable; rules in §4 (Documentation) are checked by
>    `just ci` and must not regress.
> 3. The relevant crate's `docs/development/specs/<crate>-v0.1.md`
>    if implementing against an existing spec.

### 6.2 Pre-commit checklist

Before any `feat(...)` commit the author (agent or human) checks:

- `just ci` passes locally.
- Every public item added has a `///` doc comment.
- `CHANGELOG.md` `[Unreleased]` has at least one bullet for the
  change.
- The owning spec's status is `IMPLEMENTED` (flip from `IN PROGRESS`).
- The framework spec's `§16` roadmap reflects what shipped.
- No `.unwrap()` / `.expect()` in production code paths.
- No `unsafe` added outside test files.
- Intra-doc links resolve (covered by `just ci` via `just docs`).

### 6.3 Anti-patterns to avoid (from review)

These were real mistakes caught in the v0.1 review; don't repeat:

- `PathBuf::join(user_supplied)` without traversal check.
- User-supplied closure invoked without `catch_unwind`.
- `FileSink::emit` per-event open/close without a concurrency gate.
- `[Self; N]` return on a `#[non_exhaustive]` enum.
- `SecretString::from(s.expose_secret().to_string())` instead of
  `s.clone()`.
- `#[doc(hidden)] pub fn` as "access control".
- `print!(err)` followed by a neutral error mapping (produces double
  output).
- `BUILTIN_COMMANDS` populated without dedup (clap rejects duplicate
  subcommand names at parse time).

### 6.4 Scope guardrails

If a "small fix" touches more than three files across more than one
crate, it's not small. Either split the commit or escalate to a
proper spec.

New features always have a spec under
`docs/development/specs/YYYY-MM-DD-*.md`. "Just adding a method" is
not an exception — the method's contract belongs in the spec.

---

## 7. When these standards change

This document is authoritative; changes happen via normal PR review.
When a standard changes, update:

1. This document.
2. The `CHANGELOG.md` `[Unreleased]` section under **Documentation**.
3. `CLAUDE.md`'s "Anti-patterns" table if the change adds or removes
   an anti-pattern.
4. The `~/.claude/projects/.../memory/` notes so agents pick up the
   new rule across sessions.

Last sweep: 2026-04-23, v0.1 critical-review remediation.
