#[derive(Debug, Clone)]
pub struct Page {
    pub url: String,
    pub title: String,
    pub text: String,
    pub links: Vec<String>,
    pub depth: usize,
}

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub url: String,
    pub title: String,
    pub snippet: String,
    pub score: f64,
    pub matched_terms: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchAlgorithm {
    Tf,
    TfIdf,
    Bm25,
    Boolean,
    Phrase,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WordSuggestion {
    pub term: String,
    pub frequency: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Correction {
    pub term: String,
    pub distance: usize,
    pub frequency: u64,
}

#[derive(Debug, Default)]
pub struct CrawlStats {
    pub visited: usize,
    pub indexed: usize,
    pub skipped: usize,
    pub failed: usize,
}
