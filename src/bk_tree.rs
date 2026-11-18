use crate::model::Correction;
use std::collections::HashMap;

#[derive(Debug, Default)]
pub struct BkTree {
    root: Option<Box<BkNode>>,
}

#[derive(Debug)]
struct BkNode {
    term: String,
    frequency: u64,
    children: HashMap<usize, Box<BkNode>>,
}

impl BkTree {
    pub fn insert(&mut self, term: String, frequency: u64) {
        match &mut self.root {
            Some(root) => root.insert(term, frequency),
            None => {
                self.root = Some(Box::new(BkNode {
                    term,
                    frequency,
                    children: HashMap::new(),
                }));
            }
        }
    }

    pub fn search(&self, term: &str, max_distance: usize, limit: usize) -> Vec<Correction> {
        let mut results = Vec::new();
        if let Some(root) = &self.root {
            root.search(term, max_distance, &mut results);
        }
        results.sort_by(|left, right| {
            left.distance
                .cmp(&right.distance)
                .then_with(|| right.frequency.cmp(&left.frequency))
                .then_with(|| left.term.cmp(&right.term))
        });
        results.truncate(limit);
        results
    }
}

impl BkNode {
    fn insert(&mut self, term: String, frequency: u64) {
        let distance = levenshtein(&self.term, &term);
        if distance == 0 {
            self.frequency = self.frequency.max(frequency);
            return;
        }
        match self.children.get_mut(&distance) {
            Some(child) => child.insert(term, frequency),
            None => {
                self.children.insert(
                    distance,
                    Box::new(Self {
                        term,
                        frequency,
                        children: HashMap::new(),
                    }),
                );
            }
        }
    }

    fn search(&self, term: &str, max_distance: usize, output: &mut Vec<Correction>) {
        let distance = levenshtein(&self.term, term);
        if distance <= max_distance {
            output.push(Correction {
                term: self.term.clone(),
                distance,
                frequency: self.frequency,
            });
        }

        let minimum = distance.saturating_sub(max_distance);
        let maximum = distance.saturating_add(max_distance);
        for child_distance in minimum..=maximum {
            if let Some(child) = self.children.get(&child_distance) {
                child.search(term, max_distance, output);
            }
        }
    }
}

pub fn levenshtein(left: &str, right: &str) -> usize {
    let right = right.chars().collect::<Vec<_>>();
    let mut previous = (0..=right.len()).collect::<Vec<_>>();
    let mut current = vec![0; right.len() + 1];

    for (left_index, left_character) in left.chars().enumerate() {
        current[0] = left_index + 1;
        for (right_index, right_character) in right.iter().enumerate() {
            let substitution_cost = usize::from(left_character != *right_character);
            current[right_index + 1] = (current[right_index] + 1)
                .min(previous[right_index + 1] + 1)
                .min(previous[right_index] + substitution_cost);
        }
        std::mem::swap(&mut previous, &mut current);
    }
    previous[right.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calculates_unicode_distance() {
        assert_eq!(levenshtein("căutare", "cautare"), 1);
        assert_eq!(levenshtein("rust", "rust"), 0);
    }

    #[test]
    fn finds_nearby_terms() {
        let mut tree = BkTree::default();
        tree.insert("python".into(), 10);
        tree.insert("piston".into(), 2);
        tree.insert("rust".into(), 7);

        let results = tree.search("pyhton", 2, 5);
        assert_eq!(results[0].term, "python");
        assert_eq!(results[0].distance, 2);
    }
}
