---
title: rtb-redact v0.1
status: DRAFT
date: 2026-04-23
authors: [Matt Cockayne]
crate: rtb-redact
supersedes: null
---

# `rtb-redact` v0.1 — Free-form secret redaction helper

**Status:** DRAFT — awaiting review.
**Target crate:** `rtb-redact` (new).
**Parent contract:**
[`docs/development/engineering-standards.md` § 1.4 Credential redaction](../engineering-standards.md),
referenced by
[§12 Observability](rust-tool-base.md#12-observability) of the framework spec.
**Consumers (v0.2):** `rtb-telemetry` (auto-applied to `args` / `err_msg`
fields); `rtb-cli` HTTP middleware (uses `SENSITIVE_HEADERS` constant);
any downstream tool code that writes user-supplied strings to `tracing`
spans or external observability surfaces.

---

## 1. Motivation

`rtb-telemetry` v0.1 explicitly shipped with redaction responsibility on
the caller (see the Event doc comment at
`crates/rtb-telemetry/src/event.rs:24`). That was a deliberate v0.1 punt
flagged in both the framework spec and the v0.1 secondary review; the
plan was always to land a shared helper before `rtb-telemetry`'s
redaction could become automatic.

`rtb-redact` is that helper. It sanitises free-form strings just before
they cross a boundary the caller does not control — a log line going to
Datadog, a telemetry event going to OTLP, an HTTP header going to a
third-party API. Conservative by default: false positives (redacting
things that weren't actually secrets) are preferred over a leak. The
helper does **not** try to parse structured payloads; tool authors who
need structured-field redaction use `#[serde(skip)]` + `SecretString` and
`#[tracing::instrument(skip = ...)]`.

Not replacing: `secrecy::SecretString`. `rtb-redact` is for strings whose
contents we don't know in advance (URLs with embedded tokens, error
messages that stringify arbitrary config, etc.). `SecretString` remains
the right answer for strings we *do* know are secrets at the type level.

## 2. Public API

### 2.1 Crate root

```rust
//! Free-form secret redaction for log lines, telemetry events, and
//! diagnostic surfaces.

pub fn string(input: &str) -> String;

pub fn string_into(input: &str, out: &mut String);

/// Exact case-insensitive match against this set means the header
/// value must be redacted at DEBUG/TRACE log levels. `phf::Set` keeps
/// lookup O(1) as the list grows — and it is expected to grow, so the
/// data-structure choice is future-proofed now rather than later.
pub static SENSITIVE_HEADERS: phf::Set<&'static str> = phf::phf_set! {
    "authorization",
    "proxy-authorization",
    "cookie",
    "set-cookie",
    "x-api-key",
    "x-auth-token",
    "x-amz-security-token",
    "x-goog-api-key",
    "x-anthropic-api-key",
    "x-openai-api-key",
    // Add more as new providers land. phf's perfect-hash table is
    // rebuilt at compile time; there is no runtime cost to growing
    // this list.
};

pub fn is_sensitive_header(name: &str) -> bool;

pub fn redact_header_value(value: &str) -> String;
```

### 2.2 Redaction rules

`string()` runs the following passes in order, each replacing matches
with the literal string `"[redacted]"` unless noted:

1. **URL userinfo.** Any substring matching
   `(https?|[a-z]+)://[^:\s]+:[^@\s]+@` has the userinfo stripped to
   `<scheme>://[redacted]@`.
2. **Authorization-header-style values.** `Authorization: <anything>`,
   `Bearer <token>`, `Basic <token>`, `Token <token>` — the credential
   portion becomes `[redacted]`.
3. **Query-parameter keys that are well-known secrets.** The parameter
   *value* is replaced, preserving the key for debuggability:
   `api_key`, `apikey`, `access_token`, `refresh_token`, `token`,
   `password`, `passwd`, `secret`, `signature`, `sig`, `auth`,
   `x-api-key`. Case-insensitive.
4. **Well-known credential prefixes.** Any whitespace- or boundary-
   delimited token matching one of the prefixes below and at least 20
   characters long is redacted in full (prefix-shaped tokens shorter
   than 20 chars are left alone — they're usually not secrets):
   - `sk-...` (OpenAI)
   - `sk-ant-...` (Anthropic)
   - `ghp_...`, `gho_...`, `ghs_...`, `ghu_...` (GitHub)
   - `glpat-...` (GitLab PAT)
   - `AIza...` (Google API keys)
   - `AKIA...`, `ASIA...` (AWS access-key IDs)
   - `xoxb-...`, `xoxp-...`, `xoxa-...` (Slack)
   - `SG.` + 66+ alphanumerics (SendGrid)
5. **Long opaque tokens.** Any whitespace-delimited run of 40 or more
   base64 / hex characters becomes `[redacted]`. This catches
   provider-opaque tokens without named prefixes.
6. **JWT-shaped tokens.** `eyJ` + base64 segments separated by `.`
   totaling ≥100 characters. Redacted fully.
7. **Private-key PEM blocks.** Any substring between
   `-----BEGIN ... PRIVATE KEY-----` and `-----END ... PRIVATE KEY-----`
   inclusive is replaced with
   `-----BEGIN PRIVATE KEY-----\n[redacted]\n-----END PRIVATE KEY-----`.

Rules are applied left-to-right against a single pass over the input.
Order matters only for overlapping matches — URL userinfo is processed
first so that e.g. `https://user:sk-abc...@host` doesn't get caught by
the naked-prefix rule inside the userinfo capture.

### 2.3 `redact_header_value`

```rust
pub fn redact_header_value(value: &str) -> String {
    if value.is_empty() { return String::new(); }
    // Always redact for sensitive header names; for non-sensitive names
    // the caller opts in by passing the value through explicitly.
    "[redacted]".to_string()
}
```

Caller flow in `rtb-cli`'s HTTP middleware at DEBUG:
`if is_sensitive_header(name) { log.field(name, redact_header_value(value)); }`.

### 2.4 Feature flags + dependencies

No Cargo features. The crate is small and always-on. Deps are
deliberately minimal:

- `regex` — patterns are literal, compiled once via
  `once_cell::sync::Lazy<Regex>`. No user-supplied patterns; the
  `rtb_app::regex_util::compile_bounded` helper (itself a v0.2
  deliverable — see O2) is not needed here.
- `phf` — compile-time perfect-hash set for `SENSITIVE_HEADERS`.
- `once_cell` — lazy regex compilation.

No `serde`, no `tracing`, no async runtime. The crate is a pure
string-to-string function.

## 3. Acceptance criteria

### 3.1 Unit-test acceptance (T#)

- **T1 — Empty string round-trips.** `string("") == ""`.
- **T2 — String with no secret content round-trips verbatim.**
  `string("hello world") == "hello world"`.
- **T3 — URL userinfo redacted.** Given `https://alice:hunter2@host/path`,
  output is `https://[redacted]@host/path`.
- **T4 — `Bearer` token redacted.** Given `Authorization: Bearer ghp_abc…`,
  output contains `Bearer [redacted]`.
- **T5 — `Basic` token redacted.** As T4 for `Basic ZGF2ZTpodW50ZXIy`.
- **T6 — Sensitive query param redacted, key preserved.** Given
  `GET /foo?api_key=sk-abc…&tag=prod`, output has
  `?api_key=[redacted]&tag=prod`.
- **T7 — Case-insensitive query-key match.** `?API_KEY=…` and `?apikey=…`
  and `?X-API-Key=…` all redact.
- **T8 — Provider prefix redacted when length ≥ 20.** `sk-`, `sk-ant-`,
  `ghp_`, `glpat-`, `AIza`, `AKIA`, `xoxb-` — one each.
- **T9 — Provider prefix preserved when length < 20.** `sk-abc` passes
  through unchanged.
- **T10 — Long opaque token redacted.** 40+ chars of `[A-Za-z0-9+/=_-]`
  between whitespace boundaries becomes `[redacted]`.
- **T11 — JWT redacted.** `eyJhbGci...very.long.jwt` redacted fully.
- **T12 — PEM private key block redacted.** Multi-line PEM input yields a
  header + `[redacted]` + footer; the key material is gone.
- **T13 — `SENSITIVE_HEADERS` list is comprehensive for known providers.**
  Asserts inclusion of `authorization`, `x-api-key`, `cookie`,
  `x-anthropic-api-key`, `x-openai-api-key`, `x-goog-api-key`,
  `x-amz-security-token` at minimum.
- **T14 — `is_sensitive_header` is case-insensitive.**
  `is_sensitive_header("AUTHORIZATION") == true`.
- **T15 — `redact_header_value` returns `"[redacted]"` for any non-empty
  input.** Empty stays empty.
- **T16 — `string_into` reuses the caller's buffer.**
  A call with a pre-allocated 1 KiB `String` produces no additional
  allocation for the happy-path (no redactions) — verified by an
  `jemalloc_ctl`-gated perf test or a manual capacity check.
- **T17 — No `unsafe_code` in the crate.** Verified by workspace-level
  lint, not a runtime test.

### 3.2 Gherkin acceptance (S#)

All scenarios live in `crates/rtb-redact/tests/features/redact.feature`.

- **S1 — A connection-string URL with embedded password redacts only the
  userinfo.** Feature:
  `Given the input is "postgres://app:hunter2@db.internal/mydb"`,
  `When I redact the string`,
  `Then the output is "postgres://[redacted]@db.internal/mydb"`.
- **S2 — A log line mixing a GitHub token and a JWT redacts both.**
- **S3 — A free-form error message carrying a connection URL and an
  `Authorization` header redacts both without corrupting the rest of the
  message.**
- **S4 — A PEM block embedded in a multi-line log redacts only the key
  material.**
- **S5 — Known false-positive: a Google Maps embed URL containing
  `key=AIza…` redacts the key.** (Not strictly a false positive — Maps
  API keys are mild secrets — but documents the behaviour.)
- **S6 — Known limitation: a custom token prefix not on the allowlist is
  **not** redacted unless it also trips the 40+ opaque-char rule.**
  Scenario asserts the behaviour so users know to escalate via the
  opaque-char threshold.

### 3.3 Integration with `rtb-telemetry`

`rtb-telemetry`'s `Event::args` and `Event::err_msg` fields gain an
automatic `redact::string` pass before serialisation. Implemented as a
single-line call in `FileSink::write_event` and any future `HttpSink` /
`OtlpSink`. Unit-test acceptance T10 on `rtb-telemetry` covers that
integration; the redactor itself stays rtb-telemetry-agnostic.

## 4. Security & operational requirements

- `#![forbid(unsafe_code)]` at the crate root.
- All regex patterns are literal, bounded, and compiled once via
  `once_cell::sync::Lazy<Regex>`. No user-supplied patterns. No ReDoS
  vectors; Rust's `regex` is Thompson-NFA / linear-time.
- No network, no filesystem, no environment reads. The crate is a pure
  function of its input.
- No `panic!` or `unwrap()` in public functions. Regex compilation
  failures surface at crate-load (panic in the `Lazy` initialiser is
  acceptable because it indicates a source-code bug, not a user-data
  problem — tested in T17-equivalent).
- Thread-safe: every cached regex is `&'static` via `Lazy`.
- Deterministic: same input always produces same output.

## 5. Non-goals (explicit)

- **Structured-data redaction.** No JSON-path / YAML-tree walker; callers
  redact field-by-field at the type layer with `SecretString`.
- **Customisable replacement string.** `[redacted]` is hard-coded; tools
  that need a different literal wrap `rtb_redact::string` themselves.
- **Rule allowlist / denylist configuration.** No runtime toggles; the
  rule set is a single versioned shape. v0.2.x can add opt-in via a
  `RedactOptions` struct without breaking v0.2 callers.
- **Performance guarantees below "fast enough for log lines".** The
  design goal is <10 µs for a typical 200-char log line on commodity
  hardware. Not a hot-path helper; callers who need zero-cost paths gate
  by log level first.
- **i18n.** The replacement literal is ASCII `[redacted]`.

## 6. Rollout plan

1. Land this spec + Gherkin + failing unit tests together.
2. Implement the regexes + API to green.
3. Integrate into `rtb-telemetry` in a follow-up PR; update
   `rtb-telemetry`'s event doc comment to remove the "redaction
   responsibility on the caller" language.
4. Add `rtb-redact` to `rtb-cli`'s HTTP middleware (debug-log redactor).
5. Document in `docs/components/rtb-redact.md` + add to components
   `index.md`.

## 7. Open questions

- **O1 — Should the allowlist of provider prefixes be extensible at
  compile time via Cargo features?** e.g. `features = ["aws-sso"]` adds
  `AQoDYXdz...` prefixes. **Resolved: no.** The security story is
  clearer when every supported prefix is always compiled in — a user
  cannot accidentally opt out of a prefix by omitting a feature flag,
  and a tool author cannot narrow the allowlist in the name of binary
  size at the cost of a future leak. If a prefix is well-enough-known
  for RTB to ship a rule for, it ships unconditionally. Binary-size
  cost is negligible (each rule is a small compiled regex).
- **O2 — `rtb_app::regex_util::compile_bounded`.** The framework spec
  and `CLAUDE.md` § Regex Compilation describe a helper that applies
  `RegexBuilder::size_limit(1 MiB)` + `dfa_size_limit(8 MiB)` + 1 KiB
  pattern bound for user-supplied patterns. `rtb-redact` has no
  user-supplied patterns, so it doesn't need the helper. But the helper
  itself hasn't shipped — is it v0.2 or v0.3? If v0.2, where: in
  `rtb-app`, or its own tiny crate? Proposed resolution: land in
  `rtb-app::regex_util` as part of the v0.2 cycle, as a small
  independent PR after `rtb-redact`.
- **O3 — Should `string()` return `Cow<str>` to skip allocation when no
  redactions apply?** Would save a `String::clone` on the fast path.
  Proposed resolution: yes, `Cow<'_, str>` return. Revisit if it causes
  call-site friction.
- **O4 — `redact::SENSITIVE_HEADERS` data structure.** **Resolved:
  `phf::Set`** — future-proofed now. New provider integrations (Azure,
  Cloudflare, Vercel, etc.) will add headers quickly; starting with a
  slice would mean a future breaking API swap. `phf` is a tiny
  compile-time dep with no runtime cost.
- **O5 — Behaviour on invalid UTF-8 input.** `&str` is already UTF-8; a
  caller with a `Vec<u8>` that might not be has to convert first. Should
  we offer a `bytes(input: &[u8]) -> Vec<u8>` variant? Proposed
  resolution: no — forces the caller to think about the encoding.
