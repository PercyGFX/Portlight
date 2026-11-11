//! Splits filter text into words, quoted strings, operators and parentheses.

use super::FilterError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Cmp {
    Eq,
    Ne,
    Gt,
    Lt,
    Ge,
    Le,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Tok {
    Word(String),
    Str(String),
    LParen,
    RParen,
    And,
    Or,
    Not,
    Cmp(Cmp),
    Contains,
}

#[derive(Debug, Clone)]
pub(crate) struct Token {
    pub tok: Tok,
    /// Character position in the filter text.
    pub pos: usize,
    /// The text as typed, for messages.
    pub text: String,
}

fn is_word_char(c: char) -> bool {
    !c.is_whitespace() && !matches!(c, '(' | ')' | '"' | '\'' | '!' | '=' | '<' | '>' | '&' | '|')
}

pub(crate) fn lex(input: &str) -> Result<Vec<Token>, FilterError> {
    let chars: Vec<char> = input.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        let start = i;
        let (tok, len) = match c {
            c if c.is_whitespace() => {
                i += 1;
                continue;
            }
            '(' => (Tok::LParen, 1),
            ')' => (Tok::RParen, 1),
            '=' if next == Some('=') => (Tok::Cmp(Cmp::Eq), 2),
            '=' => (Tok::Cmp(Cmp::Eq), 1),
            '!' if next == Some('=') => (Tok::Cmp(Cmp::Ne), 2),
            '!' => (Tok::Not, 1),
            '>' if next == Some('=') => (Tok::Cmp(Cmp::Ge), 2),
            '>' => (Tok::Cmp(Cmp::Gt), 1),
            '<' if next == Some('=') => (Tok::Cmp(Cmp::Le), 2),
            '<' => (Tok::Cmp(Cmp::Lt), 1),
            '&' if next == Some('&') => (Tok::And, 2),
            '|' if next == Some('|') => (Tok::Or, 2),
            '&' => return Err(FilterError::new("Use \"&&\" or \"and\" to combine conditions.", start)),
            '|' => return Err(FilterError::new("Use \"||\" or \"or\" for either condition.", start)),
            '"' | '\'' => {
                let quote = c;
                let mut value = String::new();
                let mut j = i + 1;
                loop {
                    match chars.get(j) {
                        None => return Err(FilterError::new("This quote is never closed.", start)),
                        Some(&ch) if ch == quote => break,
                        Some('\\') if chars.get(j + 1).is_some() => {
                            value.push(chars[j + 1]);
                            j += 2;
                        }
                        Some(&ch) => {
                            value.push(ch);
                            j += 1;
                        }
                    }
                }
                (Tok::Str(value), j + 1 - i)
            }
            _ => {
                let mut j = i;
                while j < chars.len() && is_word_char(chars[j]) {
                    j += 1;
                }
                let word: String = chars[i..j].iter().collect();
                let tok = match word.to_ascii_lowercase().as_str() {
                    "and" => Tok::And,
                    "or" => Tok::Or,
                    "not" => Tok::Not,
                    "eq" => Tok::Cmp(Cmp::Eq),
                    "ne" => Tok::Cmp(Cmp::Ne),
                    "gt" => Tok::Cmp(Cmp::Gt),
                    "lt" => Tok::Cmp(Cmp::Lt),
                    "ge" => Tok::Cmp(Cmp::Ge),
                    "le" => Tok::Cmp(Cmp::Le),
                    "contains" => Tok::Contains,
                    _ => Tok::Word(word),
                };
                (tok, j - i)
            }
        };
        out.push(Token { tok, pos: start, text: chars[start..start + len].iter().collect() });
        i += len;
    }
    Ok(out)
}
