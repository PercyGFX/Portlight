//! HTTP/1.x: plain, unencrypted web requests and responses.

use crate::bytes::range;
use crate::packet::Payload;
use crate::summary::{Desc, Who};
use crate::text::{byte_count, lossy, shorten};
use crate::{Field, Layer};

const METHODS: [&str; 9] = ["GET", "POST", "PUT", "DELETE", "HEAD", "OPTIONS", "PATCH", "CONNECT", "TRACE"];
const MAX_LINES: usize = 64;

/// True when the bytes start like an HTTP/1 request or response.
pub(crate) fn looks_like_start(p: &[u8]) -> bool {
    p.starts_with(b"HTTP/1.")
        || METHODS.iter().any(|m| p.len() > m.len() && p.starts_with(m.as_bytes()) && p[m.len()] == b' ')
}

#[derive(Debug, Clone)]
pub(crate) enum Start {
    Request {
        method: String,
        uri: String,
    },
    Response {
        code: u16,
        reason: String,
    },
    /// The middle of a message that started in an earlier packet.
    Continuation,
}

#[derive(Debug, Clone)]
pub(crate) struct Line {
    pub at: usize,
    /// Without the line ending.
    pub len: usize,
    pub text: String,
}

#[derive(Debug, Clone)]
pub(crate) struct Http {
    pub start: Start,
    pub first: Option<Line>,
    pub headers: Vec<Line>,
    /// Where the body starts, when the blank line after the headers is in this packet.
    pub body_at: Option<usize>,
}

impl Http {
    pub(crate) fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find_map(|l| {
            let (n, v) = l.text.split_once(':')?;
            n.trim().eq_ignore_ascii_case(name).then(|| v.trim())
        })
    }
}

/// Splits lines ending in "\n" (and an optional "\r"); the last one may be cut short.
pub(crate) fn lines(frame: &[u8], at: usize, end: usize, max: usize) -> (Vec<Line>, Option<usize>) {
    let data = range(frame, at, end);
    let mut out = Vec::new();
    let mut pos = 0;
    while pos < data.len() && out.len() < max {
        let (line_end, next) = match data[pos..].iter().position(|&c| c == b'\n') {
            Some(i) => (pos + i, pos + i + 1),
            None => (data.len(), data.len()),
        };
        let trimmed = if line_end > pos && data[line_end - 1] == b'\r' { line_end - 1 } else { line_end };
        if trimmed == pos && next > line_end {
            return (out, Some(at + next));
        }
        out.push(Line { at: at + pos, len: trimmed - pos, text: lossy(&data[pos..trimmed], 2000) });
        pos = next;
    }
    (out, None)
}

pub(crate) fn parse(frame: &[u8], at: usize, end: usize) -> Http {
    if !looks_like_start(range(frame, at, end)) {
        return Http { start: Start::Continuation, first: None, headers: Vec::new(), body_at: None };
    }
    let (mut lines, body_at) = lines(frame, at, end, MAX_LINES);
    let first = if lines.is_empty() { None } else { Some(lines.remove(0)) };
    let text = first.as_ref().map_or("", |l| l.text.as_str());
    let mut parts = text.splitn(3, ' ');
    let (a, b, c) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""), parts.next().unwrap_or(""));
    let start = if a.starts_with("HTTP/") {
        Start::Response { code: b.parse().unwrap_or(0), reason: c.to_string() }
    } else {
        Start::Request { method: a.to_string(), uri: b.to_string() }
    };
    Http { start, first, headers: lines, body_at }
}

pub(crate) fn info(h: &Http) -> String {
    match &h.start {
        Start::Continuation => "Continuation".to_string(),
        _ => h.first.as_ref().map_or(String::new(), |l| shorten(&l.text, 200)),
    }
}

pub(crate) fn describe(frame: &[u8], p: &Payload, who: &Who) -> Desc {
    let h = parse(frame, p.at, p.end);
    let (src, dst) = (&who.src, &who.dst);
    let host = h.header("host").map(|s| s.to_string()).unwrap_or_else(|| dst.clone());
    let summary = match &h.start {
        Start::Request { method, uri, .. } => {
            let uri = shorten(uri, 60);
            match method.as_str() {
                "GET" | "HEAD" => format!("{src} asks {host} for {uri}"),
                "POST" | "PUT" | "PATCH" => format!("{src} sends data to {host} ({uri})"),
                "DELETE" => format!("{src} asks {host} to delete {uri}"),
                m => format!("{src} sends an HTTP {m} request to {host}"),
            }
        }
        Start::Response { code, reason, .. } => {
            let reason = if reason.is_empty() { String::new() } else { format!(" {reason}") };
            match (code / 100, h.header("location")) {
                (3, Some(loc)) => format!("{src} answers: {code}{reason} (go to {})", shorten(loc, 60)),
                _ => format!("{src} answers: {code}{reason}"),
            }
        }
        Start::Continuation => format!("{src} sends {} more of web data to {dst}", byte_count(p.len())),
    };
    Desc::new("HTTP", info(&h), summary)
}

fn header_explain(name: &str) -> Option<&'static str> {
    Some(match name.to_ascii_lowercase().as_str() {
        "host" => "The website the request is for. One server can host many sites, so the name is sent along.",
        "user-agent" => "Which app or browser is asking, as it describes itself.",
        "content-type" => "What kind of data the body is, e.g. a web page, an image or JSON.",
        "content-length" => "The size of the body in bytes.",
        "cookie" => {
            "Data the site saved in this app earlier, sent back to it. It is visible here because plain HTTP is not encrypted."
        }
        "set-cookie" => "Data the site asks the app to save and send back next time.",
        "location" => "Where to go instead.",
        "server" => "The web server software, as it describes itself.",
        "authorization" => "Login details for the site. Over plain HTTP anyone on the way can read them.",
        _ => return None,
    })
}

pub(crate) fn layer(frame: &[u8], p: &Payload) -> Layer {
    let h = parse(frame, p.at, p.end);
    let mut layer = Layer::new(
        "http",
        "Hypertext Transfer Protocol (HTTP)",
        info(&h),
        "HTTP is how browsers and apps ask web servers for pages and data. This is plain, unencrypted HTTP, \
         so anyone on the network path could read it.",
        p.at,
        p.end,
    );
    if let Start::Continuation = h.start {
        layer.push(
            Field::new("Continuation", byte_count(p.len()), p.at, p.len())
                .explain("The body of a message that started in an earlier packet."),
        );
        return layer;
    }
    if let Some(first) = &h.first {
        let mut field = Field::new(
            if matches!(h.start, Start::Request { .. }) { "Request line" } else { "Status line" },
            shorten(&first.text, 200),
            first.at,
            first.len,
        );
        let mut offset = first.at;
        let words: Vec<&str> = first.text.splitn(3, ' ').collect();
        let labels: &[(&str, &str)] = match &h.start {
            Start::Request { .. } => &[
                ("Method", "What the app wants: GET = fetch, POST = send data."),
                ("Address", "Which page or file on the site."),
                ("Version", ""),
            ],
            _ => &[
                ("Version", ""),
                (
                    "Status code",
                    "The server's answer: 2xx = OK, 3xx = look elsewhere, 4xx = the request was wrong or not allowed, 5xx = the server had a problem.",
                ),
                ("Reason", ""),
            ],
        };
        for (word, (label, explain)) in words.iter().zip(labels) {
            let mut child = Field::new(label, shorten(word, 200), offset, word.len());
            if !explain.is_empty() {
                child = child.explain(*explain);
            }
            field = field.child(child);
            offset += word.len() + 1;
        }
        layer.push(field);
    }
    for line in &h.headers {
        let (name, value) = line.text.split_once(':').unwrap_or((line.text.as_str(), ""));
        let mut field = Field::new(name.trim(), shorten(value.trim(), 200), line.at, line.len);
        if let Some(e) = header_explain(name.trim()) {
            field = field.explain(e);
        }
        layer.push(field);
    }
    if let Some(body) = h.body_at
        && body < p.end
    {
        layer.push(Field::new("Body", byte_count(p.end - body), body, p.end - body));
    }
    layer
}
