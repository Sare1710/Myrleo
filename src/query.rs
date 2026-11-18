use crate::tokenizer::tokenize;
use anyhow::{Result, bail};
use regex::Regex;
use std::sync::LazyLock;

static QUERY_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\bAND\b|\bOR\b|\bNOT\b|[()]|[\p{L}\p{N}]+").expect("expresie regulată validă")
});

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BooleanToken {
    Term(String),
    And,
    Or,
    Not,
    LeftParenthesis,
}

pub fn parse_boolean_query(query: &str) -> Result<Vec<BooleanToken>> {
    let mut tokens = Vec::new();
    for value in QUERY_PATTERN.find_iter(query).map(|item| item.as_str()) {
        let token = match value.to_ascii_uppercase().as_str() {
            "AND" => BooleanToken::And,
            "OR" => BooleanToken::Or,
            "NOT" => BooleanToken::Not,
            "(" => BooleanToken::LeftParenthesis,
            ")" => {
                tokens.push(RawToken::RightParenthesis);
                continue;
            }
            _ => {
                let Some(term) = tokenize(value).into_iter().next() else {
                    continue;
                };
                BooleanToken::Term(term)
            }
        };
        tokens.push(RawToken::Value(token));
    }

    if tokens.is_empty() {
        bail!("interogarea booleană este goală");
    }

    let tokens = add_implicit_and(tokens);
    to_reverse_polish_notation(tokens)
}

pub fn query_terms(tokens: &[BooleanToken]) -> Vec<String> {
    let mut terms = tokens
        .iter()
        .filter_map(|token| match token {
            BooleanToken::Term(term) => Some(term.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    terms.sort();
    terms.dedup();
    terms
}

#[derive(Debug, Clone)]
enum RawToken {
    Value(BooleanToken),
    RightParenthesis,
}

fn add_implicit_and(tokens: Vec<RawToken>) -> Vec<RawToken> {
    let mut result = Vec::new();
    for token in tokens {
        if result.last().is_some_and(ends_operand) && starts_operand(&token) {
            result.push(RawToken::Value(BooleanToken::And));
        }
        result.push(token);
    }
    result
}

fn ends_operand(token: &RawToken) -> bool {
    matches!(
        token,
        RawToken::Value(BooleanToken::Term(_)) | RawToken::RightParenthesis
    )
}

fn starts_operand(token: &RawToken) -> bool {
    matches!(
        token,
        RawToken::Value(BooleanToken::Term(_))
            | RawToken::Value(BooleanToken::Not)
            | RawToken::Value(BooleanToken::LeftParenthesis)
    )
}

fn to_reverse_polish_notation(tokens: Vec<RawToken>) -> Result<Vec<BooleanToken>> {
    let mut output = Vec::new();
    let mut operators = Vec::new();

    for token in tokens {
        match token {
            RawToken::Value(BooleanToken::Term(term)) => {
                output.push(BooleanToken::Term(term));
            }
            RawToken::Value(BooleanToken::LeftParenthesis) => {
                operators.push(BooleanToken::LeftParenthesis);
            }
            RawToken::RightParenthesis => {
                let mut found_left_parenthesis = false;
                while let Some(operator) = operators.pop() {
                    if operator == BooleanToken::LeftParenthesis {
                        found_left_parenthesis = true;
                        break;
                    }
                    output.push(operator);
                }
                if !found_left_parenthesis {
                    bail!("parantezele interogării nu sunt echilibrate");
                }
            }
            RawToken::Value(operator) => {
                while operators.last().is_some_and(|top| {
                    *top != BooleanToken::LeftParenthesis
                        && (precedence(top) > precedence(&operator)
                            || (precedence(top) == precedence(&operator)
                                && operator != BooleanToken::Not))
                }) {
                    output.push(operators.pop().expect("operator existent"));
                }
                operators.push(operator);
            }
        }
    }

    while let Some(operator) = operators.pop() {
        if operator == BooleanToken::LeftParenthesis {
            bail!("parantezele interogării nu sunt echilibrate");
        }
        output.push(operator);
    }
    Ok(output)
}

fn precedence(token: &BooleanToken) -> u8 {
    match token {
        BooleanToken::Not => 3,
        BooleanToken::And => 2,
        BooleanToken::Or => 1,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uses_boolean_precedence() {
        assert_eq!(
            parse_boolean_query("rust OR python AND rapid").unwrap(),
            [
                BooleanToken::Term("rust".into()),
                BooleanToken::Term("python".into()),
                BooleanToken::Term("rapid".into()),
                BooleanToken::And,
                BooleanToken::Or,
            ]
        );
    }

    #[test]
    fn inserts_implicit_and() {
        assert_eq!(
            parse_boolean_query("rust NOT lent").unwrap(),
            [
                BooleanToken::Term("rust".into()),
                BooleanToken::Term("lent".into()),
                BooleanToken::Not,
                BooleanToken::And,
            ]
        );
    }

    #[test]
    fn rejects_unbalanced_parentheses() {
        assert!(parse_boolean_query("rust AND (sigur OR rapid").is_err());
    }
}
