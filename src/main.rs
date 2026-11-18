use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};
use myrleo::crawler::{CrawlConfig, Crawler};
use myrleo::model::SearchAlgorithm;
use myrleo::storage::Database;
use std::path::PathBuf;
use std::time::Duration;
use url::Url;

#[derive(Parser)]
#[command(name = "myrleo", version, about = "Motor de căutare web clasic")]
struct Cli {
    #[arg(long, global = true, default_value = "myrleo.db")]
    database: PathBuf,

    #[command(subcommand)]
    command: Command,
}

#[derive(Clone, Copy, ValueEnum)]
enum AlgorithmArgument {
    Tf,
    TfIdf,
    Bm25,
    Boolean,
    Phrase,
}

impl From<AlgorithmArgument> for SearchAlgorithm {
    fn from(value: AlgorithmArgument) -> Self {
        match value {
            AlgorithmArgument::Tf => Self::Tf,
            AlgorithmArgument::TfIdf => Self::TfIdf,
            AlgorithmArgument::Bm25 => Self::Bm25,
            AlgorithmArgument::Boolean => Self::Boolean,
            AlgorithmArgument::Phrase => Self::Phrase,
        }
    }
}

#[derive(Subcommand)]
enum Command {
    /// Descarcă și indexează pagini de pe un singur domeniu.
    Crawl {
        url: Url,

        #[arg(long, default_value_t = 50)]
        max_pages: usize,

        #[arg(long, default_value_t = 2)]
        max_depth: usize,

        #[arg(long, default_value_t = 1000)]
        delay_ms: u64,

        #[arg(long, default_value_t = 2_000_000)]
        max_page_bytes: usize,
    },

    /// Caută în paginile deja indexate.
    Search {
        query: String,

        #[arg(long, default_value_t = 10)]
        limit: usize,

        #[arg(long, value_enum, default_value_t = AlgorithmArgument::Bm25)]
        algorithm: AlgorithmArgument,
    },

    /// Afișează dimensiunea indexului.
    Stats,

    /// Sugerează termeni din vocabular după un prefix.
    Suggest {
        prefix: String,

        #[arg(long, default_value_t = 10)]
        limit: usize,
    },

    /// Găsește cuvinte apropiate folosind Levenshtein și BK-tree.
    Correct {
        term: String,

        #[arg(long, default_value_t = 2)]
        max_distance: usize,

        #[arg(long, default_value_t = 5)]
        limit: usize,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let mut database = Database::open(&cli.database)?;

    match cli.command {
        Command::Crawl {
            url,
            max_pages,
            max_depth,
            delay_ms,
            max_page_bytes,
        } => {
            let crawler = Crawler::new(CrawlConfig {
                seed_url: url,
                max_pages,
                max_depth,
                delay: Duration::from_millis(delay_ms),
                max_page_bytes,
            })?;
            let stats = crawler.crawl(&mut database).await?;
            println!(
                "\nGata: {} vizitate, {} indexate, {} sărite, {} erori.",
                stats.visited, stats.indexed, stats.skipped, stats.failed
            );
        }
        Command::Search {
            query,
            limit,
            algorithm,
        } => {
            let results = database.search_with_algorithm(&query, limit, algorithm.into())?;
            if results.is_empty() {
                println!("Nu am găsit rezultate pentru «{query}».");
            }
            for (position, result) in results.iter().enumerate() {
                println!("{}. {}", position + 1, result.title);
                println!("   {}", result.url);
                println!("   {}", result.snippet);
                println!(
                    "   scor {:.3} | termeni: {}\n",
                    result.score,
                    result.matched_terms.join(", ")
                );
            }
        }
        Command::Stats => {
            let (pages, terms, links) = database.stats()?;
            println!("Pagini: {pages}\nTermeni: {terms}\nLinkuri: {links}");
        }
        Command::Suggest { prefix, limit } => {
            for suggestion in database.autocomplete(&prefix, limit)? {
                println!("{} ({})", suggestion.term, suggestion.frequency);
            }
        }
        Command::Correct {
            term,
            max_distance,
            limit,
        } => {
            for correction in database.corrections(&term, max_distance, limit)? {
                println!(
                    "{} (distanță {}, frecvență {})",
                    correction.term, correction.distance, correction.frequency
                );
            }
        }
    }

    Ok(())
}
