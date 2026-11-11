//! The display filter of the capture page's search box: a friendly subset of
//! Wireshark's display filters.
//!
//! - Protocol names alone match packets that carry them: `tcp`, `dns`, `tls`.
//! - Fields compare with `==`, `!=`, `>`, `<`, `>=`, `<=` (or `eq`, `ne`, `gt`,
//!   `lt`, `ge`, `le`) and `contains`: `tcp.port == 443`, `dns.qry.name contains google`.
//!   IP fields take networks: `ip.addr == 192.168.0.0/16`. Yes/no fields work
//!   alone: `tcp.flags.syn`.
//! - `and`, `or`, `not` (or `&&`, `||`, `!`) and parentheses combine conditions;
//!   conditions side by side must all match.
//! - Any other word or quoted text searches the packet row, case-insensitively.
//!
//! A field with several values (both addresses, all question names) matches
//! `==` when any value does, and `!=` when none does. A field the packet does
//! not have matches nothing, `!=` included.

mod eval;
mod fields;
mod lexer;
mod parser;

use std::fmt;

use crate::{Context, LinkType};

/// A parsed display filter, ready to test packets.
#[derive(Debug, Clone)]
pub struct Filter {
    expr: Option<parser::Expr>,
}

/// Why a filter did not parse, in plain words, and where (a character position).
#[derive(Debug, Clone, PartialEq)]
pub struct FilterError {
    pub message: String,
    pub position: usize,
}

impl FilterError {
    pub(crate) fn new(message: impl Into<String>, position: usize) -> Self {
        FilterError { message: message.into(), position }
    }
}

impl fmt::Display for FilterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for FilterError {}

/// What a filter is tested against.
#[derive(Clone, Copy)]
pub struct PacketRef<'a> {
    pub link: LinkType,
    pub frame: &'a [u8],
    pub original_len: u32,
    pub number: u32,
    pub app: Option<&'a str>,
    pub ctx: &'a dyn Context,
}

impl Filter {
    /// Parses filter text. Empty or blank text is the empty filter, which matches everything.
    pub fn parse(text: &str) -> Result<Filter, FilterError> {
        let tokens = lexer::lex(text)?;
        let expr = parser::parse(&tokens, text.chars().count())?;
        Ok(Filter { expr })
    }

    pub fn is_empty(&self) -> bool {
        self.expr.is_none()
    }

    pub fn matches(&self, packet: &PacketRef) -> bool {
        match &self.expr {
            None => true,
            Some(expr) => eval::eval(expr, &eval::Facts::new(packet)),
        }
    }
}
