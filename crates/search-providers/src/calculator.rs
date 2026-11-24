//! Safe Arithmetic Expression Evaluator and Calculator Provider
//!
//! Evaluates math expressions without eval, shell execution, or heavy dependencies.
//! Supports operator precedence, parentheses, decimals, unary signs, and bounds checking.

use async_trait::async_trait;
use search_core::{
    LaunchTarget, MatchKind, RankingMetadata, SearchError, SearchProvider, SearchQuery,
    SearchResult, SearchResultCategory,
};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

pub const CALCULATOR_PROVIDER_ID: &str = "calculator";
pub const CALCULATOR_PROVIDER_PRIORITY: u32 = 100;

/// Fast, safe local mathematical expression provider.
#[derive(Default, Clone)]
pub struct CalculatorProvider;

impl CalculatorProvider {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl SearchProvider for CalculatorProvider {
    fn id(&self) -> &'static str {
        CALCULATOR_PROVIDER_ID
    }

    fn display_name(&self) -> &'static str {
        "Calculator"
    }

    fn priority(&self) -> u32 {
        CALCULATOR_PROVIDER_PRIORITY
    }

    fn default_timeout(&self) -> Duration {
        Duration::from_millis(20)
    }

    async fn search(
        &self,
        query: &SearchQuery,
        _cancel_token: &CancellationToken,
    ) -> Result<Vec<SearchResult>, SearchError> {
        let raw = query.raw.trim();
        if raw.is_empty() {
            return Ok(Vec::new());
        }

        // Only evaluate if the input contains mathematical operators or starts with '='
        let has_operator = raw.contains('+')
            || raw.contains('-')
            || raw.contains('*')
            || raw.contains('/')
            || raw.contains('%')
            || raw.contains('^');

        let expr_str = if let Some(stripped) = raw.strip_prefix('=') {
            stripped.trim()
        } else if has_operator {
            raw
        } else {
            return Ok(Vec::new());
        };

        if expr_str.is_empty() {
            return Ok(Vec::new());
        }

        match evaluate_expression(expr_str) {
            Ok(val) => {
                let formatted = format_result(val);
                let title = formatted.clone();
                let subtitle = format!("= {}", expr_str);

                let meta = RankingMetadata {
                    match_kind: MatchKind::Calculated,
                    base_score: 1.0,
                    frecency_boost: 0.0,
                    provider_priority: CALCULATOR_PROVIDER_PRIORITY,
                };

                let item = SearchResult::new(
                    format!("calc:{}", expr_str),
                    title,
                    subtitle,
                    SearchResultCategory::Calculation,
                    LaunchTarget::CopyToClipboard { text: formatted },
                    CALCULATOR_PROVIDER_ID,
                    1.0,
                )
                .with_ranking_meta(meta);

                Ok(vec![item])
            }
            Err(_) => Ok(Vec::new()),
        }
    }
}

/// Token types for mathematical expression parsing.
#[derive(Debug, Clone, PartialEq)]
enum Token {
    Number(f64),
    Plus,
    Minus,
    Multiply,
    Divide,
    Modulo,
    Power,
    LParen,
    RParen,
}

#[derive(Debug, PartialEq)]
pub enum CalcError {
    Empty,
    InvalidToken(char),
    UnexpectedToken,
    MismatchedParentheses,
    DivisionByZero,
    Overflow,
}

/// Evaluates a math expression string into a finite f64 result.
pub fn evaluate_expression(input: &str) -> Result<f64, CalcError> {
    let tokens = tokenize(input)?;
    if tokens.is_empty() {
        return Err(CalcError::Empty);
    }

    let mut pos = 0;
    let val = parse_additive(&tokens, &mut pos)?;

    if pos < tokens.len() {
        return Err(CalcError::UnexpectedToken);
    }

    if !val.is_finite() {
        return Err(CalcError::Overflow);
    }

    Ok(val)
}

fn tokenize(input: &str) -> Result<Vec<Token>, CalcError> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        match chars[i] {
            ' ' | '\t' | '\r' | '\n' => i += 1,
            '+' => {
                tokens.push(Token::Plus);
                i += 1;
            }
            '-' => {
                tokens.push(Token::Minus);
                i += 1;
            }
            '*' => {
                tokens.push(Token::Multiply);
                i += 1;
            }
            '/' => {
                tokens.push(Token::Divide);
                i += 1;
            }
            '%' => {
                tokens.push(Token::Modulo);
                i += 1;
            }
            '^' => {
                tokens.push(Token::Power);
                i += 1;
            }
            '(' => {
                tokens.push(Token::LParen);
                i += 1;
            }
            ')' => {
                tokens.push(Token::RParen);
                i += 1;
            }
            '0'..='9' | '.' => {
                let start = i;
                let mut has_dot = chars[i] == '.';
                i += 1;
                while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                    if chars[i] == '.' {
                        if has_dot {
                            return Err(CalcError::InvalidToken('.'));
                        }
                        has_dot = true;
                    }
                    i += 1;
                }
                let num_str: String = chars[start..i].iter().collect();
                let num: f64 = num_str.parse().map_err(|_| CalcError::UnexpectedToken)?;
                tokens.push(Token::Number(num));
            }
            c => return Err(CalcError::InvalidToken(c)),
        }
    }

    Ok(tokens)
}

fn parse_additive(tokens: &[Token], pos: &mut usize) -> Result<f64, CalcError> {
    let mut left = parse_multiplicative(tokens, pos)?;

    while *pos < tokens.len() {
        match tokens[*pos] {
            Token::Plus => {
                *pos += 1;
                let right = parse_multiplicative(tokens, pos)?;
                left += right;
            }
            Token::Minus => {
                *pos += 1;
                let right = parse_multiplicative(tokens, pos)?;
                left -= right;
            }
            _ => break,
        }
    }

    Ok(left)
}

fn parse_multiplicative(tokens: &[Token], pos: &mut usize) -> Result<f64, CalcError> {
    let mut left = parse_power(tokens, pos)?;

    while *pos < tokens.len() {
        match tokens[*pos] {
            Token::Multiply => {
                *pos += 1;
                let right = parse_power(tokens, pos)?;
                left *= right;
            }
            Token::Divide => {
                *pos += 1;
                let right = parse_power(tokens, pos)?;
                if right == 0.0 {
                    return Err(CalcError::DivisionByZero);
                }
                left /= right;
            }
            Token::Modulo => {
                *pos += 1;
                let right = parse_power(tokens, pos)?;
                if right == 0.0 {
                    return Err(CalcError::DivisionByZero);
                }
                left %= right;
            }
            _ => break,
        }
    }

    Ok(left)
}

fn parse_power(tokens: &[Token], pos: &mut usize) -> Result<f64, CalcError> {
    let base = parse_unary(tokens, pos)?;

    if *pos < tokens.len() && tokens[*pos] == Token::Power {
        *pos += 1;
        // Right associative
        let exp = parse_power(tokens, pos)?;
        let res = base.powf(exp);
        if !res.is_finite() {
            return Err(CalcError::Overflow);
        }
        return Ok(res);
    }

    Ok(base)
}

fn parse_unary(tokens: &[Token], pos: &mut usize) -> Result<f64, CalcError> {
    if *pos >= tokens.len() {
        return Err(CalcError::UnexpectedToken);
    }

    match tokens[*pos] {
        Token::Plus => {
            *pos += 1;
            parse_unary(tokens, pos)
        }
        Token::Minus => {
            *pos += 1;
            let val = parse_unary(tokens, pos)?;
            Ok(-val)
        }
        _ => parse_primary(tokens, pos),
    }
}

fn parse_primary(tokens: &[Token], pos: &mut usize) -> Result<f64, CalcError> {
    if *pos >= tokens.len() {
        return Err(CalcError::UnexpectedToken);
    }

    match &tokens[*pos] {
        Token::Number(n) => {
            let val = *n;
            *pos += 1;
            Ok(val)
        }
        Token::LParen => {
            *pos += 1;
            let val = parse_additive(tokens, pos)?;
            if *pos >= tokens.len() || tokens[*pos] != Token::RParen {
                return Err(CalcError::MismatchedParentheses);
            }
            *pos += 1;
            Ok(val)
        }
        _ => Err(CalcError::UnexpectedToken),
    }
}

fn format_result(val: f64) -> String {
    if val.fract() == 0.0 && val.abs() < 1e15 {
        format!("{:.0}", val)
    } else {
        // Format with up to 8 decimal digits, trim trailing zeros
        let mut s = format!("{:.8}", val);
        while s.contains('.') && (s.ends_with('0') || s.ends_with('.')) {
            if s.ends_with('.') {
                s.pop();
                break;
            }
            s.pop();
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculator_operator_precedence_and_associativity() {
        assert_eq!(evaluate_expression("2 + 3 * 4").unwrap(), 14.0);
        assert_eq!(evaluate_expression("2 * 3 + 4").unwrap(), 10.0);
        assert_eq!(evaluate_expression("2 ^ 3 ^ 2").unwrap(), 512.0); // 2 ^ 9
        assert_eq!(evaluate_expression("10 - 4 - 2").unwrap(), 4.0);
    }

    #[test]
    fn test_calculator_parentheses() {
        assert_eq!(evaluate_expression("(2 + 3) * 4").unwrap(), 20.0);
        assert_eq!(
            evaluate_expression("((10 - 2) * (3 + 1)) / 4").unwrap(),
            8.0
        );
    }

    #[test]
    fn test_calculator_negative_and_decimals() {
        assert_eq!(evaluate_expression("-5 + 3").unwrap(), -2.0);
        assert_eq!(evaluate_expression("3.5 * 2").unwrap(), 7.0);
        assert_eq!(evaluate_expression("-2.5 * -4").unwrap(), 10.0);
        assert_eq!(evaluate_expression("-(3 + 2)").unwrap(), -5.0);
    }

    #[test]
    fn test_calculator_division_by_zero() {
        assert_eq!(
            evaluate_expression("10 / 0"),
            Err(CalcError::DivisionByZero)
        );
        assert_eq!(evaluate_expression("5 % 0"), Err(CalcError::DivisionByZero));
        assert_eq!(
            evaluate_expression("10 / (2 - 2)"),
            Err(CalcError::DivisionByZero)
        );
    }

    #[test]
    fn test_calculator_invalid_syntax_and_mismatched_parens() {
        assert_eq!(
            evaluate_expression("(2 + 3"),
            Err(CalcError::MismatchedParentheses)
        );
        assert_eq!(
            evaluate_expression("2 + +"),
            Err(CalcError::UnexpectedToken)
        );
        assert_eq!(
            evaluate_expression("abc"),
            Err(CalcError::InvalidToken('a'))
        );
        assert_eq!(evaluate_expression(""), Err(CalcError::Empty));
    }

    #[test]
    fn test_calculator_overflow() {
        assert_eq!(evaluate_expression("10 ^ 400"), Err(CalcError::Overflow));
    }

    #[tokio::test]
    async fn test_calculator_provider_search() {
        let provider = CalculatorProvider::new();
        let cancel = CancellationToken::new();

        let q1 = SearchQuery::new("12 * 8", 1);
        let res1 = provider.search(&q1, &cancel).await.unwrap();
        assert_eq!(res1.len(), 1);
        assert_eq!(res1[0].title, "96");
        assert_eq!(res1[0].category, SearchResultCategory::Calculation);

        let q2 = SearchQuery::new("= 100 / 4", 2);
        let res2 = provider.search(&q2, &cancel).await.unwrap();
        assert_eq!(res2.len(), 1);
        assert_eq!(res2[0].title, "25");

        // Non-math query returns empty result
        let q3 = SearchQuery::new("firefox", 3);
        let res3 = provider.search(&q3, &cancel).await.unwrap();
        assert!(res3.is_empty());
    }
}
