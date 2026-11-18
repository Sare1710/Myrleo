use crate::model::Page;
use scraper::{Html, Selector};
use std::collections::HashSet;
use url::Url;

pub fn parse_page(base_url: &Url, html: &str, depth: usize) -> Page {
    let document = Html::parse_document(html);
    let title_selector = Selector::parse("title").expect("selector valid");
    let main_selector = Selector::parse("main").expect("selector valid");
    let article_selector = Selector::parse("article").expect("selector valid");
    let body_selector = Selector::parse("body").expect("selector valid");
    let link_selector = Selector::parse("a[href]").expect("selector valid");

    let title = document
        .select(&title_selector)
        .next()
        .map(|element| clean_text(element.text()))
        .filter(|text| !text.is_empty())
        .unwrap_or_else(|| base_url.as_str().to_owned());

    let text = document
        .select(&main_selector)
        .next()
        .or_else(|| document.select(&article_selector).next())
        .or_else(|| document.select(&body_selector).next())
        .map(|element| clean_text(element.text()))
        .unwrap_or_default();

    let mut unique_links = HashSet::new();
    let mut links = Vec::new();
    for element in document.select(&link_selector) {
        let nofollow = element
            .value()
            .attr("rel")
            .is_some_and(|rel| rel.split_whitespace().any(|value| value == "nofollow"));
        if nofollow {
            continue;
        }

        let Some(href) = element.value().attr("href") else {
            continue;
        };
        let Ok(mut url) = base_url.join(href) else {
            continue;
        };
        if !matches!(url.scheme(), "http" | "https") {
            continue;
        }

        url.set_fragment(None);
        let value = url.to_string();
        if unique_links.insert(value.clone()) {
            links.push(value);
        }
    }

    Page {
        url: base_url.to_string(),
        title,
        text,
        links,
        depth,
    }
}

fn clean_text<'a>(parts: impl Iterator<Item = &'a str>) -> String {
    parts
        .flat_map(str::split_whitespace)
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_title_text_and_absolute_links() {
        let html = r#"
            <html><head><title>Pagina Rust</title></head>
            <body><main><h1>Salut</h1><p>Text de test.</p></main>
            <a href="/doi.html">Doi</a></body></html>
        "#;
        let base = Url::parse("https://example.com/index.html").unwrap();
        let page = parse_page(&base, html, 0);

        assert_eq!(page.title, "Pagina Rust");
        assert_eq!(page.text, "Salut Text de test.");
        assert_eq!(page.links, ["https://example.com/doi.html"]);
    }
}
