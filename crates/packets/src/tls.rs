//! TLS: the encryption under HTTPS and many other protocols. Only the
//! unencrypted start of a session can be read: hellos, certificates, alerts.

use crate::bytes::{range, slice, u8_at, u16_at, u24_at};
use crate::names;
use crate::packet::Payload;
use crate::summary::{Desc, Who};
use crate::text::{byte_count, hex, lossy, plural};
use crate::{Field, Layer};

const MAX_RECORDS: usize = 16;
const MAX_HANDSHAKES: usize = 8;
const MAX_EXTENSIONS: usize = 64;
const MAX_LIST: usize = 32;

/// True when the bytes start like a TLS record: a known content type and version 3.x.
pub(crate) fn looks_like_record(p: &[u8]) -> bool {
    p.len() >= 3 && (20..=24).contains(&p[0]) && p[1] == 3 && p[2] <= 4
}

#[derive(Debug, Clone)]
pub(crate) struct Tls {
    pub records: Vec<Record>,
    /// The payload does not start with a record: it is the middle of one from an earlier packet.
    pub continuation: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct Record {
    pub at: usize,
    pub ctype: u8,
    pub version: u16,
    pub length: u16,
    pub body_at: usize,
    /// The end of the record's bytes in this packet.
    pub body_end: usize,
    /// The record continues in a later packet.
    pub partial: bool,
    pub content: Content,
}

#[derive(Debug, Clone)]
pub(crate) enum Content {
    Handshakes(Vec<Handshake>),
    /// The handshake is already encrypted (TLS 1.2 Finished and later).
    EncryptedHandshake,
    Alert {
        level: u8,
        desc: u8,
    },
    EncryptedAlert,
    ChangeCipherSpec,
    AppData,
    Heartbeat,
}

#[derive(Debug, Clone)]
pub(crate) struct Handshake {
    pub at: usize,
    pub htype: u8,
    pub length: u32,
    pub end: usize,
    pub partial: bool,
    pub body: HsBody,
}

#[derive(Debug, Clone)]
pub(crate) enum HsBody {
    Hello(Box<Hello>),
    Certificate { count: usize, total: u32 },
    Other,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct Hello {
    pub client: bool,
    pub legacy_version: u16,
    pub session_id: (usize, usize),
    /// Client: the offered list. Server: the one chosen.
    pub ciphers: Vec<(usize, u16)>,
    pub ciphers_at: (usize, usize),
    pub extensions: Vec<Ext>,
    pub sni: Option<(String, usize, usize)>,
    pub alpn: Vec<(String, usize, usize)>,
    /// Client: versions offered. Server: the one chosen.
    pub versions: Vec<(usize, u16)>,
}

impl Hello {
    /// The version really in use (server) or the best offered (client).
    pub(crate) fn version(&self) -> u16 {
        self.versions.iter().map(|&(_, v)| v).filter(|&v| !names::is_grease(v)).max().unwrap_or(self.legacy_version)
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Ext {
    pub at: usize,
    pub etype: u16,
    pub data_at: usize,
    pub data_len: usize,
}

pub(crate) fn parse(frame: &[u8], at: usize, end: usize) -> Tls {
    let mut tls = Tls { records: Vec::new(), continuation: false };
    if !looks_like_record(range(frame, at, end)) {
        tls.continuation = true;
        return tls;
    }
    let mut pos = at;
    while tls.records.len() < MAX_RECORDS {
        let Some(head) = slice(frame, pos, 5).filter(|_| pos + 5 <= end) else { break };
        if !looks_like_record(head) {
            break;
        }
        let version = u16::from_be_bytes([head[1], head[2]]);
        let length = u16::from_be_bytes([head[3], head[4]]);
        let body_at = pos + 5;
        let declared_end = body_at + usize::from(length);
        let body_end = declared_end.min(end);
        let partial = declared_end > end;
        let content = match head[0] {
            20 => Content::ChangeCipherSpec,
            21 if length == 2 => Content::Alert {
                level: u8_at(frame, body_at).unwrap_or(0),
                desc: u8_at(frame, body_at + 1).unwrap_or(0),
            },
            21 => Content::EncryptedAlert,
            22 => handshakes(frame, body_at, body_end, declared_end),
            23 => Content::AppData,
            _ => Content::Heartbeat,
        };
        tls.records.push(Record { at: pos, ctype: head[0], version, length, body_at, body_end, partial, content });
        if partial {
            break;
        }
        pos = declared_end;
    }
    tls
}

fn handshakes(frame: &[u8], at: usize, end: usize, declared_end: usize) -> Content {
    let mut list = Vec::new();
    let mut pos = at;
    while pos + 4 <= end && list.len() < MAX_HANDSHAKES {
        let (Some(htype), Some(length)) = (u8_at(frame, pos), u24_at(frame, pos + 1)) else { break };
        let hs_end = pos + 4 + length as usize;
        if names::tls_handshake(htype).is_none() || hs_end > declared_end {
            if list.is_empty() {
                return Content::EncryptedHandshake;
            }
            break;
        }
        let body_end = hs_end.min(end);
        let body = match htype {
            1 | 2 => HsBody::Hello(Box::new(hello(frame, pos + 4, body_end, htype == 1))),
            11 => certificate(frame, pos + 4, body_end),
            _ => HsBody::Other,
        };
        list.push(Handshake { at: pos, htype, length, end: body_end, partial: hs_end > end, body });
        pos = hs_end;
    }
    if list.is_empty() { Content::EncryptedHandshake } else { Content::Handshakes(list) }
}

fn hello(frame: &[u8], at: usize, end: usize, client: bool) -> Hello {
    let b = &frame[..end.min(frame.len())];
    let mut h = Hello { client, ..Hello::default() };
    let Some(version) = u16_at(b, at) else { return h };
    h.legacy_version = version;
    let mut p = at + 34;
    let Some(sid_len) = u8_at(b, p) else { return h };
    h.session_id = (p, usize::from(sid_len) + 1);
    p += 1 + usize::from(sid_len);
    if client {
        let Some(len) = u16_at(b, p) else { return h };
        let len = usize::from(len);
        h.ciphers_at = (p, len + 2);
        let mut q = p + 2;
        while q + 2 <= (p + 2 + len).min(b.len()) && h.ciphers.len() < 256 {
            h.ciphers.push((q, u16_at(b, q).unwrap_or(0)));
            q += 2;
        }
        p += 2 + len;
        let Some(comp) = u8_at(b, p) else { return h };
        p += 1 + usize::from(comp);
    } else {
        let Some(cipher) = u16_at(b, p) else { return h };
        h.ciphers_at = (p, 2);
        h.ciphers.push((p, cipher));
        p += 3;
    }
    let Some(total) = u16_at(b, p) else { return h };
    let ext_end = (p + 2 + usize::from(total)).min(b.len());
    p += 2;
    while p + 4 <= ext_end && h.extensions.len() < MAX_EXTENSIONS {
        let etype = u16_at(b, p).unwrap_or(0);
        let len = usize::from(u16_at(b, p + 2).unwrap_or(0));
        let data_at = p + 4;
        let data_end = (data_at + len).min(ext_end);
        h.extensions.push(Ext { at: p, etype, data_at, data_len: data_end - data_at });
        match etype {
            0 => h.sni = server_name(b, data_at, data_end),
            16 => h.alpn = alpn(b, data_at, data_end),
            43 if client => {
                let n = usize::from(u8_at(b, data_at).unwrap_or(0));
                let mut q = data_at + 1;
                while q + 2 <= (data_at + 1 + n).min(data_end) && h.versions.len() < MAX_LIST {
                    h.versions.push((q, u16_at(b, q).unwrap_or(0)));
                    q += 2;
                }
            }
            43 => {
                if let Some(v) = u16_at(b, data_at).filter(|_| data_at + 2 <= data_end) {
                    h.versions.push((data_at, v));
                }
            }
            _ => {}
        }
        p = data_at + len;
    }
    h
}

fn server_name(b: &[u8], at: usize, end: usize) -> Option<(String, usize, usize)> {
    let mut q = at + 2;
    while q + 3 <= end {
        let kind = u8_at(b, q)?;
        let len = usize::from(u16_at(b, q + 1)?);
        let name_end = (q + 3 + len).min(end);
        if kind == 0 {
            return Some((lossy(range(b, q + 3, name_end), 255), q + 3, name_end - (q + 3)));
        }
        q += 3 + len;
    }
    None
}

fn alpn(b: &[u8], at: usize, end: usize) -> Vec<(String, usize, usize)> {
    let mut out = Vec::new();
    let mut q = at + 2;
    while q < end && out.len() < MAX_LIST {
        let Some(len) = u8_at(b, q) else { break };
        let id_end = (q + 1 + usize::from(len)).min(end);
        out.push((lossy(range(b, q + 1, id_end), 64), q + 1, id_end - (q + 1)));
        q += 1 + usize::from(len);
    }
    out
}

fn certificate(frame: &[u8], at: usize, end: usize) -> HsBody {
    let b = &frame[..end.min(frame.len())];
    let Some(total) = u24_at(b, at) else { return HsBody::Certificate { count: 0, total: 0 } };
    let mut count = 0;
    let mut q = at + 3;
    while q + 3 <= end && count < 16 {
        let Some(len) = u24_at(b, q) else { break };
        count += 1;
        q += 3 + len as usize;
    }
    HsBody::Certificate { count, total }
}

/// The ClientHello's server name, for the display filter.
pub(crate) fn sni(t: &Tls) -> Option<&str> {
    hellos(t).find(|h| h.client).and_then(|h| h.sni.as_ref()).map(|(name, _, _)| name.as_str())
}

pub(crate) fn hellos(t: &Tls) -> impl Iterator<Item = &Hello> {
    t.records
        .iter()
        .flat_map(|r| match &r.content {
            Content::Handshakes(list) => list.as_slice(),
            _ => &[],
        })
        .filter_map(|h| match &h.body {
            HsBody::Hello(hello) => Some(hello.as_ref()),
            _ => None,
        })
}

pub(crate) fn handshake_types(t: &Tls) -> impl Iterator<Item = u8> + '_ {
    t.records
        .iter()
        .flat_map(|r| match &r.content {
            Content::Handshakes(list) => list.as_slice(),
            _ => &[],
        })
        .map(|h| h.htype)
}

/// "Client Hello (SNI=github.com)", "Application Data"...
fn record_info(r: &Record) -> Vec<String> {
    match &r.content {
        Content::Handshakes(list) => list
            .iter()
            .map(|h| {
                let name = names::tls_handshake(h.htype).unwrap_or("Handshake");
                match &h.body {
                    HsBody::Hello(hello) if hello.client && hello.sni.is_some() => {
                        format!("{name} (SNI={})", hello.sni.as_ref().map_or("", |s| s.0.as_str()))
                    }
                    _ => name.to_string(),
                }
            })
            .collect(),
        Content::EncryptedHandshake => vec!["Encrypted Handshake Message".to_string()],
        Content::Alert { level, desc } => {
            let level = if *level == 2 { "Fatal" } else { "Warning" };
            vec![format!("Alert (Level: {level}, Description: {})", names::tls_alert(*desc))]
        }
        Content::EncryptedAlert => vec!["Encrypted Alert".to_string()],
        Content::ChangeCipherSpec => vec!["Change Cipher Spec".to_string()],
        Content::AppData => vec!["Application Data".to_string()],
        Content::Heartbeat => vec!["Heartbeat".to_string()],
    }
}

pub(crate) fn info(t: &Tls) -> String {
    if t.continuation {
        return "Continuation Data".to_string();
    }
    let mut parts: Vec<String> = Vec::new();
    for r in &t.records {
        for part in record_info(r) {
            if parts.last() != Some(&part) {
                parts.push(part);
            }
        }
    }
    if parts.is_empty() { "Continuation Data".to_string() } else { parts.join(", ") }
}

pub(crate) fn describe(frame: &[u8], p: &Payload, who: &Who) -> Desc {
    let t = parse(frame, p.at, p.end);
    let (src, dst) = (&who.src, &who.dst);
    let encrypted = || format!("{src} sends {} of encrypted data to {dst}", byte_count(p.len()));
    let hello = hellos(&t).next();
    let summary = if let Some(h) = hello {
        let version = names::tls_version(h.version());
        if h.client {
            let server = h.sni.as_ref().map_or(dst.as_str(), |s| s.0.as_str());
            format!("{src} starts a secure ({version}) session with {server}")
        } else {
            format!("{src} agrees to a secure ({version}) session")
        }
    } else if let Some(count) = t.records.iter().find_map(|r| match &r.content {
        Content::Handshakes(list) => list.iter().find_map(|h| match h.body {
            HsBody::Certificate { count, .. } => Some(count),
            _ => None,
        }),
        _ => None,
    }) {
        format!(
            "{src} sends its {} to prove who it is",
            if count <= 1 { "certificate".to_string() } else { plural(count, "certificate", "certificates") }
        )
    } else if let Some((level, desc)) = t.records.iter().find_map(|r| match r.content {
        Content::Alert { level, desc } => Some((level, desc)),
        _ => None,
    }) {
        if desc == 0 {
            format!("{src} closes the secure session")
        } else if level == 2 {
            format!("{src} stops the secure session because of a problem: {}", names::tls_alert(desc))
        } else {
            format!("{src} warns about a problem with the secure session: {}", names::tls_alert(desc))
        }
    } else if t.records.iter().any(|r| matches!(r.content, Content::EncryptedAlert)) {
        format!("{src} sends an encrypted alert (usually: closing the secure session)")
    } else if t.continuation || t.records.iter().any(|r| matches!(r.content, Content::AppData)) {
        encrypted()
    } else if t.records.iter().any(|r| matches!(r.content, Content::EncryptedHandshake | Content::ChangeCipherSpec)) {
        format!("{src} finishes setting up the secure session with {dst}")
    } else if t.records.iter().any(|r| matches!(r.content, Content::Handshakes(_))) {
        format!("{src} continues setting up the secure session with {dst}")
    } else {
        encrypted()
    };
    Desc::new("TLS", info(&t), summary)
}

const EXPLAIN: &str = "TLS encrypts the conversation so nobody in between can read or change it. The start of a \
    session is visible (which site, which version); everything after that is private.";
const PRIVATE: &str =
    "Encrypted data — the contents are private. Only the app and the server have the keys to read it.";

pub(crate) fn layer(frame: &[u8], p: &Payload) -> Layer {
    let t = parse(frame, p.at, p.end);
    let mut layer = Layer::new("tls", "Transport Layer Security (TLS)", info(&t), EXPLAIN, p.at, p.end);
    if t.continuation {
        layer.push(
            Field::new(
                "Continuation",
                format!("{} of a record that started in an earlier packet", byte_count(p.len())),
                p.at,
                p.len(),
            )
            .explain(PRIVATE),
        );
        return layer;
    }
    for r in &t.records {
        layer.push(record_field(frame, r));
    }
    if let Some(last) = t.records.last()
        && !last.partial
    {
        let used = last.body_end;
        if used < p.end {
            layer.push(
                Field::new("More data", byte_count(p.end - used), used, p.end - used)
                    .explain("The start of the next record."),
            );
        }
    }
    layer
}

fn record_field(frame: &[u8], r: &Record) -> Field {
    let title = record_info(r).join(", ");
    let present = r.body_end - r.at;
    let mut field = Field::new("Record", format!("{}: {title}", names::tls_content_type(r.ctype)), r.at, present)
        .child(Field::new("Content type", format!("{} ({})", r.ctype, names::tls_content_type(r.ctype)), r.at, 1))
        .child(Field::new("Version", format!("0x{:04x} ({})", r.version, names::tls_version(r.version)), r.at + 1, 2).explain(
            "An old version number kept for compatibility. The version really used is agreed in the hello messages.",
        ))
        .child(Field::new("Length", byte_count(usize::from(r.length)), r.at + 3, 2));
    let body_len = r.body_end - r.body_at;
    match &r.content {
        Content::Handshakes(list) => {
            for h in list {
                field = field.child(handshake_field(frame, h));
            }
        }
        Content::EncryptedHandshake => {
            field = field.child(
                Field::new("Encrypted handshake message", byte_count(body_len), r.body_at, body_len)
                    .explain("The last step of setting up the session, already encrypted."),
            );
        }
        Content::Alert { level, desc } => {
            let level_name = match level {
                1 => "1 (Warning)",
                2 => "2 (Fatal)",
                _ => "unknown",
            };
            field = field
                .child(Field::new("Level", level_name, r.body_at, 1).explain("Fatal alerts end the session at once."))
                .child(Field::new("Description", format!("{desc} ({})", names::tls_alert(*desc)), r.body_at + 1, 1));
        }
        Content::EncryptedAlert => {
            field = field.child(
                Field::new("Encrypted alert", byte_count(body_len), r.body_at, body_len)
                    .explain("An alert sent after encryption started. Usually it says the session is closing."),
            );
        }
        Content::ChangeCipherSpec => {
            field = field.child(
                Field::note("Meaning", "From now on, this side encrypts what it sends")
                    .explain("Kept in TLS 1.3 only for compatibility with old network equipment."),
            );
        }
        Content::AppData => {
            field = field.child(
                Field::new(
                    "Encrypted data",
                    format!("{} — the contents are private", byte_count(body_len)),
                    r.body_at,
                    body_len,
                )
                .explain(PRIVATE),
            );
        }
        Content::Heartbeat => {
            field = field.child(Field::new("Heartbeat", byte_count(body_len), r.body_at, body_len));
        }
    }
    if r.partial {
        field = field.child(Field::note("Continues", "The rest of this record is in a later packet"));
    }
    field
}

fn handshake_field(frame: &[u8], h: &Handshake) -> Field {
    let name = names::tls_handshake(h.htype).unwrap_or("Handshake");
    let mut field = Field::new("Handshake", name, h.at, h.end - h.at)
        .child(Field::new("Handshake type", format!("{} ({name})", h.htype), h.at, 1))
        .child(Field::new("Length", byte_count(h.length as usize), h.at + 1, 3));
    match &h.body {
        HsBody::Hello(hello) => field = hello_fields(frame, h, hello, field),
        HsBody::Certificate { count, total } => {
            field = field
                .explain(
                    "The server's certificate: an ID card, signed by a trusted authority, proving it is the real site.",
                )
                .child(Field::new(
                    "Certificates",
                    format!("{count} ({} in all)", byte_count(*total as usize)),
                    h.at + 4,
                    h.end.saturating_sub(h.at + 4),
                ));
        }
        HsBody::Other => {}
    }
    if h.partial {
        field = field.child(Field::note("Continues", "The rest of this message is in a later packet"));
    }
    field
}

fn hello_fields(frame: &[u8], h: &Handshake, hello: &Hello, mut field: Field) -> Field {
    let at = h.at + 4;
    field = if hello.client {
        field.explain("The app opens the secure session: it says which site it wants and which encryption it can do.")
    } else {
        field.explain("The server answers: it picks the TLS version and the encryption to use.")
    };
    if hello.legacy_version != 0 {
        field = field.child(
            Field::new(
                "Version",
                format!("0x{:04x} ({})", hello.legacy_version, names::tls_version(hello.legacy_version)),
                at,
                2,
            )
            .explain("Kept at TLS 1.2 for compatibility; TLS 1.3 is agreed in the supported_versions extension."),
        );
    }
    if slice(frame, at + 2, 32).is_some() {
        field = field.child(
            Field::new("Random", hex(range(frame, at + 2, at + 34), 32), at + 2, 32)
                .explain("Random bytes that make every session's keys different."),
        );
    }
    let (sid_at, sid_len) = hello.session_id;
    if sid_len > 0 {
        field = field.child(Field::new("Session ID", byte_count(sid_len - 1), sid_at, sid_len));
    }
    let (cs_at, cs_len) = hello.ciphers_at;
    if hello.client && cs_len > 0 {
        let children = hello
            .ciphers
            .iter()
            .take(MAX_LIST)
            .map(|&(pos, c)| Field::new("Cipher suite", cipher_value(c), pos, 2))
            .collect();
        field = field.child(
            Field::new("Cipher suites", plural(hello.ciphers.len(), "suite", "suites"), cs_at, cs_len)
                .explain("The kinds of encryption the app can use, best first.")
                .children(children),
        );
    } else if let Some(&(pos, c)) = hello.ciphers.first() {
        field = field
            .child(Field::new("Cipher suite", cipher_value(c), pos, 2).explain("The encryption the server chose."));
    }
    if !hello.extensions.is_empty() {
        let start = hello.extensions[0].at;
        let end = hello.extensions.last().map_or(start, |e| e.data_at + e.data_len);
        let children = hello.extensions.iter().map(|e| extension_field(e, hello)).collect();
        field = field.child(
            Field::new("Extensions", hello.extensions.len().to_string(), start, end - start)
                .explain("Extra details and requests added to the hello.")
                .children(children),
        );
    }
    field
}

fn cipher_value(c: u16) -> String {
    match names::tls_cipher(c) {
        Some(name) => format!("{name} (0x{c:04x})"),
        None if names::is_grease(c) => format!("Reserved (GREASE) (0x{c:04x})"),
        None => format!("0x{c:04x}"),
    }
}

fn extension_field(e: &Ext, hello: &Hello) -> Field {
    let name = names::tls_extension(e.etype);
    let len = e.data_len + 4;
    match e.etype {
        0 => {
            let mut f = Field::new(
                "Extension: server_name",
                hello.sni.as_ref().map_or(String::new(), |s| s.0.clone()),
                e.at,
                len,
            );
            if let Some((sni, at, n)) = &hello.sni {
                f = f.child(Field::new("Server name", sni.clone(), *at, *n).explain(
                    "The site the app wants to reach. It is sent before encryption starts, so the network can see it.",
                ));
            }
            f
        }
        16 => {
            let ids: Vec<String> = hello.alpn.iter().map(|(id, _, _)| id.clone()).collect();
            let children = hello
                .alpn
                .iter()
                .map(|(id, at, n)| {
                    let value = names::alpn(id).map_or(id.clone(), |m| format!("{id} ({m})"));
                    Field::new("Protocol", value, *at, *n)
                })
                .collect();
            Field::new("Extension: application_layer_protocol_negotiation", ids.join(", "), e.at, len)
                .explain("Which web protocol to use inside the secure session: h2 = HTTP/2, http/1.1 = HTTP/1.1.")
                .children(children)
        }
        43 => {
            let list: Vec<String> = hello.versions.iter().map(|&(_, v)| names::tls_version(v)).collect();
            let children = hello
                .versions
                .iter()
                .map(|&(at, v)| Field::new("Version", format!("0x{v:04x} ({})", names::tls_version(v)), at, 2))
                .collect();
            let explain = if hello.client {
                "TLS versions the app can use. The server picks one; TLS 1.3 is the newest."
            } else {
                "The TLS version the server picked."
            };
            Field::new("Extension: supported_versions", list.join(", "), e.at, len).explain(explain).children(children)
        }
        0xfe0d => Field::new(&format!("Extension: {name}"), byte_count(e.data_len), e.at, len)
            .explain("Encrypted Client Hello: hides the real site name from the network."),
        _ => Field::new(&format!("Extension: {name}"), byte_count(e.data_len), e.at, len),
    }
}
