//! Free-form secret redaction for log lines, telemetry events, and
//! diagnostic surfaces.
//!
//! See `docs/development/specs/2026-04-23-rtb-redact-v0.1.md` for the
//! full design and the seven-pass rule set.
//!
//! ```
//! use rtb_redact::string;
//!
//! let scrubbed = string("connect to postgres://app:hunter2@db/mydb");
//! assert!(scrubbed.contains("[redacted]"));
//! assert!(!scrubbed.contains("hunter2"));
//! ```

#![forbid(unsafe_code)]

use std::borrow::Cow;
use std::sync::LazyLock;

use regex::Regex;

/// Header names whose values must be redacted at DEBUG / TRACE log
/// levels. Case-insensitive match via
/// [`is_sensitive_header`]. `phf::Set` keeps lookup O(1) as the list
/// grows.
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
};

const REDACTED: &str = "[redacted]";

/// Redact secrets in `input`, returning a borrowed [`Cow`] when no
/// redactions apply and an owned `String` otherwise.
///
/// The rule set is applied in the documented order (see the spec).
/// `input` is expected to be UTF-8 (it is already typed as `&str`);
/// callers holding `&[u8]` must convert themselves.
#[must_use]
pub fn string(input: &str) -> Cow<'_, str> {
    if input.is_empty() {
        return Cow::Borrowed(input);
    }

    // Fast path: if none of the anchor characters or keywords appear
    // in the string, no rule can match. Avoids allocating.
    if !fast_has_sensitive_anchor(input) {
        return Cow::Borrowed(input);
    }

    let mut out = input.to_string();
    apply_rules(&mut out);

    // If nothing actually changed, return Borrowed so callers don't
    // pay for the clone on false positives of the fast-path check.
    if out == input {
        Cow::Borrowed(input)
    } else {
        // SAFETY of correctness: `out` was derived from the input via
        // regex replacements; there's no way to get back to &input
        // from here without the Owned wrapper.
        Cow::Owned(out)
    }
}

/// Same as [`string`], but writes into a caller-supplied `String`.
/// Useful for hot loops that want to reuse a buffer.
pub fn string_into(input: &str, out: &mut String) {
    out.clear();
    if input.is_empty() {
        return;
    }
    if !fast_has_sensitive_anchor(input) {
        out.push_str(input);
        return;
    }
    out.push_str(input);
    apply_rules(out);
}

/// Case-insensitive membership check against [`SENSITIVE_HEADERS`].
#[must_use]
pub fn is_sensitive_header(name: &str) -> bool {
    // `phf::Set` is case-sensitive. We lowercase the name into a small
    // stack-friendly buffer; realistic header names are < 64 chars.
    let mut buf = [0u8; 128];
    let bytes = name.as_bytes();
    if bytes.len() > buf.len() {
        // Oversized header names can't be in the known-sensitive list.
        return false;
    }
    for (i, &b) in bytes.iter().enumerate() {
        buf[i] = b.to_ascii_lowercase();
    }
    // Safe: ASCII-lowercased ASCII stays ASCII stays UTF-8.
    let lower = std::str::from_utf8(&buf[..bytes.len()]).unwrap_or("");
    SENSITIVE_HEADERS.contains(lower)
}

/// Unconditionally redact a header value. Callers invoke this for
/// any header name matching [`is_sensitive_header`].
#[must_use]
pub fn redact_header_value(value: &str) -> String {
    if value.is_empty() {
        String::new()
    } else {
        REDACTED.to_string()
    }
}

// ---------------------------------------------------------------------
// Internal: fast-path pre-check.
// ---------------------------------------------------------------------

/// Returns `true` if `input` contains any character or substring that
/// could plausibly trigger a redaction rule. This avoids allocating
/// and running seven regexes over clean strings.
fn fast_has_sensitive_anchor(input: &str) -> bool {
    // Any of these anchors could indicate a match. The check is
    // intentionally loose — false positives here cost one allocation
    // and seven regex runs on a small string.
    input.contains('@')        // URL userinfo
        || input.contains('=') // query params
        || input.contains('?')
        || input.contains('-') // token prefixes use hyphens heavily
        || input.contains('_')
        || input.contains('.') // JWT dots, AWS prefixes
        || input.contains("-----BEGIN ")
        || has_auth_scheme(input)
        || has_long_run(input)
}

/// Crude check for "contains a whitespace-delimited run of 40+ chars
/// that could be a token."
fn has_long_run(input: &str) -> bool {
    let mut run = 0usize;
    for b in input.bytes() {
        if b.is_ascii_alphanumeric()
            || b == b'+'
            || b == b'/'
            || b == b'='
            || b == b'_'
            || b == b'-'
        {
            run += 1;
            if run >= 40 {
                return true;
            }
        } else {
            run = 0;
        }
    }
    false
}

fn has_auth_scheme(input: &str) -> bool {
    // Case-insensitive lookahead for the three tokens we act on.
    // Simple byte scan is enough; we're avoiding a regex here.
    let lower_bytes = input.as_bytes();
    for window in lower_bytes.windows(7) {
        let w = window;
        if eq_ignore_ascii_case(w, b"bearer ")
            || eq_ignore_ascii_case(&w[..6], b"basic ")
            || eq_ignore_ascii_case(&w[..6], b"token ")
        {
            return true;
        }
    }
    false
}

fn eq_ignore_ascii_case(a: &[u8], b: &[u8]) -> bool {
    if a.len() < b.len() {
        return false;
    }
    a[..b.len()].iter().zip(b.iter()).all(|(x, y)| x.eq_ignore_ascii_case(y))
}

// ---------------------------------------------------------------------
// Internal: rule application.
// ---------------------------------------------------------------------

fn apply_rules(out: &mut String) {
    // 1. URL userinfo
    replace_all(out, &RE_URL_USERINFO, |caps| format!("{}://{REDACTED}@", &caps[1]));
    // 2. Authorization-header-style values.
    replace_all(out, &RE_AUTH_SCHEME, |caps| format!("{} {REDACTED}", &caps[1]));
    // 3. Query-parameter sensitive keys.
    replace_all(out, &RE_QUERY_SENSITIVE, |caps| format!("{}={REDACTED}", &caps[1]));
    // 7. PEM private key blocks — run before token rules so the
    //    key body (which can otherwise match the long-run rule) is
    //    collapsed into a single REDACTED marker.
    replace_all(out, &RE_PEM_BLOCK, |_caps| {
        "-----BEGIN PRIVATE KEY-----\n[redacted]\n-----END PRIVATE KEY-----".to_string()
    });
    // 4. Well-known credential prefixes.
    replace_all(out, &RE_NAMED_PREFIX, |caps| {
        let matched = &caps[0];
        if matched.len() >= 20 {
            REDACTED.to_string()
        } else {
            matched.to_string()
        }
    });
    // 6. JWT-shaped tokens. (Run before the generic long-run rule so
    //    partial overlaps don't produce a half-masked JWT.) The spec
    //    requires total length >= 100 chars — short "eyJ.x.y.z"
    //    strings pass through unchanged.
    replace_all(out, &RE_JWT, |caps| {
        let matched = &caps[0];
        if matched.len() >= 100 {
            REDACTED.to_string()
        } else {
            matched.to_string()
        }
    });
    // 5. Long opaque tokens. The captured boundary chars (leading /
    //    trailing whitespace, or start / end of input) are re-emitted
    //    verbatim so the word spacing around the redaction is preserved.
    replace_all(out, &RE_LONG_OPAQUE, |caps| format!("{}{REDACTED}{}", &caps[1], &caps[3]));
}

fn replace_all<F>(buf: &mut String, re: &Regex, mut f: F)
where
    F: FnMut(&regex::Captures<'_>) -> String,
{
    // Build a new string if any match exists; otherwise leave `buf`
    // untouched. We don't use `Regex::replace_all` with a closure
    // directly because it takes a Replacer by value and we want to
    // keep the API on `&mut String` rather than allocating a Cow.
    if !re.is_match(buf) {
        return;
    }
    let mut out = String::with_capacity(buf.len());
    let mut last = 0;
    for caps in re.captures_iter(buf) {
        let whole = caps.get(0).expect("captures always include group 0");
        out.push_str(&buf[last..whole.start()]);
        out.push_str(&f(&caps));
        last = whole.end();
    }
    out.push_str(&buf[last..]);
    *buf = out;
}

// ---------------------------------------------------------------------
// Internal: compiled patterns. All literal; compiled once.
// ---------------------------------------------------------------------

static RE_URL_USERINFO: LazyLock<Regex> = LazyLock::new(|| {
    // Matches scheme://user:pass@ (userinfo). Captures the scheme
    // so we can re-emit it.
    Regex::new(r"([a-zA-Z][a-zA-Z0-9+.-]*)://[^:\s/?#]+:[^@\s]+@").expect("valid regex")
});

static RE_AUTH_SCHEME: LazyLock<Regex> = LazyLock::new(|| {
    // Bearer / Basic / Token <credential>. The credential is whatever
    // follows the scheme until whitespace or end.
    Regex::new(r"(?i)\b(Bearer|Basic|Token)\s+[A-Za-z0-9_\-.+/=]+").expect("valid regex")
});

static RE_QUERY_SENSITIVE: LazyLock<Regex> = LazyLock::new(|| {
    // Match `<sensitive-key>=<value>` in query strings. Value runs up
    // to `&`, whitespace, or end. Case-insensitive key match.
    Regex::new(
        r"(?i)([?&]?(?:api[_-]?key|access[_-]?token|refresh[_-]?token|token|password|passwd|secret|signature|sig|auth|x[_-]?api[_-]?key))=[^&\s#]+"
    )
    .expect("valid regex")
});

static RE_NAMED_PREFIX: LazyLock<Regex> = LazyLock::new(|| {
    // Well-known provider tokens. Matches the prefix + trailing
    // allowed characters. The closure in `apply_rules` checks for
    // total length >= 20 before redacting.
    Regex::new(
        r"(?x)
        \b(
            sk-ant-[A-Za-z0-9_\-]+
          | sk-[A-Za-z0-9_\-]+
          | (?:ghp|gho|ghs|ghu)_[A-Za-z0-9]+
          | glpat-[A-Za-z0-9_\-]+
          | AIza[A-Za-z0-9_\-]+
          | (?:AKIA|ASIA)[A-Z0-9]+
          | xox[baprs]-[A-Za-z0-9\-]+
          | SG\.[A-Za-z0-9_\-]{22,}\.[A-Za-z0-9_\-]{43,}
        )
        ",
    )
    .expect("valid regex")
});

static RE_JWT: LazyLock<Regex> = LazyLock::new(|| {
    // eyJ... . ... . ... totalling >= 100 chars.
    Regex::new(r"eyJ[A-Za-z0-9_\-]+\.[A-Za-z0-9_\-]+\.[A-Za-z0-9_\-]+").expect("valid regex")
});

static RE_LONG_OPAQUE: LazyLock<Regex> = LazyLock::new(|| {
    // Whitespace-bounded run of 40+ base64/hex-ish chars. The boundary
    // chars are explicit capture groups so the replacement closure can
    // preserve the surrounding whitespace.
    Regex::new(r"(^|\s)([A-Za-z0-9+/=_\-]{40,})(\s|$)").expect("valid regex")
});

static RE_PEM_BLOCK: LazyLock<Regex> = LazyLock::new(|| {
    // DOTALL via (?s) so `.` crosses newlines.
    Regex::new(r"(?s)-----BEGIN [A-Z ]*PRIVATE KEY-----.*?-----END [A-Z ]*PRIVATE KEY-----")
        .expect("valid regex")
});
