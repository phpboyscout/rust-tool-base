//! `DocsError` — every failure mode the browser / server / search
//! path can surface.

use std::sync::Arc;

/// Every failure mode surfaced by `rtb-docs`.
#[derive(Debug, thiserror::Error, miette::Diagnostic, Clone)]
#[non_exhaustive]
pub enum DocsError {
    /// The configured `root` doesn't exist in the asset tree.
    #[error("docs root not found in assets: {0}")]
    #[diagnostic(code(rtb::docs::root_missing))]
    RootMissing(String),

    /// `_index.yaml` was present but couldn't be parsed.
    #[error("index file not found or malformed: {0}")]
    #[diagnostic(code(rtb::docs::index_malformed))]
    IndexMalformed(String),

    /// Markdown parse failed on a specific page.
    #[error("markdown parse failed for {path}: {reason}")]
    #[diagnostic(code(rtb::docs::markdown_error))]
    MarkdownError {
        /// The doc-tree-relative path that failed to parse.
        path: String,
        /// The inner parser error message.
        reason: String,
    },

    /// Terminal initialisation (raw mode / alternate screen) failed.
    #[error("terminal initialisation failed: {0}")]
    #[diagnostic(code(rtb::docs::terminal))]
    Terminal(String),

    /// `docs ask` invoked without the `ai` feature compiled in.
    #[error("AI feature not enabled")]
    #[diagnostic(
        code(rtb::docs::ai_disabled),
        help("rebuild with `--features ai` to enable `docs ask`")
    )]
    AiDisabled,

    /// Wrapped `rtb-assets::AssetError`. Carried as a message since
    /// `AssetError` is not `Clone` and we want `DocsError: Clone`.
    #[error("asset error: {0}")]
    #[diagnostic(code(rtb::docs::assets))]
    Assets(String),

    /// Wrapped `tantivy::TantivyError`. Carried as a message since
    /// `tantivy`'s error type is not `Clone`.
    #[error("full-text search index error: {0}")]
    #[diagnostic(code(rtb::docs::search))]
    Search(String),

    /// Wrapped `hyper` / `axum` server error.
    #[error("docs server error: {0}")]
    #[diagnostic(code(rtb::docs::server))]
    Server(String),

    /// I/O error surfaced through the cache-dir or server path.
    #[error("I/O error: {0}")]
    #[diagnostic(code(rtb::docs::io))]
    Io(#[from] Arc<std::io::Error>),
}

impl From<std::io::Error> for DocsError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(Arc::new(err))
    }
}

impl From<tantivy::TantivyError> for DocsError {
    fn from(err: tantivy::TantivyError) -> Self {
        Self::Search(err.to_string())
    }
}

impl From<rtb_assets::AssetError> for DocsError {
    fn from(err: rtb_assets::AssetError) -> Self {
        Self::Assets(err.to_string())
    }
}

/// `Result<T, DocsError>`.
pub type Result<T> = std::result::Result<T, DocsError>;
