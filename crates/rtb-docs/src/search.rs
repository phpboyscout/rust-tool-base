//! Title + full-text search over the doc tree.
//!
//! Two code paths:
//!
//! - **Title search** — `fuzzy-matcher::SkimMatcherV2` against the
//!   index's entry titles. Fast, no preprocessing, suitable for the
//!   "I know roughly what the page is called" query.
//! - **Full-text search** — `tantivy` in-memory (`RamDirectory`)
//!   index built once at [`SearchIndex::build`]. Matches tokens in
//!   the plain-text projection of every page's body and returns
//!   top-N matches with a 140-character snippet.
//!
//! The `DocsBrowser` TUI and the `DocsServer::search_handler` both
//! call into [`SearchIndex`].

use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher as _;
use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;
use tantivy::schema::{Schema, STORED, TEXT};
use tantivy::{doc, Index, IndexWriter, TantivyDocument};

use crate::error::Result;
use crate::render;

/// Result of a title search — one row per hit, ordered by descending
/// fuzzy score.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TitleHit {
    /// Doc-tree-relative page path.
    pub path: String,
    /// The title as stored on the index entry.
    pub title: String,
    /// Fuzzy score (higher = better match).
    pub score: i64,
}

/// Result of a full-text search.
#[derive(Debug, Clone)]
pub struct FullTextHit {
    /// Doc-tree-relative page path.
    pub path: String,
    /// The page's title.
    pub title: String,
    /// Up to 140 chars of the matched body, with the match token
    /// preserved.
    pub snippet: String,
    /// tantivy score (higher = better match).
    pub score: f32,
}

/// In-memory tantivy-backed full-text index over the doc tree.
pub struct SearchIndex {
    index: Index,
    query_parser: QueryParser,
    title_field: tantivy::schema::Field,
    path_field: tantivy::schema::Field,
    body_field: tantivy::schema::Field,
    titles: Vec<(String, String)>, // (path, title) — for title search
    matcher: SkimMatcherV2,
}

impl SearchIndex {
    /// Build an index from a list of `(path, title, body)` tuples.
    ///
    /// # Errors
    ///
    /// [`DocsError::Search`](crate::error::DocsError::Search) on any
    /// tantivy-internal failure.
    pub fn build(pages: &[(String, String, String)]) -> Result<Self> {
        let mut schema_builder = Schema::builder();
        let path_field = schema_builder.add_text_field("path", STORED);
        let title_field = schema_builder.add_text_field("title", TEXT | STORED);
        let body_field = schema_builder.add_text_field("body", TEXT | STORED);
        let schema = schema_builder.build();

        let index = Index::create_in_ram(schema);
        {
            let mut writer: IndexWriter = index.writer(50_000_000)?;
            for (path, title, body) in pages {
                let plain = render::to_plain_text(body);
                writer.add_document(doc!(
                    path_field => path.as_str(),
                    title_field => title.as_str(),
                    body_field => plain,
                ))?;
            }
            writer.commit()?;
        }

        let query_parser = QueryParser::for_index(&index, vec![title_field, body_field]);
        let titles: Vec<(String, String)> =
            pages.iter().map(|(p, t, _)| (p.clone(), t.clone())).collect();

        Ok(Self {
            index,
            query_parser,
            title_field,
            path_field,
            body_field,
            titles,
            matcher: SkimMatcherV2::default(),
        })
    }

    /// Fuzzy-match `query` against every stored title. Empty query
    /// returns all titles in insertion order with score 0.
    #[must_use]
    pub fn title_search(&self, query: &str) -> Vec<TitleHit> {
        if query.is_empty() {
            return self
                .titles
                .iter()
                .map(|(path, title)| TitleHit {
                    path: path.clone(),
                    title: title.clone(),
                    score: 0,
                })
                .collect();
        }
        let mut hits: Vec<TitleHit> = self
            .titles
            .iter()
            .filter_map(|(path, title)| {
                self.matcher.fuzzy_match(title, query).map(|score| TitleHit {
                    path: path.clone(),
                    title: title.clone(),
                    score,
                })
            })
            .collect();
        hits.sort_by_key(|h| std::cmp::Reverse(h.score));
        hits
    }

    /// Full-text search. Returns up to `limit` hits, highest-score
    /// first.
    ///
    /// # Errors
    ///
    /// Surfaces tantivy parse / search failures.
    pub fn full_text_search(&self, query: &str, limit: usize) -> Result<Vec<FullTextHit>> {
        if query.trim().is_empty() || limit == 0 {
            return Ok(Vec::new());
        }
        let reader = self.index.reader()?;
        let searcher = reader.searcher();
        let Ok(parsed) = self.query_parser.parse_query(query) else {
            // Malformed queries produce no hits rather than failing
            // loudly — the user can't always control what they type.
            return Ok(Vec::new());
        };
        let top_docs = searcher.search(&parsed, &TopDocs::with_limit(limit))?;
        let mut out = Vec::with_capacity(top_docs.len());
        for (score, doc_address) in top_docs {
            let retrieved: TantivyDocument = searcher.doc(doc_address)?;
            let path = extract_first_text(&retrieved, self.path_field).unwrap_or_default();
            let title = extract_first_text(&retrieved, self.title_field).unwrap_or_default();
            let body_text = extract_first_text(&retrieved, self.body_field).unwrap_or_default();
            let snippet = build_snippet(&body_text, query);
            out.push(FullTextHit { path, title, snippet, score });
        }
        Ok(out)
    }
}

fn extract_first_text(document: &TantivyDocument, field: tantivy::schema::Field) -> Option<String> {
    use tantivy::schema::Value;
    document.get_all(field).next().and_then(|v| v.as_str().map(str::to_string))
}

fn build_snippet(body: &str, query: &str) -> String {
    let lowered = body.to_ascii_lowercase();
    let q = query.to_ascii_lowercase();
    let Some(idx) = lowered.find(&q) else {
        // No literal match — return the head of the body.
        return truncate(body, 140);
    };
    let start = idx.saturating_sub(30);
    let end = (idx + q.len() + 110).min(body.len());
    let prefix = if start > 0 { "…" } else { "" };
    let suffix = if end < body.len() { "…" } else { "" };
    format!("{prefix}{}{suffix}", &body[start..end])
}

fn truncate(s: &str, max_bytes: usize) -> String {
    if s.len() <= max_bytes {
        return s.to_string();
    }
    let mut end = max_bytes;
    while !s.is_char_boundary(end) && end > 0 {
        end -= 1;
    }
    format!("{}…", &s[..end])
}

// ---------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_pages() -> Vec<(String, String, String)> {
        vec![
            (
                "intro.md".into(),
                "Introduction".into(),
                "# Introduction\n\nWelcome to the tool.".into(),
            ),
            (
                "config.md".into(),
                "Configuration".into(),
                "# Configuration\n\nThe tool reads `config.yaml` for settings.".into(),
            ),
            (
                "shutdown.md".into(),
                "App context".into(),
                "# App context\n\nUse a cancellation token to coordinate shutdown.".into(),
            ),
        ]
    }

    #[test]
    fn build_succeeds_on_empty_input() {
        let idx = SearchIndex::build(&[]).expect("empty build");
        assert!(idx.title_search("anything").is_empty());
    }

    #[test]
    fn title_search_is_fuzzy() {
        let idx = SearchIndex::build(&sample_pages()).expect("build");
        let hits = idx.title_search("conf");
        assert!(!hits.is_empty(), "'conf' should fuzzy-match Configuration");
        assert_eq!(hits[0].title, "Configuration");
    }

    #[test]
    fn title_search_ranks_best_match_first() {
        let idx = SearchIndex::build(&sample_pages()).expect("build");
        let hits = idx.title_search("intro");
        assert_eq!(hits[0].title, "Introduction");
    }

    #[test]
    fn full_text_search_finds_body_tokens() {
        let idx = SearchIndex::build(&sample_pages()).expect("build");
        let hits = idx.full_text_search("cancellation", 5).expect("search");
        assert_eq!(hits.len(), 1, "'cancellation' should hit shutdown.md only");
        assert_eq!(hits[0].path, "shutdown.md");
        assert!(
            hits[0].snippet.to_lowercase().contains("cancellation"),
            "snippet should contain the query: {}",
            hits[0].snippet
        );
    }

    #[test]
    fn full_text_search_honours_limit() {
        let idx = SearchIndex::build(&sample_pages()).expect("build");
        let hits = idx.full_text_search("tool", 1).expect("search");
        assert_eq!(hits.len(), 1);
    }

    #[test]
    fn full_text_search_rejects_empty_query() {
        let idx = SearchIndex::build(&sample_pages()).expect("build");
        let hits = idx.full_text_search("", 5).expect("search");
        assert!(hits.is_empty());
    }

    #[test]
    fn build_snippet_centers_on_match() {
        let body = "The quick brown fox jumps over the lazy dog for testing purposes";
        let snippet = build_snippet(body, "jumps");
        assert!(snippet.contains("jumps"));
    }
}
