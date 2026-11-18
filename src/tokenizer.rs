use regex::Regex;
use std::collections::HashMap;
use std::sync::LazyLock;
use unicode_normalization::UnicodeNormalization;

static WORD_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[\p{L}\p{N}]+").expect("expresie regulată validă"));

pub fn tokenize(text: &str) -> Vec<String> {
    let normalized = text.nfkc().collect::<String>().to_lowercase();
    WORD_PATTERN
        .find_iter(&normalized)
        .map(|word| word.as_str().to_owned())
        .collect()
}

pub fn positions(text: &str) -> HashMap<String, Vec<usize>> {
    let mut result = HashMap::new();
    for (position, term) in tokenize(text).into_iter().enumerate() {
        result.entry(term).or_insert_with(Vec::new).push(position);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenizes_unicode_text() {
        assert_eq!(
            tokenize("Rust este RAPID, sigur și clar!"),
            ["rust", "este", "rapid", "sigur", "și", "clar"]
        );
    }

    #[test]
    fn records_term_positions() {
        let index = positions("rust este rapid rust");
        assert_eq!(index["rust"], [0, 3]);
    }
}
