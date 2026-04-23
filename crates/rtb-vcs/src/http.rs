//! Shared HTTP plumbing for REST-API-backed backends (github, gitlab,
//! gitea, codeberg).
//!
//! Every backend needs the same machinery: a reqwest client with
//! `https_only` enforcement (bypassable for tests), streaming asset
//! downloads, a uniform 401 / 404 / 429 / rate-limited → `ProviderError`
//! mapping, and percent-encoding for path segments. Pulling it here
//! keeps each backend module focused on endpoint shapes.
//!
//! # Lint exception
//!
//! This module allows `unsafe_code` because `reqwest`'s byte-stream
//! integration via `tokio_util::io::StreamReader` routes through
//! futures-util combinators that can pick up `link_section` under
//! certain dependency versions. No hand-rolled `unsafe` blocks exist
//! here.

#![allow(unsafe_code)]

use std::fmt::Write as _;
use std::time::Duration;

use tokio::io::AsyncRead;
use tokio_util::io::StreamReader;

use crate::release::ProviderError;

// ---------------------------------------------------------------------
// Client construction
// ---------------------------------------------------------------------

/// Build a reqwest client with the conventions every rest-backed
/// backend inherits: HTTPS enforcement on by default (bypassable for
/// tests via `allow_insecure`), a stable user-agent, optional
/// per-request timeout.
pub fn build_client(
    timeout_seconds: u64,
    allow_insecure: bool,
) -> Result<reqwest::Client, ProviderError> {
    let mut builder = reqwest::Client::builder()
        .https_only(!allow_insecure)
        .user_agent(concat!("rtb-vcs/", env!("CARGO_PKG_VERSION")));
    if timeout_seconds > 0 {
        builder = builder.timeout(Duration::from_secs(timeout_seconds));
    }
    builder.build().map_err(|e| ProviderError::InvalidConfig(format!("reqwest build failed: {e}")))
}

/// Return the correct URL scheme for the client.
#[must_use]
pub const fn scheme_for(allow_insecure: bool) -> &'static str {
    if allow_insecure {
        "http"
    } else {
        "https"
    }
}

// ---------------------------------------------------------------------
// Status / header mapping
// ---------------------------------------------------------------------

/// Translate a non-2xx HTTP response into the right `ProviderError`.
/// Populates `retry_after` from `Retry-After` or `X-RateLimit-Reset`
/// when rate-limited. `extra_rate_limit_signal` is a hook for backend-
/// specific rate-limit detection (e.g. GitHub's `X-RateLimit-Remaining: 0`
/// on 403).
pub fn map_status_to_error(
    resp: &reqwest::Response,
    host: &str,
    extra_rate_limit_signal: bool,
) -> Result<(), ProviderError> {
    let status = resp.status();
    if status.is_success() {
        return Ok(());
    }
    let headers = resp.headers();
    let is_rate_limit = extra_rate_limit_signal || status == reqwest::StatusCode::TOO_MANY_REQUESTS;

    if is_rate_limit {
        return Err(ProviderError::RateLimited {
            host: host.to_string(),
            retry_after: retry_after_from_headers(headers),
        });
    }
    match status {
        reqwest::StatusCode::UNAUTHORIZED => {
            Err(ProviderError::Unauthorized { host: host.to_string() })
        }
        reqwest::StatusCode::NOT_FOUND => Err(ProviderError::NotFound {
            what: format!("{} {}", status.as_u16(), status.canonical_reason().unwrap_or("")),
        }),
        _ => Err(ProviderError::Transport(format!("unexpected status {status} from {host}"))),
    }
}

/// Parse the GitHub-style rate-limit headers. Returns the first
/// non-zero `Retry-After` value in seconds, or falls back to computing
/// the delay from `X-RateLimit-Reset` epoch seconds.
pub fn retry_after_from_headers(headers: &reqwest::header::HeaderMap) -> Option<Duration> {
    if let Some(s) = header_str(headers, "retry-after") {
        if let Ok(secs) = s.parse::<u64>() {
            return Some(Duration::from_secs(secs));
        }
    }
    if let Some(reset) = header_str(headers, "x-ratelimit-reset") {
        if let Ok(epoch) = reset.parse::<i64>() {
            let now = time::OffsetDateTime::now_utc().unix_timestamp();
            if epoch > now {
                let secs = u64::try_from(epoch - now).unwrap_or(0);
                return Some(Duration::from_secs(secs));
            }
        }
    }
    None
}

/// Return a stringified header value, if present.
pub fn header_str<'a>(headers: &'a reqwest::header::HeaderMap, name: &str) -> Option<&'a str> {
    headers.get(name).and_then(|v| v.to_str().ok())
}

// ---------------------------------------------------------------------
// Response body
// ---------------------------------------------------------------------

/// Parse the response body as JSON into the requested type, mapping
/// failures to `ProviderError::MalformedResponse`.
pub async fn parse_json<T: serde::de::DeserializeOwned>(
    resp: reqwest::Response,
) -> Result<T, ProviderError> {
    resp.json::<T>().await.map_err(|e| ProviderError::MalformedResponse(e.to_string()))
}

/// Stream a response body as an `AsyncRead`. Returns the content-length
/// the server reported (or `0` if absent).
pub fn stream_body(resp: reqwest::Response) -> (Box<dyn AsyncRead + Send + Unpin>, u64) {
    let content_length = resp.content_length().unwrap_or(0);
    let stream = MapErrIo { inner: resp.bytes_stream() };
    let reader = StreamReader::new(stream);
    (Box::new(reader), content_length)
}

/// Adapter turning `reqwest::Error` into `io::Error` for byte streams.
/// `StreamReader` wants `io::Result` items; reqwest's native stream
/// yields `reqwest::Result`.
struct MapErrIo<S> {
    inner: S,
}

impl<S> futures_util::Stream for MapErrIo<S>
where
    S: futures_util::Stream<Item = reqwest::Result<bytes::Bytes>> + Send + Unpin,
{
    type Item = std::io::Result<bytes::Bytes>;
    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        use futures_util::StreamExt as _;
        match self.inner.poll_next_unpin(cx) {
            std::task::Poll::Ready(Some(Ok(bytes))) => std::task::Poll::Ready(Some(Ok(bytes))),
            std::task::Poll::Ready(Some(Err(e))) => {
                std::task::Poll::Ready(Some(Err(std::io::Error::other(e))))
            }
            std::task::Poll::Ready(None) => std::task::Poll::Ready(None),
            std::task::Poll::Pending => std::task::Poll::Pending,
        }
    }
}

// ---------------------------------------------------------------------
// Percent-encoding for path segments
// ---------------------------------------------------------------------

/// Percent-encode a tag or path segment. Minimal — reserved chars from
/// RFC 3986 that commonly appear in tags (`/`, `+`, `#`) get encoded;
/// unreserved chars pass through.
pub fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => {
                let _ = write!(out, "%{b:02X}");
            }
        }
    }
    out
}
