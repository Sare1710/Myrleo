use crate::model::{SearchAlgorithm, SearchResult, WordSuggestion};
use crate::storage::Database;
use crate::tokenizer::tokenize;
use anyhow::{Context, Result};
use axum::extract::{Query, State};
use axum::http::{StatusCode, header};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;
use std::fmt::Write;
use std::path::PathBuf;
use std::sync::Arc;

const STYLE: &str = include_str!("../assets/style.css");
const SCRIPT: &str = include_str!("../assets/app.js");
const EMBLEM: &[u8] = include_bytes!("../assets/myrleo-emblem.webp");

#[derive(Clone)]
struct WebState {
    database_path: PathBuf,
}

#[derive(Debug)]
struct WebError(anyhow::Error);

impl IntoResponse for WebError {
    fn into_response(self) -> Response {
        eprintln!("Eroare web: {:#}", self.0);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Html(layout(
                "Eroare",
                "<main class=\"message-page\"><h1>A apărut o eroare</h1><p>Încearcă din nou.</p><a href=\"/\">Înapoi la căutare</a></main>",
            )),
        )
            .into_response()
    }
}

impl From<anyhow::Error> for WebError {
    fn from(error: anyhow::Error) -> Self {
        Self(error)
    }
}

#[derive(Debug, Deserialize)]
struct SearchParameters {
    #[serde(default)]
    q: String,
    #[serde(default = "default_algorithm")]
    algorithm: String,
}

#[derive(Debug, Deserialize)]
struct SuggestParameters {
    #[serde(default)]
    q: String,
    #[serde(default = "default_suggestion_limit")]
    limit: usize,
}

struct SearchPageData {
    results: Vec<SearchResult>,
    correction: Option<String>,
}

pub async fn serve(database_path: PathBuf, host: String, port: u16) -> Result<()> {
    let state = Arc::new(WebState { database_path });
    let application = Router::new()
        .route("/", get(home))
        .route("/search", get(search))
        .route("/api/suggest", get(suggest))
        .route("/health", get(|| async { "ok" }))
        .route("/style.css", get(style))
        .route("/app.js", get(script))
        .route("/myrleo-emblem.webp", get(emblem))
        .with_state(state);

    let address = format!("{host}:{port}");
    let listener = tokio::net::TcpListener::bind(&address)
        .await
        .with_context(|| format!("nu pot porni serverul pe {address}"))?;
    println!("Myrleo rulează la http://{address}");
    println!("Oprește serverul cu Ctrl+C.");

    axum::serve(listener, application)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
        .context("serverul web s-a oprit cu o eroare")?;
    Ok(())
}

async fn home(State(state): State<Arc<WebState>>) -> Result<Html<String>, WebError> {
    let path = state.database_path.clone();
    let (pages, terms, links) = run_database(path, |database| database.stats()).await?;
    let content = format!(
        r#"<main class="hero">
            <a class="brand brand-large" href="/" aria-label="Myrleo pagina principală">
              <span class="brand-mark"><img src="/myrleo-emblem.webp" alt=""></span><span>Myrleo</span>
            </a>
            <p class="tagline">Căutare clasică. Fără reclame. Fără AI.</p>
            {}
            <div class="index-stats" aria-label="Statistici index">
              <span><strong>{pages}</strong> pagini</span>
              <span><strong>{terms}</strong> termeni</span>
              <span><strong>{links}</strong> linkuri</span>
            </div>
          </main>"#,
        search_form("", "bm25", true)
    );
    Ok(Html(layout("Myrleo", &content)))
}

async fn search(
    State(state): State<Arc<WebState>>,
    Query(parameters): Query<SearchParameters>,
) -> Result<Html<String>, WebError> {
    let query = parameters.q.trim().to_owned();
    let (algorithm, algorithm_name) = parse_algorithm(&parameters.algorithm);
    if query.is_empty() {
        return home(State(state)).await;
    }

    let path = state.database_path.clone();
    let query_for_database = query.clone();
    let data = run_database(path, move |database| {
        let results = database.search_with_algorithm(&query_for_database, 20, algorithm)?;
        let correction = if results.is_empty()
            && !matches!(
                algorithm,
                SearchAlgorithm::Boolean | SearchAlgorithm::Phrase
            ) {
            corrected_query(&database, &query_for_database)?
        } else {
            None
        };
        Ok(SearchPageData {
            results,
            correction,
        })
    })
    .await?;

    let mut content = String::new();
    write!(
        content,
        r#"<header class="search-header">
             <a class="brand" href="/"><span class="brand-mark"><img src="/myrleo-emblem.webp" alt=""></span><span>Myrleo</span></a>
             {}
           </header>
           <main class="results-page">"#,
        search_form(&query, algorithm_name, false)
    )
    .expect("scriere în String");

    write!(
        content,
        "<p class=\"result-count\">{} rezultat(e) pentru <strong>„{}”</strong></p>",
        data.results.len(),
        escape_html(&query)
    )
    .expect("scriere în String");

    if let Some(correction) = &data.correction {
        write!(
            content,
            "<p class=\"correction\">Ai vrut să cauți <a href=\"{}\">{}</a>?</p>",
            search_url(correction, algorithm_name),
            escape_html(correction)
        )
        .expect("scriere în String");
    }

    if data.results.is_empty() {
        content.push_str(
            "<section class=\"empty-state\"><h1>Niciun rezultat</h1><p>Încearcă mai puține cuvinte sau verifică ortografia.</p></section>",
        );
    } else {
        content.push_str("<ol class=\"results-list\">");
        for result in data.results {
            write_result(&mut content, &result);
        }
        content.push_str("</ol>");
    }
    content.push_str("</main>");
    Ok(Html(layout(&format!("{query} — Myrleo"), &content)))
}

async fn suggest(
    State(state): State<Arc<WebState>>,
    Query(parameters): Query<SuggestParameters>,
) -> Result<Json<Vec<WordSuggestion>>, WebError> {
    let path = state.database_path.clone();
    let query = parameters.q;
    let limit = parameters.limit.clamp(1, 20);
    let suggestions =
        run_database(path, move |database| database.autocomplete(&query, limit)).await?;
    Ok(Json(suggestions))
}

async fn style() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "text/css; charset=utf-8")], STYLE)
}

async fn script() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
        SCRIPT,
    )
}

async fn emblem() -> impl IntoResponse {
    (
        [
            (header::CONTENT_TYPE, "image/webp"),
            (header::CACHE_CONTROL, "public, max-age=86400"),
        ],
        EMBLEM,
    )
}

async fn run_database<T, F>(path: PathBuf, operation: F) -> Result<T, WebError>
where
    T: Send + 'static,
    F: FnOnce(Database) -> Result<T> + Send + 'static,
{
    tokio::task::spawn_blocking(move || {
        let database = Database::open(&path)?;
        operation(database)
    })
    .await
    .map_err(|error| WebError(anyhow::Error::new(error)))?
    .map_err(WebError)
}

fn corrected_query(database: &Database, query: &str) -> Result<Option<String>> {
    let terms = tokenize(query);
    if terms.is_empty() || terms.len() > 6 {
        return Ok(None);
    }

    let mut changed = false;
    let mut corrected = Vec::new();
    for term in terms {
        let correction = database
            .corrections(&term, 2, 5)?
            .into_iter()
            .find(|candidate| candidate.distance > 0);
        if let Some(correction) = correction {
            changed = true;
            corrected.push(correction.term);
        } else {
            corrected.push(term);
        }
    }
    Ok(changed.then(|| corrected.join(" ")))
}

fn write_result(output: &mut String, result: &SearchResult) {
    let terms = result
        .matched_terms
        .iter()
        .map(|term| escape_html(term))
        .collect::<Vec<_>>()
        .join(", ");
    write!(
        output,
        r#"<li class="result-card">
              <a class="result-url" href="{}" rel="noopener noreferrer">{}</a>
              <h2><a href="{}" rel="noopener noreferrer">{}</a></h2>
              <p>{}</p>
              <div class="result-meta">Scor {:.3} · {}</div>
            </li>"#,
        escape_html(&result.url),
        escape_html(&result.url),
        escape_html(&result.url),
        escape_html(&result.title),
        escape_html(&result.snippet),
        result.score,
        terms
    )
    .expect("scriere în String");
}

fn search_form(query: &str, algorithm: &str, large: bool) -> String {
    let class = if large {
        "search-form search-form-large"
    } else {
        "search-form"
    };
    format!(
        r#"<form class="{class}" action="/search" method="get" role="search" autocomplete="off">
             <div class="search-box">
               <input name="q" value="{}" placeholder="Caută în Myrleo" aria-label="Termeni de căutare" required>
               <button type="submit">Caută</button>
             </div>
             <div class="suggestions" role="listbox" hidden></div>
             <label class="algorithm-label">Algoritm
               <select name="algorithm">
                 {}
               </select>
             </label>
           </form>"#,
        escape_html(query),
        algorithm_options(algorithm)
    )
}

fn algorithm_options(selected: &str) -> String {
    [
        ("bm25", "BM25 + PageRank"),
        ("tf-idf", "TF-IDF"),
        ("tf", "TF"),
        ("boolean", "Boolean"),
        ("phrase", "Expresie exactă"),
    ]
    .into_iter()
    .map(|(value, label)| {
        let selected_attribute = if value == selected { " selected" } else { "" };
        format!("<option value=\"{value}\"{selected_attribute}>{label}</option>")
    })
    .collect()
}

fn parse_algorithm(value: &str) -> (SearchAlgorithm, &'static str) {
    match value {
        "tf" => (SearchAlgorithm::Tf, "tf"),
        "tf-idf" | "tfidf" => (SearchAlgorithm::TfIdf, "tf-idf"),
        "boolean" => (SearchAlgorithm::Boolean, "boolean"),
        "phrase" => (SearchAlgorithm::Phrase, "phrase"),
        _ => (SearchAlgorithm::Bm25, "bm25"),
    }
}

fn search_url(query: &str, algorithm: &str) -> String {
    let encoded = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("q", query)
        .append_pair("algorithm", algorithm)
        .finish();
    format!("/search?{encoded}")
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn layout(title: &str, content: &str) -> String {
    format!(
        r#"<!doctype html>
<html lang="ro">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <meta name="description" content="Myrleo — motor de căutare clasic, fără reclame și fără AI">
  <title>{}</title>
  <link rel="stylesheet" href="/style.css">
  <script src="/app.js" defer></script>
</head>
<body>
  {}
  <footer><span>Myrleo</span><span>Fără reclame · Fără AI</span></footer>
</body>
</html>"#,
        escape_html(title),
        content
    )
}

fn default_algorithm() -> String {
    "bm25".to_owned()
}

const fn default_suggestion_limit() -> usize {
    8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_untrusted_html() {
        assert_eq!(
            escape_html("<script>'x' & \"y\"</script>"),
            "&lt;script&gt;&#39;x&#39; &amp; &quot;y&quot;&lt;/script&gt;"
        );
    }

    #[test]
    fn builds_encoded_search_urls() {
        assert_eq!(
            search_url("motor de căutare", "bm25"),
            "/search?q=motor+de+c%C4%83utare&algorithm=bm25"
        );
    }
}
