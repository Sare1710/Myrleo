use crate::bk_tree::BkTree;
use crate::model::{Correction, Page, SearchAlgorithm, SearchResult, WordSuggestion};
use crate::query::{BooleanToken, parse_boolean_query, query_terms};
use crate::tokenizer::{positions, tokenize};
use crate::trie::Trie;
use anyhow::{Context, Result, anyhow, bail};
use rusqlite::{Connection, OptionalExtension, params};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

pub struct Database {
    connection: Connection,
}

#[derive(Debug)]
struct Candidate {
    url: String,
    title: String,
    text: String,
    score: f64,
    pagerank: f64,
    matched_terms: HashSet<String>,
}

impl Database {
    pub fn open(path: &Path) -> Result<Self> {
        let connection = Connection::open(path)
            .with_context(|| format!("nu pot deschide baza de date {}", path.display()))?;
        connection.execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA journal_mode = WAL;
             CREATE TABLE IF NOT EXISTS pages (
                 id           INTEGER PRIMARY KEY,
                 url          TEXT NOT NULL UNIQUE,
                 title        TEXT NOT NULL,
                 text         TEXT NOT NULL,
                 depth        INTEGER NOT NULL,
                 token_count  INTEGER NOT NULL,
                 content_hash TEXT NOT NULL,
                 crawled_at   INTEGER NOT NULL,
                 pagerank     REAL NOT NULL DEFAULT 0.0
             );
             CREATE INDEX IF NOT EXISTS pages_hash_idx ON pages(content_hash);

             CREATE TABLE IF NOT EXISTS postings (
                 term            TEXT NOT NULL,
                 page_id         INTEGER NOT NULL REFERENCES pages(id) ON DELETE CASCADE,
                 body_frequency  INTEGER NOT NULL,
                 title_frequency INTEGER NOT NULL,
                 positions       TEXT NOT NULL,
                 PRIMARY KEY (term, page_id)
             );
             CREATE INDEX IF NOT EXISTS postings_page_idx ON postings(page_id);

             CREATE TABLE IF NOT EXISTS links (
                 from_page_id INTEGER NOT NULL REFERENCES pages(id) ON DELETE CASCADE,
                 to_url       TEXT NOT NULL,
                 PRIMARY KEY (from_page_id, to_url)
             );",
        )?;
        Ok(Self { connection })
    }

    pub fn contains_hash(&self, hash: &str) -> Result<bool> {
        let found = self
            .connection
            .query_row(
                "SELECT 1 FROM pages WHERE content_hash = ?1 LIMIT 1",
                [hash],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        Ok(found)
    }

    pub fn index_page(&mut self, page: &Page, content_hash: &str) -> Result<()> {
        let body_positions = positions(&page.text);
        let title_terms = tokenize(&page.title);
        let mut title_frequencies: HashMap<String, usize> = HashMap::new();
        for term in title_terms {
            *title_frequencies.entry(term).or_default() += 1;
        }
        let token_count: usize = body_positions.values().map(Vec::len).sum();
        let crawled_at = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() as i64;

        let transaction = self.connection.transaction()?;
        let page_id: i64 = transaction.query_row(
            "INSERT INTO pages(url, title, text, depth, token_count, content_hash, crawled_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(url) DO UPDATE SET
                 title = excluded.title,
                 text = excluded.text,
                 depth = excluded.depth,
                 token_count = excluded.token_count,
                 content_hash = excluded.content_hash,
                 crawled_at = excluded.crawled_at
             RETURNING id",
            params![
                page.url,
                page.title,
                page.text,
                page.depth as i64,
                token_count as i64,
                content_hash,
                crawled_at
            ],
            |row| row.get(0),
        )?;

        transaction.execute("DELETE FROM postings WHERE page_id = ?1", [page_id])?;
        transaction.execute("DELETE FROM links WHERE from_page_id = ?1", [page_id])?;

        let mut all_terms: HashSet<&str> = body_positions.keys().map(String::as_str).collect();
        all_terms.extend(title_frequencies.keys().map(String::as_str));

        {
            let mut statement = transaction.prepare(
                "INSERT INTO postings(term, page_id, body_frequency, title_frequency, positions)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )?;
            for term in all_terms {
                let term_positions = body_positions.get(term).map(Vec::as_slice).unwrap_or(&[]);
                let encoded_positions = term_positions
                    .iter()
                    .map(usize::to_string)
                    .collect::<Vec<_>>()
                    .join(",");
                statement.execute(params![
                    term,
                    page_id,
                    term_positions.len() as i64,
                    title_frequencies.get(term).copied().unwrap_or(0) as i64,
                    encoded_positions
                ])?;
            }
        }

        {
            let mut statement = transaction
                .prepare("INSERT OR IGNORE INTO links(from_page_id, to_url) VALUES (?1, ?2)")?;
            for link in &page.links {
                statement.execute(params![page_id, link])?;
            }
        }

        transaction.commit()?;
        Ok(())
    }

    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        self.search_with_algorithm(query, limit, SearchAlgorithm::Bm25)
    }

    pub fn search_with_algorithm(
        &self,
        query: &str,
        limit: usize,
        algorithm: SearchAlgorithm,
    ) -> Result<Vec<SearchResult>> {
        if algorithm == SearchAlgorithm::Boolean {
            return self.search_boolean(query, limit);
        }
        if algorithm == SearchAlgorithm::Phrase {
            return self.search_phrase(query, limit);
        }

        let query_terms: HashSet<String> = tokenize(query).into_iter().collect();
        if query_terms.is_empty() || limit == 0 {
            return Ok(Vec::new());
        }

        let (document_count, average_length): (i64, f64) = self.connection.query_row(
            "SELECT COUNT(*), COALESCE(AVG(token_count), 0.0) FROM pages",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        if document_count == 0 {
            return Ok(Vec::new());
        }

        let mut candidates: HashMap<i64, Candidate> = HashMap::new();
        const K1: f64 = 1.2;
        const B: f64 = 0.75;

        for term in query_terms {
            let mut statement = self.connection.prepare(
                "SELECT p.id, p.url, p.title, p.text, p.token_count, p.pagerank,
                        x.body_frequency, x.title_frequency
                 FROM postings x
                 JOIN pages p ON p.id = x.page_id
                 WHERE x.term = ?1",
            )?;
            let rows = statement.query_map([&term], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, f64>(5)?,
                    row.get::<_, i64>(6)?,
                    row.get::<_, i64>(7)?,
                ))
            })?;
            let matches = rows.collect::<rusqlite::Result<Vec<_>>>()?;
            let document_frequency = matches.len() as f64;
            if document_frequency == 0.0 {
                continue;
            }
            let bm25_idf = (1.0
                + (document_count as f64 - document_frequency + 0.5) / (document_frequency + 0.5))
                .ln();
            let tf_idf = ((document_count as f64 + 1.0) / (document_frequency + 1.0)).ln() + 1.0;

            for (id, url, title, text, length, pagerank, body_tf, title_tf) in matches {
                let length_ratio = if average_length > 0.0 {
                    length as f64 / average_length
                } else {
                    1.0
                };
                let tf = body_tf as f64;
                let (body_score, title_score) = match algorithm {
                    SearchAlgorithm::Tf => (tf, title_tf as f64 * 2.5),
                    SearchAlgorithm::TfIdf => (
                        if tf > 0.0 {
                            (1.0 + tf.ln()) * tf_idf
                        } else {
                            0.0
                        },
                        if title_tf > 0 {
                            (1.0 + (title_tf as f64).ln()) * tf_idf * 2.5
                        } else {
                            0.0
                        },
                    ),
                    SearchAlgorithm::Bm25 => (
                        if tf > 0.0 {
                            bm25_idf * (tf * (K1 + 1.0)) / (tf + K1 * (1.0 - B + B * length_ratio))
                        } else {
                            0.0
                        },
                        if title_tf > 0 {
                            bm25_idf * 2.5 * (1.0 + (title_tf as f64).ln())
                        } else {
                            0.0
                        },
                    ),
                    SearchAlgorithm::Boolean | SearchAlgorithm::Phrase => unreachable!(),
                };

                let candidate = candidates.entry(id).or_insert_with(|| Candidate {
                    url,
                    title,
                    text,
                    score: 0.0,
                    pagerank,
                    matched_terms: HashSet::new(),
                });
                candidate.score += body_score + title_score;
                candidate.matched_terms.insert(term.clone());
            }
        }

        if algorithm == SearchAlgorithm::Bm25 {
            for candidate in candidates.values_mut() {
                candidate.score += 0.25 * (1.0 + candidate.pagerank * document_count as f64).ln();
            }
        }

        let mut results = candidates
            .into_values()
            .map(|candidate| {
                let mut matched_terms = candidate.matched_terms.into_iter().collect::<Vec<_>>();
                matched_terms.sort();
                SearchResult {
                    url: candidate.url,
                    title: candidate.title,
                    snippet: make_snippet(&candidate.text, 220),
                    score: candidate.score,
                    matched_terms,
                }
            })
            .collect::<Vec<_>>();
        results.sort_by(|left, right| {
            right
                .score
                .total_cmp(&left.score)
                .then_with(|| left.url.cmp(&right.url))
        });
        results.truncate(limit);
        Ok(results)
    }

    fn search_phrase(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        let phrase = tokenize(query);
        if phrase.is_empty() || limit == 0 {
            return Ok(Vec::new());
        }

        let position_maps = phrase
            .iter()
            .map(|term| self.positions_for_term(term))
            .collect::<Result<Vec<_>>>()?;
        let Some(first_map) = position_maps.first() else {
            return Ok(Vec::new());
        };
        let unique_terms = phrase.iter().cloned().collect::<HashSet<_>>();
        let mut results = Vec::new();

        for page_id in first_map.keys() {
            if !position_maps.iter().all(|map| map.contains_key(page_id)) {
                continue;
            }
            let body_occurrences = phrase_occurrences(*page_id, &position_maps);
            let (url, title, text, pagerank): (String, String, String, f64) =
                self.connection.query_row(
                    "SELECT url, title, text, pagerank FROM pages WHERE id = ?1",
                    [page_id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                )?;
            let title_occurrences = sequence_occurrences(&tokenize(&title), &phrase);
            if body_occurrences == 0 && title_occurrences == 0 {
                continue;
            }

            let mut matched_terms = unique_terms.iter().cloned().collect::<Vec<_>>();
            matched_terms.sort();
            results.push(SearchResult {
                url,
                title,
                snippet: make_snippet(&text, 220),
                score: body_occurrences as f64 + title_occurrences as f64 * 2.5 + pagerank * 0.25,
                matched_terms,
            });
        }

        results.sort_by(|left, right| {
            right
                .score
                .total_cmp(&left.score)
                .then_with(|| left.url.cmp(&right.url))
        });
        results.truncate(limit);
        Ok(results)
    }

    pub fn autocomplete(&self, prefix: &str, limit: usize) -> Result<Vec<WordSuggestion>> {
        let normalized_prefix = tokenize(prefix).into_iter().next().unwrap_or_default();
        if normalized_prefix.is_empty() {
            return Ok(Vec::new());
        }
        let mut trie = Trie::default();
        for (term, frequency) in self.vocabulary()? {
            trie.insert(term, frequency);
        }
        Ok(trie.complete(&normalized_prefix, limit))
    }

    pub fn corrections(
        &self,
        term: &str,
        max_distance: usize,
        limit: usize,
    ) -> Result<Vec<Correction>> {
        let normalized_term = tokenize(term).into_iter().next().unwrap_or_default();
        if normalized_term.is_empty() {
            return Ok(Vec::new());
        }
        let mut tree = BkTree::default();
        for (term, frequency) in self.vocabulary()? {
            tree.insert(term, frequency);
        }
        Ok(tree.search(&normalized_term, max_distance, limit))
    }

    fn vocabulary(&self) -> Result<Vec<(String, u64)>> {
        let mut statement = self.connection.prepare(
            "SELECT term, SUM(body_frequency + title_frequency) AS frequency
             FROM postings GROUP BY term",
        )?;
        let values = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as u64))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(values)
    }

    fn positions_for_term(&self, term: &str) -> Result<HashMap<i64, Vec<usize>>> {
        let mut statement = self
            .connection
            .prepare("SELECT page_id, positions FROM postings WHERE term = ?1")?;
        let rows = statement.query_map([term], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?;
        let mut result = HashMap::new();
        for row in rows {
            let (page_id, encoded) = row?;
            let positions = encoded
                .split(',')
                .filter(|value| !value.is_empty())
                .map(str::parse)
                .collect::<std::result::Result<Vec<usize>, _>>()?;
            result.insert(page_id, positions);
        }
        Ok(result)
    }

    fn search_boolean(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        let expression = parse_boolean_query(query)?;
        let matching_ids = self.evaluate_boolean_expression(&expression)?;
        let terms = query_terms(&expression);
        let mut results = Vec::new();

        for page_id in matching_ids {
            let (url, title, text, pagerank): (String, String, String, f64) =
                self.connection.query_row(
                    "SELECT url, title, text, pagerank FROM pages WHERE id = ?1",
                    [page_id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                )?;
            let document_terms: HashSet<String> =
                tokenize(&format!("{title} {text}")).into_iter().collect();
            let matched_terms = terms
                .iter()
                .filter(|term| document_terms.contains(*term))
                .cloned()
                .collect();
            results.push(SearchResult {
                url,
                title,
                snippet: make_snippet(&text, 220),
                score: pagerank,
                matched_terms,
            });
        }

        results.sort_by(|left, right| {
            right
                .score
                .total_cmp(&left.score)
                .then_with(|| left.url.cmp(&right.url))
        });
        results.truncate(limit);
        Ok(results)
    }

    fn evaluate_boolean_expression(&self, expression: &[BooleanToken]) -> Result<HashSet<i64>> {
        let universe = self.page_ids()?;
        let mut stack: Vec<HashSet<i64>> = Vec::new();

        for token in expression {
            match token {
                BooleanToken::Term(term) => stack.push(self.page_ids_for_term(term)?),
                BooleanToken::Not => {
                    let operand = stack
                        .pop()
                        .ok_or_else(|| anyhow!("operatorul NOT nu are un termen"))?;
                    stack.push(universe.difference(&operand).copied().collect());
                }
                BooleanToken::And | BooleanToken::Or => {
                    let right = stack
                        .pop()
                        .ok_or_else(|| anyhow!("operator boolean fără operandul din dreapta"))?;
                    let left = stack
                        .pop()
                        .ok_or_else(|| anyhow!("operator boolean fără operandul din stânga"))?;
                    let result = if *token == BooleanToken::And {
                        left.intersection(&right).copied().collect()
                    } else {
                        left.union(&right).copied().collect()
                    };
                    stack.push(result);
                }
                BooleanToken::LeftParenthesis => unreachable!(),
            }
        }

        if stack.len() != 1 {
            bail!("interogarea booleană nu este validă");
        }
        Ok(stack.pop().expect("un singur rezultat"))
    }

    fn page_ids(&self) -> Result<HashSet<i64>> {
        let mut statement = self.connection.prepare("SELECT id FROM pages")?;
        let values = statement
            .query_map([], |row| row.get(0))?
            .collect::<rusqlite::Result<HashSet<_>>>()?;
        Ok(values)
    }

    fn page_ids_for_term(&self, term: &str) -> Result<HashSet<i64>> {
        let mut statement = self
            .connection
            .prepare("SELECT page_id FROM postings WHERE term = ?1")?;
        let values = statement
            .query_map([term], |row| row.get(0))?
            .collect::<rusqlite::Result<HashSet<_>>>()?;
        Ok(values)
    }

    pub fn calculate_pagerank(&mut self, iterations: usize, damping: f64) -> Result<()> {
        let mut statement = self
            .connection
            .prepare("SELECT id, url FROM pages ORDER BY id")?;
        let pages = statement
            .query_map([], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        drop(statement);
        if pages.is_empty() {
            return Ok(());
        }

        let index_by_id: HashMap<i64, usize> = pages
            .iter()
            .enumerate()
            .map(|(index, (id, _))| (*id, index))
            .collect();
        let index_by_url: HashMap<&str, usize> = pages
            .iter()
            .enumerate()
            .map(|(index, (_, url))| (url.as_str(), index))
            .collect();
        let mut outgoing = vec![Vec::new(); pages.len()];

        let mut statement = self
            .connection
            .prepare("SELECT from_page_id, to_url FROM links")?;
        let links = statement
            .query_map([], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        drop(statement);
        for (from_id, to_url) in &links {
            if let (Some(&from), Some(&to)) =
                (index_by_id.get(from_id), index_by_url.get(to_url.as_str()))
            {
                outgoing[from].push(to);
            }
        }

        let count = pages.len();
        let mut ranks = vec![1.0 / count as f64; count];
        for _ in 0..iterations {
            let dangling: f64 = outgoing
                .iter()
                .enumerate()
                .filter(|(_, links)| links.is_empty())
                .map(|(index, _)| ranks[index])
                .sum();
            let mut next =
                vec![(1.0 - damping) / count as f64 + damping * dangling / count as f64; count];
            for (from, links) in outgoing.iter().enumerate() {
                if links.is_empty() {
                    continue;
                }
                let share = damping * ranks[from] / links.len() as f64;
                for &to in links {
                    next[to] += share;
                }
            }
            ranks = next;
        }

        let transaction = self.connection.transaction()?;
        {
            let mut update = transaction.prepare("UPDATE pages SET pagerank = ?1 WHERE id = ?2")?;
            for ((id, _), rank) in pages.iter().zip(ranks) {
                update.execute(params![rank, id])?;
            }
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn stats(&self) -> Result<(i64, i64, i64)> {
        let pages = self
            .connection
            .query_row("SELECT COUNT(*) FROM pages", [], |row| row.get(0))?;
        let terms =
            self.connection
                .query_row("SELECT COUNT(DISTINCT term) FROM postings", [], |row| {
                    row.get(0)
                })?;
        let links = self
            .connection
            .query_row("SELECT COUNT(*) FROM links", [], |row| row.get(0))?;
        Ok((pages, terms, links))
    }
}

fn make_snippet(text: &str, max_chars: usize) -> String {
    let mut snippet = text.chars().take(max_chars).collect::<String>();
    if text.chars().count() > max_chars {
        snippet.push('…');
    }
    snippet
}

fn phrase_occurrences(page_id: i64, maps: &[HashMap<i64, Vec<usize>>]) -> usize {
    let Some(first_positions) = maps.first().and_then(|map| map.get(&page_id)) else {
        return 0;
    };
    first_positions
        .iter()
        .filter(|&&start| {
            maps.iter().enumerate().skip(1).all(|(offset, map)| {
                map.get(&page_id)
                    .is_some_and(|positions| positions.binary_search(&(start + offset)).is_ok())
            })
        })
        .count()
}

fn sequence_occurrences(text: &[String], phrase: &[String]) -> usize {
    if phrase.is_empty() || phrase.len() > text.len() {
        return 0;
    }
    text.windows(phrase.len())
        .filter(|window| *window == phrase)
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn indexes_and_finds_a_page() {
        let file = NamedTempFile::new().unwrap();
        let mut database = Database::open(file.path()).unwrap();
        let page = Page {
            url: "https://example.com/rust".into(),
            title: "Limbajul Rust".into(),
            text: "Rust este rapid și sigur.".into(),
            links: vec![],
            depth: 0,
        };
        database.index_page(&page, "hash-test").unwrap();

        let results = database.search("rust", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Limbajul Rust");
    }

    #[test]
    fn supports_boolean_search() {
        let file = NamedTempFile::new().unwrap();
        let mut database = Database::open(file.path()).unwrap();
        let pages = [
            Page {
                url: "https://example.com/rust".into(),
                title: "Rust".into(),
                text: "rapid sigur".into(),
                links: vec![],
                depth: 0,
            },
            Page {
                url: "https://example.com/python".into(),
                title: "Python".into(),
                text: "rapid simplu".into(),
                links: vec![],
                depth: 0,
            },
        ];
        database.index_page(&pages[0], "hash-rust").unwrap();
        database.index_page(&pages[1], "hash-python").unwrap();

        let results = database
            .search_with_algorithm("rapid AND NOT python", 10, SearchAlgorithm::Boolean)
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Rust");
    }

    #[test]
    fn supports_exact_phrase_search() {
        let file = NamedTempFile::new().unwrap();
        let mut database = Database::open(file.path()).unwrap();
        let page = Page {
            url: "https://example.com/search".into(),
            title: "Căutare".into(),
            text: "Acesta este un motor de căutare clasic.".into(),
            links: vec![],
            depth: 0,
        };
        database.index_page(&page, "hash-phrase").unwrap();

        assert_eq!(
            database
                .search_with_algorithm("motor de căutare", 10, SearchAlgorithm::Phrase)
                .unwrap()
                .len(),
            1
        );
        assert!(
            database
                .search_with_algorithm("motor căutare", 10, SearchAlgorithm::Phrase)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn offers_autocomplete_and_corrections() {
        let file = NamedTempFile::new().unwrap();
        let mut database = Database::open(file.path()).unwrap();
        let page = Page {
            url: "https://example.com/rust".into(),
            title: "Rust".into(),
            text: "motor motoare căutare".into(),
            links: vec![],
            depth: 0,
        };
        database.index_page(&page, "hash-suggest").unwrap();

        assert_eq!(database.autocomplete("mot", 5).unwrap().len(), 2);
        assert_eq!(
            database.corrections("mtoor", 2, 5).unwrap()[0].term,
            "motor"
        );
    }
}
