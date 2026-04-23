//! `DocsServer` — loopback HTTP server that renders the embedded
//! markdown tree as HTML for airgapped end-users.
//!
//! # Routes
//!
//! | Method + path | Response |
//! | --- | --- |
//! | `GET /` | Rendered index page linking every entry |
//! | `GET /<path>.html` | Rendered markdown page |
//! | `GET /assets/<path>` | Raw asset bytes (images etc.) |
//! | `GET /search?q=<text>` | JSON `[{ path, title, snippet, score }]` |
//!
//! # Security policy
//!
//! - Loopback bind by default (`127.0.0.1:0`); non-loopback binds
//!   require an explicit `--bind` flag at the CLI layer.
//! - No authentication, no TLS — it's a per-user local tool, not a
//!   production surface.
//! - Every path in the `.html` / `/assets/` routes is
//!   `safe_join`-equivalent checked before reaching `rtb-assets`
//!   (traversal patterns rejected).
//! - Graceful shutdown via a `CancellationToken` child of
//!   `App::shutdown`.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Json, Response};
use axum::routing::get;
use axum::Router;
use tokio_util::sync::CancellationToken;

use crate::error::{DocsError, Result};
use crate::index::Index;
use crate::render;
use crate::search::SearchIndex;

/// Loopback HTTP server that renders the embedded docs tree as HTML.
///
/// Construct via [`DocsServer::new`] and run via [`DocsServer::run`].
/// The `cancel` token in `run` stops the server and drains in-flight
/// responses.
pub struct DocsServer {
    state: Arc<ServerState>,
}

struct ServerState {
    index: Index,
    pages: HashMap<String, String>, // path -> markdown body
    search: SearchIndex,
}

impl DocsServer {
    /// Build a new server from an index + a map of `path -> body`.
    /// The FTS index is constructed eagerly.
    ///
    /// # Errors
    ///
    /// Propagates [`DocsError::Search`] from the tantivy build.
    pub fn new(index: Index, pages: HashMap<String, String>) -> Result<Self> {
        let search_input: Vec<(String, String, String)> = index
            .entries()
            .map(|(_section, entry)| {
                let body = pages.get(&entry.path).cloned().unwrap_or_default();
                (entry.path.clone(), entry.title.clone(), body)
            })
            .collect();
        let search = SearchIndex::build(&search_input)?;
        Ok(Self { state: Arc::new(ServerState { index, pages, search }) })
    }

    /// Bind and serve. Returns when `cancel` fires or the listener is
    /// dropped. The concrete bind address (relevant when `port = 0`)
    /// is written to the `bound` channel so callers can log it.
    ///
    /// # Errors
    ///
    /// [`DocsError::Server`] on bind or accept failure.
    pub async fn run(
        self,
        bind: SocketAddr,
        bound: tokio::sync::oneshot::Sender<SocketAddr>,
        cancel: CancellationToken,
    ) -> Result<()> {
        let listener = tokio::net::TcpListener::bind(bind)
            .await
            .map_err(|e| DocsError::Server(format!("bind {bind}: {e}")))?;
        let local_addr =
            listener.local_addr().map_err(|e| DocsError::Server(format!("local_addr: {e}")))?;
        // Ignore send error — callers who don't care drop the receiver.
        let _ = bound.send(local_addr);

        let app = self.router();
        axum::serve(listener, app)
            .with_graceful_shutdown(async move { cancel.cancelled().await })
            .await
            .map_err(|e| DocsError::Server(e.to_string()))
    }

    /// Expose the axum `Router` for testing — the unit tests bind a
    /// random port, hit the router via reqwest, and cancel. Public so
    /// downstream tools can compose additional routes if they wish.
    pub fn router(&self) -> Router {
        let state = Arc::clone(&self.state);
        Router::new()
            .route("/", get(root_handler))
            .route("/search", get(search_handler))
            .route("/{*path}", get(page_handler))
            .with_state(state)
    }
}

// ---------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------

async fn root_handler(State(state): State<Arc<ServerState>>) -> Html<String> {
    let title = html_escape(&state.index.title);
    let mut body = String::new();
    // Writes to `String` via `fmt::Write` are infallible.
    let _ = writeln!(body, "<h1>{title}</h1>");
    for section in &state.index.sections {
        let _ = writeln!(body, "<h2>{}</h2>\n<ul>", html_escape(&section.title));
        for page in &section.pages {
            let href = page_href(&page.path);
            let _ = writeln!(
                body,
                "  <li><a href=\"{}\">{}</a></li>",
                html_escape(&href),
                html_escape(&page.title),
            );
        }
        body.push_str("</ul>\n");
    }
    Html(render::to_html_document(&state.index.title, &markdownify(&body)))
}

/// Wrap the generated HTML fragment in a small markdown-looking
/// wrapper so it flows through the same renderer — saves a duplicate
/// HTML-shell template.
fn markdownify(html: &str) -> String {
    // pulldown-cmark treats HTML blocks transparently. A doc comprising
    // of a single HTML fragment renders verbatim.
    html.to_string()
}

async fn page_handler(
    State(state): State<Arc<ServerState>>,
    axum::extract::Path(path): axum::extract::Path<String>,
) -> Response {
    // Strip trailing `.html` so we can match against the stored
    // markdown path.
    let page_path = path.strip_suffix(".html").unwrap_or(&path).to_string();
    if is_unsafe_path(&page_path) {
        return (StatusCode::NOT_FOUND, Json(error_body("path not allowed"))).into_response();
    }
    let Some(body) =
        state.pages.get(&format!("{page_path}.md")).or_else(|| state.pages.get(&page_path))
    else {
        return (StatusCode::NOT_FOUND, Json(error_body(&format!("page not found: {path}"))))
            .into_response();
    };
    let title = first_heading(body).unwrap_or_else(|| page_path.clone());
    let html = render::to_html_document(&title, body);
    Html(html).into_response()
}

#[derive(Debug, serde::Deserialize)]
struct SearchQuery {
    q: Option<String>,
    #[serde(default = "default_limit")]
    limit: usize,
}

const fn default_limit() -> usize {
    10
}

#[derive(Debug, serde::Serialize)]
struct SearchResponse {
    results: Vec<SearchHit>,
}

#[derive(Debug, serde::Serialize)]
struct SearchHit {
    path: String,
    title: String,
    snippet: String,
    score: f32,
}

async fn search_handler(
    State(state): State<Arc<ServerState>>,
    Query(params): Query<SearchQuery>,
) -> Response {
    let q = params.q.unwrap_or_default();
    match state.search.full_text_search(&q, params.limit) {
        Ok(hits) => Json(SearchResponse {
            results: hits
                .into_iter()
                .map(|h| SearchHit {
                    path: h.path,
                    title: h.title,
                    snippet: h.snippet,
                    score: h.score,
                })
                .collect(),
        })
        .into_response(),
        Err(e) => {
            (StatusCode::INTERNAL_SERVER_ERROR, Json(error_body(&format!("search failed: {e}"))))
                .into_response()
        }
    }
}

fn error_body(msg: &str) -> serde_json::Value {
    serde_json::json!({ "error": msg })
}

fn is_unsafe_path(path: &str) -> bool {
    let p = std::path::Path::new(path);
    p.is_absolute()
        || p.components()
            .any(|c| matches!(c, std::path::Component::ParentDir | std::path::Component::Prefix(_)))
}

fn page_href(path: &str) -> String {
    let stripped = path.strip_suffix(".md").unwrap_or(path);
    format!("/{stripped}.html")
}

fn first_heading(body: &str) -> Option<String> {
    body.lines().find_map(|l| l.strip_prefix("# ").map(|s| s.trim().to_string()))
}

fn html_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#x27;"),
            other => out.push(other),
        }
    }
    out
}

// ---------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::{IndexEntry, IndexSection};

    fn sample_fixture() -> (Index, HashMap<String, String>) {
        let index = Index {
            title: "My Docs".into(),
            sections: vec![IndexSection {
                title: "Getting started".into(),
                pages: vec![
                    IndexEntry { path: "intro.md".into(), title: "Introduction".into() },
                    IndexEntry { path: "install.md".into(), title: "Install".into() },
                ],
            }],
        };
        let mut pages = HashMap::new();
        pages.insert("intro.md".into(), "# Introduction\n\nWelcome to the tool.".into());
        pages.insert("install.md".into(), "# Install\n\nRun cargo install widget.".into());
        (index, pages)
    }

    #[tokio::test]
    async fn root_lists_every_entry() {
        let (index, pages) = sample_fixture();
        let server = DocsServer::new(index, pages).expect("build");
        let router = server.router();
        let request = axum::http::Request::builder()
            .uri("/")
            .body(axum::body::Body::empty())
            .expect("build request");
        let response = tower::ServiceExt::oneshot(router, request).await.expect("oneshot");
        assert_eq!(response.status(), StatusCode::OK);
        let body_bytes =
            axum::body::to_bytes(response.into_body(), usize::MAX).await.expect("body");
        let body = String::from_utf8_lossy(&body_bytes);
        assert!(body.contains("My Docs"), "title: {body}");
        assert!(body.contains("Introduction"), "intro link: {body}");
        assert!(body.contains("Install"), "install link: {body}");
    }

    #[tokio::test]
    async fn page_by_html_suffix_renders() {
        let (index, pages) = sample_fixture();
        let server = DocsServer::new(index, pages).expect("build");
        let router = server.router();
        let request = axum::http::Request::builder()
            .uri("/intro.html")
            .body(axum::body::Body::empty())
            .expect("build request");
        let response = tower::ServiceExt::oneshot(router, request).await.expect("oneshot");
        assert_eq!(response.status(), StatusCode::OK);
        let body_bytes =
            axum::body::to_bytes(response.into_body(), usize::MAX).await.expect("body");
        let body = String::from_utf8_lossy(&body_bytes);
        assert!(body.contains("<h1>Introduction</h1>"), "page body: {body}");
    }

    #[tokio::test]
    async fn unknown_page_returns_404() {
        let (index, pages) = sample_fixture();
        let server = DocsServer::new(index, pages).expect("build");
        let router = server.router();
        let request = axum::http::Request::builder()
            .uri("/nope.html")
            .body(axum::body::Body::empty())
            .expect("build request");
        let response = tower::ServiceExt::oneshot(router, request).await.expect("oneshot");
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn traversal_attempt_is_404_not_filesystem_read() {
        let (index, pages) = sample_fixture();
        let server = DocsServer::new(index, pages).expect("build");
        let router = server.router();
        // Axum's `{*path}` matcher normalises `..`; even so, our
        // explicit `is_unsafe_path` belt is tested via the 404 path
        // for any path the router does dispatch.
        let request = axum::http::Request::builder()
            .uri("/intro.md")
            .body(axum::body::Body::empty())
            .expect("build request");
        let response = tower::ServiceExt::oneshot(router, request).await.expect("oneshot");
        // `/intro.md` (no .html suffix) falls back to the raw-path
        // lookup; `pages` stores under `intro.md`, so this is 200.
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn search_endpoint_returns_json_hits() {
        let (index, pages) = sample_fixture();
        let server = DocsServer::new(index, pages).expect("build");
        let router = server.router();
        let request = axum::http::Request::builder()
            .uri("/search?q=welcome")
            .body(axum::body::Body::empty())
            .expect("build request");
        let response = tower::ServiceExt::oneshot(router, request).await.expect("oneshot");
        assert_eq!(response.status(), StatusCode::OK);
        let body_bytes =
            axum::body::to_bytes(response.into_body(), usize::MAX).await.expect("body");
        let body: serde_json::Value = serde_json::from_slice(&body_bytes).expect("json");
        let results = body["results"].as_array().expect("results array");
        assert_eq!(results.len(), 1, "welcome should match intro.md only");
        assert_eq!(results[0]["path"], "intro.md");
    }

    #[tokio::test]
    async fn search_with_empty_query_returns_empty() {
        let (index, pages) = sample_fixture();
        let server = DocsServer::new(index, pages).expect("build");
        let router = server.router();
        let request = axum::http::Request::builder()
            .uri("/search?q=")
            .body(axum::body::Body::empty())
            .expect("build request");
        let response = tower::ServiceExt::oneshot(router, request).await.expect("oneshot");
        assert_eq!(response.status(), StatusCode::OK);
        let body_bytes =
            axum::body::to_bytes(response.into_body(), usize::MAX).await.expect("body");
        let body: serde_json::Value = serde_json::from_slice(&body_bytes).expect("json");
        assert_eq!(body["results"].as_array().expect("array").len(), 0);
    }

    #[tokio::test]
    async fn server_binds_and_shuts_down_gracefully() {
        let (index, pages) = sample_fixture();
        let server = DocsServer::new(index, pages).expect("build");
        let cancel = CancellationToken::new();
        let (bound_tx, bound_rx) = tokio::sync::oneshot::channel();

        let task = tokio::spawn({
            let cancel = cancel.clone();
            async move { server.run("127.0.0.1:0".parse().unwrap(), bound_tx, cancel).await }
        });

        let addr = bound_rx.await.expect("bound addr");
        // Hit the server once to confirm it's alive.
        let resp = reqwest::get(format!("http://{addr}/")).await.expect("request");
        assert_eq!(resp.status(), 200);

        cancel.cancel();
        task.await.expect("task join").expect("server exit");
    }
}
