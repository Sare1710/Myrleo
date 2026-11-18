use crate::model::WordSuggestion;
use std::collections::BTreeMap;

#[derive(Debug, Default)]
pub struct Trie {
    root: TrieNode,
}

#[derive(Debug, Default)]
struct TrieNode {
    children: BTreeMap<char, TrieNode>,
    terminal: Option<WordSuggestion>,
}

impl Trie {
    pub fn insert(&mut self, term: String, frequency: u64) {
        let mut node = &mut self.root;
        for character in term.chars() {
            node = node.children.entry(character).or_default();
        }
        node.terminal = Some(WordSuggestion { term, frequency });
    }

    pub fn complete(&self, prefix: &str, limit: usize) -> Vec<WordSuggestion> {
        if limit == 0 {
            return Vec::new();
        }

        let mut node = &self.root;
        for character in prefix.chars() {
            let Some(next) = node.children.get(&character) else {
                return Vec::new();
            };
            node = next;
        }

        let mut suggestions = Vec::new();
        collect_terms(node, &mut suggestions);
        suggestions.sort_by(|left, right| {
            right
                .frequency
                .cmp(&left.frequency)
                .then_with(|| left.term.cmp(&right.term))
        });
        suggestions.truncate(limit);
        suggestions
    }
}

fn collect_terms(node: &TrieNode, output: &mut Vec<WordSuggestion>) {
    if let Some(term) = &node.terminal {
        output.push(term.clone());
    }
    for child in node.children.values() {
        collect_terms(child, output);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completes_and_orders_by_frequency() {
        let mut trie = Trie::default();
        trie.insert("motor".into(), 5);
        trie.insert("motoare".into(), 2);
        trie.insert("motocicletă".into(), 8);

        let suggestions = trie.complete("mot", 2);
        assert_eq!(suggestions[0].term, "motocicletă");
        assert_eq!(suggestions[1].term, "motor");
    }
}
