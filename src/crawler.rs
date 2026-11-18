use crate::model::{CrawlStats, Page};
use crate::page_parser::parse_page;
use crate::storage::Database;
use anyhow::{Context, Result, bail};
use futures_util::StreamExt;
use reqwest::header::{CONTENT_LENGTH, CONTENT_TYPE};
use robotstxt::DefaultMatcher;
use sha2::{Digest, Sha256};
use std::collections::{HashSet, VecDeque};
use std::time::{Duration, Instant};
use tokio::time::sleep;
use url::Url;

const USER_AGENT_TOKEN: &str = "MyrleoBot";
const USER_AGENT_HEADER: &str =
    "MyrleoBot/0.1 (+https://github.com/Sare1710/Myrleo; educational search crawler)";

pub struct CrawlConfig {
    pub seed_url: Url,
    pub max_pages: usize,
    pub max_depth: usize,
    pub delay: Duration,
    pub max_page_bytes: usize,
}

pub struct Crawler {
    client: reqwest::Client,
    config: CrawlConfig,
}

impl Crawler {
    pub fn new(config: CrawlConfig) -> Result<Self> {
        if !matches!(config.seed_url.scheme(), "http" | "https") {
            bail!("adresa trebuie să înceapă cu http:// sau https://");
        }
        if config.seed_url.host_str().is_none() {
            bail!("adresa nu conține un domeniu valid");
        }

        let client = reqwest::Client::builder()
            .user_agent(USER_AGENT_HEADER)
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .redirect(reqwest::redirect::Policy::limited(5))
            .build()?;
        Ok(Self { client, config })
    }

    pub async fn crawl(&self, database: &mut Database) -> Result<CrawlStats> {
        let robots = self.fetch_robots().await;
        let seed_host = self.config.seed_url.host_str().unwrap().to_owned();
        let mut queue = VecDeque::from([(self.config.seed_url.clone(), 0usize)]);
        let mut seen = HashSet::new();
        let mut stats = CrawlStats::default();
        let mut last_request: Option<Instant> = None;

        while let Some((mut url, depth)) = queue.pop_front() {
            if stats.visited >= self.config.max_pages {
                break;
            }
            url.set_fragment(None);
            let normalized = url.to_string();
            if !seen.insert(normalized) {
                continue;
            }
            stats.visited += 1;

            if depth > self.config.max_depth || url.host_str() != Some(seed_host.as_str()) {
                stats.skipped += 1;
                continue;
            }
            if !is_html_like_url(&url) || !allowed_by_robots(&robots, &url) {
                println!("Sarit: {url}");
                stats.skipped += 1;
                continue;
            }

            wait_for_delay(last_request, self.config.delay).await;
            last_request = Some(Instant::now());
            println!("Descarc [{depth}]: {url}");

            match self.fetch_html(&url).await {
                Ok(Some((final_url, html))) => {
                    let page = parse_page(&final_url, &html, depth);
                    if page.text.is_empty() {
                        stats.skipped += 1;
                        continue;
                    }

                    enqueue_links(
                        &mut queue,
                        &seen,
                        &page,
                        depth,
                        self.config.max_depth,
                        &seed_host,
                    );
                    let content_hash = hash_content(&page.title, &page.text);
                    if database.contains_hash(&content_hash)? {
                        println!("Duplicat: {}", page.url);
                        stats.skipped += 1;
                        continue;
                    }

                    database.index_page(&page, &content_hash)?;
                    stats.indexed += 1;
                }
                Ok(None) => stats.skipped += 1,
                Err(error) => {
                    eprintln!("Eroare la {url}: {error:#}");
                    stats.failed += 1;
                }
            }
        }

        database.calculate_pagerank(30, 0.85)?;
        Ok(stats)
    }

    async fn fetch_robots(&self) -> String {
        let Ok(robots_url) = self.config.seed_url.join("/robots.txt") else {
            return "User-agent: *\nDisallow: /".to_owned();
        };
        match self.client.get(robots_url).send().await {
            Ok(response) if response.status().is_success() => {
                response.text().await.unwrap_or_default()
            }
            Ok(response) if matches!(response.status().as_u16(), 401 | 403) => {
                "User-agent: *\nDisallow: /".to_owned()
            }
            _ => String::new(),
        }
    }

    async fn fetch_html(&self, url: &Url) -> Result<Option<(Url, String)>> {
        let response = self.client.get(url.clone()).send().await?;
        if !response.status().is_success() {
            return Ok(None);
        }

        let content_type = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or("");
        if !content_type.to_ascii_lowercase().starts_with("text/html") {
            return Ok(None);
        }
        if response
            .headers()
            .get(CONTENT_LENGTH)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<usize>().ok())
            .is_some_and(|length| length > self.config.max_page_bytes)
        {
            return Ok(None);
        }

        let final_url = response.url().clone();
        let mut bytes = Vec::new();
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.context("nu pot citi corpul răspunsului")?;
            if bytes.len() + chunk.len() > self.config.max_page_bytes {
                return Ok(None);
            }
            bytes.extend_from_slice(&chunk);
        }
        let html = String::from_utf8_lossy(&bytes).into_owned();
        Ok(Some((final_url, html)))
    }
}

fn allowed_by_robots(robots: &str, url: &Url) -> bool {
    let mut matcher = DefaultMatcher::default();
    matcher.one_agent_allowed_by_robots(robots, USER_AGENT_TOKEN, url.as_str())
}

async fn wait_for_delay(last_request: Option<Instant>, delay: Duration) {
    if let Some(last_request) = last_request {
        let elapsed = last_request.elapsed();
        if elapsed < delay {
            sleep(delay - elapsed).await;
        }
    }
}

fn enqueue_links(
    queue: &mut VecDeque<(Url, usize)>,
    seen: &HashSet<String>,
    page: &Page,
    depth: usize,
    max_depth: usize,
    seed_host: &str,
) {
    if depth >= max_depth {
        return;
    }
    for link in &page.links {
        let Ok(url) = Url::parse(link) else {
            continue;
        };
        if url.host_str() == Some(seed_host) && !seen.contains(url.as_str()) {
            queue.push_back((url, depth + 1));
        }
    }
}

fn is_html_like_url(url: &Url) -> bool {
    let path = url.path().to_ascii_lowercase();
    ![
        ".7z", ".avi", ".css", ".csv", ".doc", ".docx", ".gif", ".gz", ".ico", ".jpeg", ".jpg",
        ".js", ".json", ".mp3", ".mp4", ".pdf", ".png", ".rar", ".svg", ".tar", ".webp", ".xml",
        ".zip",
    ]
    .iter()
    .any(|extension| path.ends_with(extension))
}

fn hash_content(title: &str, text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(title.as_bytes());
    hasher.update([0]);
    hasher.update(text.as_bytes());
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn obeys_robots_rules() {
        let robots = "User-agent: *\nDisallow: /privat\nAllow: /";
        assert!(!allowed_by_robots(
            robots,
            &Url::parse("https://example.com/privat").unwrap()
        ));
        assert!(allowed_by_robots(
            robots,
            &Url::parse("https://example.com/public").unwrap()
        ));
    }

    #[test]
    fn filters_non_html_extensions() {
        assert!(!is_html_like_url(
            &Url::parse("https://example.com/manual.pdf").unwrap()
        ));
        assert!(is_html_like_url(
            &Url::parse("https://example.com/articol").unwrap()
        ));
    }
}
