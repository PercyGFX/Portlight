//! Turns tokens into an expression tree, checking every value as it goes.

use std::net::IpAddr;

use super::FilterError;
use super::fields::{self, FieldId, Kind, Proto};
use super::lexer::{Cmp, Tok, Token};

/// Parentheses and "not" nest at most this deep.
const MAX_DEPTH: usize = 64;

#[derive(Debug, Clone)]
pub(crate) enum Expr {
    /// Every one matches. Kept flat, so long filters do not nest deeply.
    And(Vec<Expr>),
    Or(Vec<Expr>),
    Not(Box<Expr>),
    Test(Test),
}

#[derive(Debug, Clone)]
pub(crate) enum Test {
    Proto(Proto),
    /// The field is present (for yes/no fields: and set).
    Has(FieldId),
    Cmp(FieldId, Cmp, Value),
    /// Case-insensitive substring; the needle is lower case.
    Contains(FieldId, String),
    /// Free-text search across the packet row; lower case.
    Text(String),
}

#[derive(Debug, Clone)]
pub(crate) enum Value {
    Num(u64),
    Net(IpNet),
    Mac([u8; 6]),
    Bool(bool),
    /// Lower case.
    Text(String),
}

/// An address, or a network in CIDR form.
#[derive(Debug, Clone, Copy)]
pub(crate) struct IpNet {
    pub addr: IpAddr,
    pub prefix: u8,
}

impl IpNet {
    pub(crate) fn contains(&self, ip: IpAddr) -> bool {
        match (self.addr, ip) {
            (IpAddr::V4(net), IpAddr::V4(ip)) => {
                let mask = if self.prefix == 0 { 0 } else { u32::MAX << (32 - u32::from(self.prefix)) };
                u32::from(net) & mask == u32::from(ip) & mask
            }
            (IpAddr::V6(net), IpAddr::V6(ip)) => {
                let mask = if self.prefix == 0 { 0 } else { u128::MAX << (128 - u32::from(self.prefix)) };
                u128::from(net) & mask == u128::from(ip) & mask
            }
            _ => false,
        }
    }

    fn parse(text: &str) -> Option<IpNet> {
        let (addr, prefix) = match text.split_once('/') {
            Some((a, p)) => (a, Some(p)),
            None => (text, None),
        };
        let addr: IpAddr = addr.parse().ok()?;
        let max = if addr.is_ipv4() { 32 } else { 128 };
        let prefix = match prefix {
            Some(p) => p.parse::<u8>().ok().filter(|&p| p <= max)?,
            None => max,
        };
        Some(IpNet { addr, prefix })
    }
}

pub(crate) fn parse(tokens: &[Token], input_len: usize) -> Result<Option<Expr>, FilterError> {
    if tokens.is_empty() {
        return Ok(None);
    }
    let mut p = Parser { toks: tokens, i: 0, end: input_len };
    let expr = p.or(0)?;
    if let Some(t) = p.peek() {
        return Err(match t.tok {
            Tok::RParen => FilterError::new("There is a \")\" without a matching \"(\".", t.pos),
            _ => FilterError::new(format!("I did not understand \"{}\" here.", t.text), t.pos),
        });
    }
    Ok(Some(expr))
}

struct Parser<'a> {
    toks: &'a [Token],
    i: usize,
    end: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<&Token> {
        self.toks.get(self.i)
    }

    fn bump(&mut self) -> Option<Token> {
        let t = self.toks.get(self.i).cloned();
        self.i += 1;
        t
    }

    /// Where an error about "nothing after this token" points.
    fn after(&self, t: &Token) -> usize {
        self.peek().map_or(self.end, |_| t.pos + t.text.chars().count())
    }

    fn or(&mut self, depth: usize) -> Result<Expr, FilterError> {
        let mut terms = vec![self.and(depth)?];
        while let Some(t) = self.peek().filter(|t| t.tok == Tok::Or).cloned() {
            self.bump();
            self.expect_operand(&t)?;
            terms.push(self.and(depth)?);
        }
        Ok(if terms.len() == 1 { terms.remove(0) } else { Expr::Or(terms) })
    }

    fn and(&mut self, depth: usize) -> Result<Expr, FilterError> {
        let mut terms = vec![self.not(depth)?];
        while let Some(t) = self.peek().cloned() {
            match t.tok {
                Tok::And => {
                    self.bump();
                    self.expect_operand(&t)?;
                }
                // Two conditions side by side mean both, like a search box.
                Tok::Word(_) | Tok::Str(_) | Tok::LParen | Tok::Not => {}
                _ => break,
            }
            terms.push(self.not(depth)?);
        }
        Ok(if terms.len() == 1 { terms.remove(0) } else { Expr::And(terms) })
    }

    fn expect_operand(&self, op: &Token) -> Result<(), FilterError> {
        match self.peek() {
            None => Err(FilterError::new(format!("Expected something after \"{}\".", op.text), self.end)),
            Some(t) if matches!(t.tok, Tok::RParen | Tok::And | Tok::Or) => {
                Err(FilterError::new(format!("Expected something after \"{}\".", op.text), t.pos))
            }
            Some(_) => Ok(()),
        }
    }

    fn not(&mut self, depth: usize) -> Result<Expr, FilterError> {
        if depth > MAX_DEPTH {
            return Err(FilterError::new("This filter is nested too deeply.", self.peek().map_or(self.end, |t| t.pos)));
        }
        if let Some(t) = self.peek().filter(|t| t.tok == Tok::Not).cloned() {
            self.bump();
            self.expect_operand(&t)?;
            return Ok(Expr::Not(Box::new(self.not(depth + 1)?)));
        }
        self.primary(depth)
    }

    fn primary(&mut self, depth: usize) -> Result<Expr, FilterError> {
        let Some(t) = self.bump() else {
            return Err(FilterError::new("Expected a condition.", self.end));
        };
        match t.tok {
            Tok::LParen => {
                if matches!(self.peek().map(|t| &t.tok), None | Some(Tok::RParen)) {
                    if self.peek().is_none() {
                        return Err(FilterError::new("Unclosed parenthesis.", t.pos));
                    }
                    return Err(FilterError::new("There is nothing inside these parentheses.", t.pos));
                }
                let inner = self.or(depth + 1)?;
                match self.bump() {
                    Some(close) if close.tok == Tok::RParen => Ok(inner),
                    _ => Err(FilterError::new("Unclosed parenthesis.", t.pos)),
                }
            }
            Tok::RParen => Err(FilterError::new("There is a \")\" without a matching \"(\".", t.pos)),
            Tok::Cmp(_) | Tok::Contains => {
                Err(FilterError::new(format!("Expected a field name before \"{}\", like tcp.port.", t.text), t.pos))
            }
            Tok::And | Tok::Or => {
                Err(FilterError::new(format!("\"{}\" needs a condition on both sides.", t.text), t.pos))
            }
            Tok::Not => Err(FilterError::new("Expected a condition.", t.pos)),
            Tok::Str(ref s) => Ok(Expr::Test(Test::Text(s.to_lowercase()))),
            Tok::Word(ref w) => self.word(&t, w),
        }
    }

    fn word(&mut self, t: &Token, w: &str) -> Result<Expr, FilterError> {
        let next = self.peek().map(|n| n.tok.clone());
        let compared = matches!(next, Some(Tok::Cmp(_) | Tok::Contains));
        if let Some((id, kind, name)) = fields::field(w) {
            return self.field_test(t, id, kind, name, next);
        }
        if fields::protocol(w).is_some() && compared {
            let mut msg = format!("\"{w}\" is a protocol, not a field, so it cannot be compared.");
            if let Some(first) = fields::fields_of(w).first() {
                msg.push_str(&format!(" Did you mean {first}?"));
            }
            return Err(FilterError::new(msg, t.pos));
        }
        if let Some(p) = fields::protocol(w) {
            return Ok(Expr::Test(Test::Proto(p)));
        }
        if compared || looks_like_field(w) {
            let mut msg = format!("\"{w}\" is not a field I know.");
            let prefix = w.split('.').next().unwrap_or("");
            if let Some(s) = fields::suggest(w) {
                msg.push_str(&format!(" Did you mean {s}?"));
            } else if !fields::fields_of(prefix).is_empty() {
                let list = fields::fields_of(prefix);
                let shown: Vec<&str> = list.iter().take(4).copied().collect();
                msg.push_str(&format!(" Fields for {} include {}.", prefix.to_ascii_lowercase(), shown.join(", ")));
            }
            if !compared {
                msg.push_str(&format!(" To search for this text, put it in quotes: \"{w}\"."));
            }
            return Err(FilterError::new(msg, t.pos));
        }
        Ok(Expr::Test(Test::Text(w.to_lowercase())))
    }

    fn field_test(
        &mut self,
        t: &Token,
        id: FieldId,
        kind: Kind,
        name: &str,
        next: Option<Tok>,
    ) -> Result<Expr, FilterError> {
        match next {
            Some(Tok::Cmp(cmp)) => {
                let op = self.bump().unwrap_or_else(|| t.clone());
                let value = self.value(&op)?;
                if !matches!(cmp, Cmp::Eq | Cmp::Ne) && kind != Kind::Num {
                    return Err(FilterError::new(
                        format!("\"{}\" only compares numbers, and {name} is not a number.", op.text),
                        op.pos,
                    ));
                }
                Ok(Expr::Test(Test::Cmp(id, cmp, parse_value(kind, name, &value)?)))
            }
            Some(Tok::Contains) => {
                let op = self.bump().unwrap_or_else(|| t.clone());
                let value = self.value(&op)?;
                if matches!(kind, Kind::Num | Kind::Bool) {
                    return Err(FilterError::new(
                        format!("\"contains\" works with text, like dns.qry.name or app, not with {name}."),
                        op.pos,
                    ));
                }
                Ok(Expr::Test(Test::Contains(id, value_text(&value).to_lowercase())))
            }
            // "port 443", "host github.com", "app chrome": a shorthand for "==".
            Some(Tok::Word(ref w)) if shorthand(id) && fields::field(w).is_none() && fields::protocol(w).is_none() => {
                let value = self.bump().unwrap_or_else(|| t.clone());
                Ok(Expr::Test(Test::Cmp(id, Cmp::Eq, parse_value(kind, name, &value)?)))
            }
            Some(Tok::Str(_)) if shorthand(id) => {
                let value = self.bump().unwrap_or_else(|| t.clone());
                Ok(Expr::Test(Test::Cmp(id, Cmp::Eq, parse_value(kind, name, &value)?)))
            }
            _ => Ok(Expr::Test(Test::Has(id))),
        }
    }

    fn value(&mut self, op: &Token) -> Result<Token, FilterError> {
        match self.peek() {
            Some(v) if matches!(v.tok, Tok::Word(_) | Tok::Str(_)) => Ok(self.bump().unwrap_or_else(|| op.clone())),
            _ => Err(FilterError::new(format!("Expected a value after \"{}\".", op.text), self.after(op))),
        }
    }
}

fn shorthand(id: FieldId) -> bool {
    matches!(id, FieldId::Port | FieldId::Host | FieldId::App)
}

/// A dotted word that looks like an attempt at a field name rather than a site name.
fn looks_like_field(w: &str) -> bool {
    let Some((prefix, _)) = w.split_once('.') else { return false };
    fields::protocol(prefix).is_some() || fields::suggest(w).is_some()
}

fn value_text(t: &Token) -> &str {
    match &t.tok {
        Tok::Word(w) | Tok::Str(w) => w,
        _ => "",
    }
}

fn parse_value(kind: Kind, field: &str, t: &Token) -> Result<Value, FilterError> {
    let text = value_text(t);
    let err = |msg: String| Err(FilterError::new(msg, t.pos));
    match kind {
        Kind::Num => match parse_number(text) {
            Some(n) => Ok(Value::Num(n)),
            None => err(format!("\"{text}\" is not a number. {field} needs a number, like 443 or 0x1bb.")),
        },
        Kind::Ip => match IpNet::parse(text) {
            Some(net) => Ok(Value::Net(net)),
            None if text.chars().any(|c| c.is_ascii_alphabetic()) && !text.contains(':') => {
                err(format!("\"{text}\" is not an IP address. To match a name, try host == {text}."))
            }
            None => err(format!("\"{text}\" is not an IP address or network (like 192.168.1.1 or 10.0.0.0/8).")),
        },
        Kind::Mac => match parse_mac(text) {
            Some(m) => Ok(Value::Mac(m)),
            None => err(format!("\"{text}\" is not a MAC address (like aa:bb:cc:dd:ee:ff).")),
        },
        Kind::Bool => match text.to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => Ok(Value::Bool(true)),
            "0" | "false" | "no" | "off" => Ok(Value::Bool(false)),
            _ => err(format!("\"{text}\" is not a yes/no value. Use 1 or 0.")),
        },
        Kind::Text => Ok(Value::Text(text.to_lowercase())),
        Kind::Host => Ok(match IpNet::parse(text) {
            Some(net) => Value::Net(net),
            None => Value::Text(text.trim_end_matches('.').to_lowercase()),
        }),
    }
}

fn parse_number(text: &str) -> Option<u64> {
    match text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
        Some(hex) => u64::from_str_radix(hex, 16).ok(),
        None => text.parse().ok(),
    }
}

fn parse_mac(text: &str) -> Option<[u8; 6]> {
    let parts: Vec<&str> = text.split([':', '-']).collect();
    if parts.len() != 6 {
        return None;
    }
    let mut out = [0u8; 6];
    for (slot, part) in out.iter_mut().zip(&parts) {
        if part.is_empty() || part.len() > 2 {
            return None;
        }
        *slot = u8::from_str_radix(part, 16).ok()?;
    }
    Some(out)
}
