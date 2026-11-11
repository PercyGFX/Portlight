//! DNS, multicast DNS and LLMNR: turning names into addresses.

use std::fmt::Write;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use crate::bytes::{ipv4_at, ipv6_at, u8_at, u16_at, u32_at};
use crate::names;
use crate::packet::{DnsKind, Payload};
use crate::summary::{Desc, Who};
use crate::text::{byte_count, duration, lossy, shorten};
use crate::{Field, Layer};

const MAX_QUESTIONS: usize = 16;
const MAX_RECORDS: usize = 64;
/// A name is at most 255 bytes on the wire.
const MAX_NAME: usize = 255;
/// Compression pointers followed per name; real names need two or three.
const MAX_JUMPS: usize = 16;

#[derive(Debug, Clone)]
pub(crate) struct Name {
    pub text: String,
    /// Absolute offset and length of the name's bytes at this place in the packet.
    pub at: usize,
    pub len: usize,
}

#[derive(Debug, Clone)]
pub(crate) struct Question {
    pub name: Name,
    pub qtype: u16,
    pub class: u16,
    pub at: usize,
    pub len: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Section {
    Answer,
    Authority,
    Additional,
}

#[derive(Debug, Clone)]
pub(crate) enum RData {
    A(Ipv4Addr),
    Aaaa(Ipv6Addr),
    /// CNAME, PTR, NS.
    Name(Name),
    Mx(u16, Name),
    Txt(String),
    Srv {
        priority: u16,
        weight: u16,
        port: u16,
        target: Name,
    },
    Svcb {
        priority: u16,
        target: Name,
    },
    Soa(Name),
    Other,
}

#[derive(Debug, Clone)]
pub(crate) struct Record {
    pub section: Section,
    pub name: Name,
    pub rtype: u16,
    pub class: u16,
    pub ttl: u32,
    pub at: usize,
    pub len: usize,
    pub data_at: usize,
    pub data_len: usize,
    pub data: RData,
}

#[derive(Debug, Clone)]
pub(crate) struct Message {
    /// Absolute offset of the message (after the 2-byte length over TCP).
    pub base: usize,
    pub end: usize,
    /// The TCP length prefix: where it is and what it says.
    pub tcp_len: Option<(usize, u16)>,
    pub id: u16,
    pub flags: u16,
    pub counts: [u16; 4],
    pub questions: Vec<Question>,
    pub records: Vec<Record>,
    /// False when the message was cut short or damaged before the last record.
    pub complete: bool,
}

impl Message {
    pub(crate) fn is_response(&self) -> bool {
        self.flags & 0x8000 != 0
    }

    pub(crate) fn rcode(&self) -> u16 {
        self.flags & 0x000f
    }

    pub(crate) fn opcode(&self) -> u16 {
        (self.flags >> 11) & 0x0f
    }

    pub(crate) fn answers(&self) -> impl Iterator<Item = &Record> {
        self.records.iter().filter(|r| r.section == Section::Answer)
    }

    /// Addresses in the answer section.
    pub(crate) fn addresses(&self) -> impl Iterator<Item = IpAddr> + '_ {
        self.answers().filter_map(|r| match r.data {
            RData::A(a) => Some(IpAddr::V4(a)),
            RData::Aaaa(a) => Some(IpAddr::V6(a)),
            _ => None,
        })
    }
}

/// Reads the DNS message in a TCP or UDP payload.
pub(crate) fn parse_payload(frame: &[u8], p: &Payload) -> Option<Message> {
    if p.tcp {
        let len = u16_at(frame, p.at)?;
        let base = p.at + 2;
        let end = (base + usize::from(len)).min(p.end);
        let mut msg = parse(frame, base, end)?;
        msg.tcp_len = Some((p.at, len));
        Some(msg)
    } else {
        parse(frame, p.at, p.end)
    }
}

/// Reads a DNS message lying from `base` to `end`.
pub(crate) fn parse(frame: &[u8], base: usize, end: usize) -> Option<Message> {
    let msg = frame.get(base..end)?;
    let id = u16_at(msg, 0)?;
    let flags = u16_at(msg, 2)?;
    let counts = [u16_at(msg, 4)?, u16_at(msg, 6)?, u16_at(msg, 8)?, u16_at(msg, 10)?];
    let mut m = Message {
        base,
        end,
        tcp_len: None,
        id,
        flags,
        counts,
        questions: Vec::new(),
        records: Vec::new(),
        complete: true,
    };
    let mut pos = 12;
    for _ in 0..usize::from(counts[0]).min(MAX_QUESTIONS) {
        let Some((name, used)) = read_name(msg, pos) else {
            m.complete = false;
            return Some(m);
        };
        let (Some(qtype), Some(class)) = (u16_at(msg, pos + used), u16_at(msg, pos + used + 2)) else {
            m.complete = false;
            return Some(m);
        };
        m.questions.push(Question {
            name: Name { text: name, at: base + pos, len: used },
            qtype,
            class,
            at: base + pos,
            len: used + 4,
        });
        pos += used + 4;
    }
    let sections = [(Section::Answer, counts[1]), (Section::Authority, counts[2]), (Section::Additional, counts[3])];
    for (section, count) in sections {
        for _ in 0..count {
            if m.records.len() == MAX_RECORDS {
                return Some(m);
            }
            match read_record(msg, base, pos, section) {
                Some((record, next)) => {
                    m.records.push(record);
                    pos = next;
                }
                None => {
                    m.complete = false;
                    return Some(m);
                }
            }
        }
    }
    Some(m)
}

fn read_record(msg: &[u8], base: usize, pos: usize, section: Section) -> Option<(Record, usize)> {
    let (text, used) = read_name(msg, pos)?;
    let p = pos + used;
    let rtype = u16_at(msg, p)?;
    let class = u16_at(msg, p + 2)?;
    let ttl = u32_at(msg, p + 4)?;
    let data_len = usize::from(u16_at(msg, p + 8)?);
    let data_at = p + 10;
    if data_at + data_len > msg.len() {
        return None;
    }
    let name_at = |at: usize| -> Option<Name> {
        let (text, len) = read_name(msg, at)?;
        Some(Name { text, at: base + at, len })
    };
    let data = match (rtype, data_len) {
        (1, 4) => ipv4_at(msg, data_at).map_or(RData::Other, RData::A),
        (28, 16) => ipv6_at(msg, data_at).map_or(RData::Other, RData::Aaaa),
        (2 | 5 | 12, _) => name_at(data_at).map_or(RData::Other, RData::Name),
        (15, _) => match (u16_at(msg, data_at), name_at(data_at + 2)) {
            (Some(pref), Some(n)) => RData::Mx(pref, n),
            _ => RData::Other,
        },
        (16, _) => RData::Txt(txt(msg, data_at, data_at + data_len)),
        (33, _) => {
            match (u16_at(msg, data_at), u16_at(msg, data_at + 2), u16_at(msg, data_at + 4), name_at(data_at + 6)) {
                (Some(priority), Some(weight), Some(port), Some(target)) => {
                    RData::Srv { priority, weight, port, target }
                }
                _ => RData::Other,
            }
        }
        (64 | 65, _) => match (u16_at(msg, data_at), name_at(data_at + 2)) {
            (Some(priority), Some(target)) => RData::Svcb { priority, target },
            _ => RData::Other,
        },
        (6, _) => name_at(data_at).map_or(RData::Other, RData::Soa),
        _ => RData::Other,
    };
    let record = Record {
        section,
        name: Name { text, at: base + pos, len: used },
        rtype,
        class,
        ttl,
        at: base + pos,
        len: data_at + data_len - pos,
        data_at: base + data_at,
        data_len,
        data,
    };
    Some((record, data_at + data_len))
}

/// Reads a possibly compressed name at `at`. Returns the name and how many
/// bytes it takes at that position. Loops and overlong names give `None`.
pub(crate) fn read_name(msg: &[u8], at: usize) -> Option<(String, usize)> {
    let mut out = String::new();
    let mut pos = at;
    let mut used = None;
    let mut jumps = 0;
    let mut wire_len = 0;
    loop {
        let len = u8_at(msg, pos)?;
        match len & 0xc0 {
            0x00 if len == 0 => {
                used.get_or_insert_with(|| pos + 1 - at);
                break;
            }
            0x00 => {
                let label = msg.get(pos + 1..pos + 1 + usize::from(len))?;
                wire_len += label.len() + 1;
                if wire_len > MAX_NAME {
                    return None;
                }
                if !out.is_empty() {
                    out.push('.');
                }
                out.push_str(&lossy(label, 63));
                pos += 1 + label.len();
            }
            0xc0 => {
                let low = u8_at(msg, pos + 1)?;
                used.get_or_insert_with(|| pos + 2 - at);
                jumps += 1;
                if jumps > MAX_JUMPS {
                    return None;
                }
                pos = usize::from(len & 0x3f) << 8 | usize::from(low);
            }
            _ => return None,
        }
    }
    if out.is_empty() {
        out.push_str("<Root>");
    }
    Some((out, used.unwrap_or(1)))
}

fn txt(msg: &[u8], mut pos: usize, end: usize) -> String {
    let mut parts = Vec::new();
    while pos < end && parts.len() < 8 {
        let Some(len) = u8_at(msg, pos) else { break };
        let s = crate::bytes::range(msg, pos + 1, (pos + 1 + usize::from(len)).min(end));
        parts.push(lossy(s, 100));
        pos += 1 + usize::from(len);
    }
    shorten(&parts.join(" "), 120)
}

/// The value of a record, short: "140.82.112.3", "github.map.fastly.net".
fn data_text(r: &Record) -> String {
    match &r.data {
        RData::A(a) => a.to_string(),
        RData::Aaaa(a) => a.to_string(),
        RData::Name(n) | RData::Soa(n) => n.text.clone(),
        RData::Mx(pref, n) => format!("{pref} {}", n.text),
        RData::Txt(t) => format!("\"{t}\""),
        RData::Srv { priority, weight, port, target } => format!("{priority} {weight} {port} {}", target.text),
        RData::Svcb { priority, target } => format!("{priority} {}", target.text),
        RData::Other if r.rtype == 41 => String::new(),
        RData::Other => byte_count(r.data_len),
    }
}

/// "Standard query 0x1a2b A github.com".
pub(crate) fn info(m: &Message) -> String {
    let mut s = names::dns_opcode(m.opcode()).to_string();
    if m.is_response() {
        s.push_str(" response");
    }
    let _ = write!(s, " 0x{:04x}", m.id);
    if m.is_response() && m.rcode() != 0 {
        let _ = write!(s, " {}", names::dns_rcode(m.rcode()));
    }
    for q in m.questions.iter().take(4) {
        let _ = write!(s, " {} {}", names::dns_type_label(q.qtype), q.name.text);
    }
    if m.is_response() {
        for r in m.answers().take(8) {
            let _ = write!(s, " {} {}", names::dns_type_label(r.rtype), data_text(r));
        }
    }
    s.truncate(s.trim_end().len());
    s
}

/// "3.112.82.140.in-addr.arpa" → 140.82.112.3.
pub(crate) fn reverse_ip(name: &str) -> Option<IpAddr> {
    let lower = name.to_ascii_lowercase();
    let lower = lower.trim_end_matches('.');
    if let Some(rest) = lower.strip_suffix(".in-addr.arpa") {
        let parts: Vec<u8> = rest.split('.').map(|p| p.parse().ok()).collect::<Option<_>>()?;
        if parts.len() != 4 {
            return None;
        }
        return Some(IpAddr::V4(Ipv4Addr::new(parts[3], parts[2], parts[1], parts[0])));
    }
    let rest = lower.strip_suffix(".ip6.arpa")?;
    let nibbles: Vec<u8> =
        rest.split('.').map(|p| u8::from_str_radix(p, 16).ok().filter(|_| p.len() == 1)).collect::<Option<_>>()?;
    if nibbles.len() != 32 {
        return None;
    }
    let mut bytes = [0u8; 16];
    for (i, pair) in nibbles.rchunks(2).enumerate() {
        // Reversed order: the last two nibbles are the first byte.
        bytes[i] = (pair[1] << 4) | pair[0];
    }
    Some(IpAddr::V6(Ipv6Addr::from(bytes)))
}

/// What a question asks for: "the address of github.com".
fn asking_for(qtype: u16, name: &str) -> String {
    match qtype {
        1 | 28 => format!("the address of {name}"),
        64 | 65 => format!("the connection settings of {name}"),
        12 => match reverse_ip(name) {
            Some(ip) => format!("the name of {ip}"),
            None => format!("the services called {name}"),
        },
        5 => format!("the real name of {name}"),
        15 => format!("the mail servers of {name}"),
        16 => format!("the text records of {name}"),
        33 => format!("where to find the service {name}"),
        255 => format!("everything about {name}"),
        t => format!("the {} records of {name}", names::dns_type_label(t)),
    }
}

/// "github.com is at 140.82.112.3", or `None` when there is nothing to say.
fn answer_phrase(m: &Message, asked: Option<&Question>) -> Option<String> {
    let name = asked.map(|q| q.name.text.as_str()).or_else(|| m.answers().next().map(|r| r.name.text.as_str()))?;
    let addrs: Vec<IpAddr> = m.addresses().collect();
    if !addrs.is_empty() {
        let list = match addrs.len() {
            1 => addrs[0].to_string(),
            2 => format!("{} and {}", addrs[0], addrs[1]),
            n => format!("{} (and {} more)", addrs[0], n - 1),
        };
        return Some(format!("{name} is at {list}"));
    }
    let first = m.answers().next()?;
    Some(match &first.data {
        RData::Name(n) if first.rtype == 5 => format!("{name} is another name for {}", n.text),
        RData::Name(n) if first.rtype == 12 => match reverse_ip(&first.name.text) {
            Some(ip) => format!("{ip} is called {}", n.text),
            None => format!("{} is {}", first.name.text, n.text),
        },
        _ if matches!(first.rtype, 64 | 65) => format!("here are the connection settings for {name}"),
        _ => format!("{name} has {} {} records", m.answers().count(), names::dns_type_label(first.rtype)),
    })
}

pub(crate) fn describe(frame: &[u8], p: &Payload, kind: DnsKind, who: &Who) -> Option<Desc> {
    let m = parse_payload(frame, p)?;
    let protocol = match kind {
        DnsKind::Dns => "DNS",
        DnsKind::Mdns => "MDNS",
        DnsKind::Llmnr => "LLMNR",
    };
    let local = kind != DnsKind::Dns;
    let (src, dst) = (&who.src, &who.dst);
    let q = m.questions.first();
    let summary = if !m.is_response() {
        let what = match q {
            Some(q) => asking_for(q.qtype, &q.name.text),
            None => "a name".to_string(),
        };
        if local {
            format!("{src} asks devices on the local network for {what}")
        } else if dst == "This PC" {
            format!("{src} asks This PC for {what}")
        } else {
            format!("{src} asks the DNS server for {what}")
        }
    } else {
        let speaker = if local {
            format!("{src} answers on the local network")
        } else if src == "This PC" {
            format!("This PC answers {dst}")
        } else {
            "The DNS server answers".to_string()
        };
        let name = q.map(|q| q.name.text.clone()).unwrap_or_else(|| "the name".to_string());
        match m.rcode() {
            0 => match answer_phrase(&m, q) {
                Some(phrase) => format!("{speaker}: {phrase}"),
                None => match q.map(|q| q.qtype) {
                    Some(1) => format!("{speaker}: {name} has no IPv4 address"),
                    Some(28) => format!("{speaker}: {name} has no IPv6 address"),
                    Some(t) => format!("{speaker}: {name} has no {} records", names::dns_type_label(t)),
                    None => format!("{speaker} with no records"),
                },
            },
            3 => format!("{speaker}: {name} does not exist"),
            code => format!(
                "{speaker} that it could not answer the question about {name} ({})",
                names::dns_rcode(code).to_lowercase()
            ),
        }
    };
    Some(Desc::new(protocol, info(&m), summary))
}

const DNS_EXPLAIN: &str = "DNS turns names like github.com into the IP addresses computers use. An app asks the \
    question, a DNS server (usually your router or your internet provider) answers.";
const MDNS_EXPLAIN: &str = "Multicast DNS lets devices on the local network find each other by name (like \
    printer.local) without a DNS server: the question goes to everyone, and the device with that name answers.";
const LLMNR_EXPLAIN: &str = "LLMNR is an older Windows way of finding computers on the local network by name: the \
    question goes to everyone, and the computer with that name answers.";

pub(crate) fn layer(frame: &[u8], p: &Payload, kind: DnsKind) -> Option<Layer> {
    let m = parse_payload(frame, p)?;
    let (title, explain) = match kind {
        DnsKind::Dns => ("Domain Name System (DNS)", DNS_EXPLAIN),
        DnsKind::Mdns => ("Multicast DNS (mDNS)", MDNS_EXPLAIN),
        DnsKind::Llmnr => ("Link-Local Multicast Name Resolution (LLMNR)", LLMNR_EXPLAIN),
    };
    let first =
        m.questions.first().map(|q| q.name.text.clone()).or_else(|| m.records.first().map(|r| r.name.text.clone()));
    let summary = match (m.is_response(), first) {
        (false, Some(n)) => format!("Question about {n}"),
        (true, Some(n)) => {
            format!("Answer about {n}: {}", crate::text::plural(usize::from(m.counts[1]), "record", "records"))
        }
        (_, None) => format!("Message 0x{:04x}", m.id),
    };
    let mut layer = Layer::new("dns", title, summary, explain, p.at, m.end);
    let b = m.base;
    if let Some((at, len)) = m.tcp_len {
        layer.push(
            Field::new("Length", byte_count(usize::from(len)), at, 2)
                .explain("Over TCP, each DNS message starts with its size."),
        );
    }
    layer.push(
        Field::new("Transaction ID", format!("0x{:04x}", m.id), b, 2)
            .explain("A number the asker picks; the answer carries the same one, so the two can be matched."),
    );
    layer.push(flags_field(&m));
    let labels = ["Questions", "Answer records", "Authority records", "Additional records"];
    for (i, label) in labels.iter().enumerate() {
        layer.push(Field::new(label, m.counts[i].to_string(), b + 4 + i * 2, 2));
    }
    if !m.questions.is_empty() {
        let start = m.questions[0].at;
        let end = m.questions.last().map_or(start, |q| q.at + q.len);
        let children = m.questions.iter().map(|q| question_field(q, kind)).collect();
        layer.push(Field::new("Queries", m.questions.len().to_string(), start, end - start).children(children));
    }
    for (section, label) in [
        (Section::Answer, "Answers"),
        (Section::Authority, "Authoritative name servers"),
        (Section::Additional, "Additional records"),
    ] {
        let records: Vec<&Record> = m.records.iter().filter(|r| r.section == section).collect();
        if let (Some(first), Some(last)) = (records.first(), records.last()) {
            let start = first.at;
            let end = last.at + last.len;
            let children = records.iter().map(|r| record_field(r, kind)).collect();
            let mut field = Field::new(label, records.len().to_string(), start, end - start).children(children);
            if section == Section::Answer {
                field = field.explain("The answers to the question.");
            }
            layer.push(field);
        }
    }
    if !m.complete {
        layer.push(Field::note("Cut short", "The rest of this message is missing or damaged"));
    }
    Some(layer)
}

fn flags_field(m: &Message) -> Field {
    let at = m.base + 2;
    let f = m.flags;
    let bit = |mask: u16| f & mask != 0;
    let mut summary = format!("{}{}", names::dns_opcode(m.opcode()), if m.is_response() { " response" } else { "" });
    if m.is_response() {
        let _ = write!(summary, ", {}", names::dns_rcode(m.rcode()));
    }
    let yes_no = |b: bool| if b { "Yes" } else { "No" };
    let mut field = Field::new("Flags", format!("0x{f:04x} ({summary})"), at, 2)
        .child(
            Field::new("Response", if m.is_response() { "This is an answer" } else { "This is a question" }, at, 1)
                .explain("Questions and answers look alike; this bit tells them apart."),
        )
        .child(Field::new("Opcode", format!("{} ({})", m.opcode(), names::dns_opcode(m.opcode())), at, 1));
    if m.is_response() {
        field = field.child(
            Field::new("Authoritative", yes_no(bit(0x0400)), at, 1)
                .explain("Yes when the answer comes from the server in charge of the name, not from a cache."),
        );
    }
    field = field
        .child(
            Field::new("Truncated", yes_no(bit(0x0200)), at, 1)
                .explain("The answer was too big and was cut; the asker should try again over TCP."),
        )
        .child(
            Field::new("Recursion desired", yes_no(bit(0x0100)), at, 1)
                .explain("The asker wants the server to find the answer for it, asking other servers if needed."),
        );
    if m.is_response() {
        field = field.child(Field::new("Recursion available", yes_no(bit(0x0080)), at + 1, 1)).child(
            Field::new("Reply code", format!("{} ({})", m.rcode(), names::dns_rcode(m.rcode())), at + 1, 1)
                .explain("Whether the lookup worked. \"No such name\" means the name does not exist."),
        );
    }
    field
}

fn class_text(class: u16, kind: DnsKind, question: bool) -> String {
    let base = match class & 0x7fff {
        1 => "IN (Internet)".to_string(),
        255 => "ANY".to_string(),
        c => c.to_string(),
    };
    if kind == DnsKind::Mdns && class & 0x8000 != 0 {
        let note = if question { "answer directly to the asker" } else { "replace older answers" };
        return format!("{base}, {note}");
    }
    base
}

fn name_field(name: &Name) -> Field {
    Field::new("Name", name.text.clone(), name.at, name.len).explain("The name being looked up.")
}

fn question_field(q: &Question, kind: DnsKind) -> Field {
    let tail = q.at + q.name.len;
    Field::new("Question", format!("{}: type {}", q.name.text, names::dns_type_label(q.qtype)), q.at, q.len)
        .child(name_field(&q.name))
        .child(Field::new("Type", names::dns_type_value(q.qtype), tail, 2).explain(
            "What kind of answer is wanted: A = IPv4 address, AAAA = IPv6 address, HTTPS = connection settings.",
        ))
        .child(Field::new("Class", class_text(q.class, kind, true), tail + 2, 2))
}

fn record_field(r: &Record, kind: DnsKind) -> Field {
    let tail = r.at + r.name.len;
    if r.rtype == 41 {
        return Field::new("OPT", format!("extended DNS: replies up to {} bytes", r.class), r.at, r.len)
            .explain("Not an answer: settings that let DNS messages be bigger and carry extra options.");
    }
    let value = data_text(r);
    let mut field = Field::new(
        "Record",
        format!(
            "{}: type {}{}{}",
            r.name.text,
            names::dns_type_label(r.rtype),
            if value.is_empty() { "" } else { ", " },
            value
        ),
        r.at,
        r.len,
    )
    .child(name_field(&r.name))
    .child(Field::new("Type", names::dns_type_value(r.rtype), tail, 2))
    .child(Field::new("Class", class_text(r.class, kind, false), tail + 2, 2))
    .child(
        Field::new("Time to live", format!("{} ({})", r.ttl, duration(u64::from(r.ttl))), tail + 4, 4)
            .explain("How long the answer may be remembered (cached) before asking again."),
    )
    .child(Field::new("Data length", byte_count(r.data_len), tail + 8, 2));
    let data = match &r.data {
        RData::A(a) => {
            Some(Field::new("Address", a.to_string(), r.data_at, 4).explain("The IPv4 address for this name."))
        }
        RData::Aaaa(a) => {
            Some(Field::new("Address", a.to_string(), r.data_at, 16).explain("The IPv6 address for this name."))
        }
        RData::Name(n) if r.rtype == 5 => Some(
            Field::new("Canonical name", n.text.clone(), n.at, n.len)
                .explain("The name is an alias: the real name is this one, often a content delivery network."),
        ),
        RData::Name(n) => {
            Some(Field::new(if r.rtype == 2 { "Name server" } else { "Domain name" }, n.text.clone(), n.at, n.len))
        }
        RData::Mx(pref, n) => {
            Some(Field::new("Mail server", format!("{} (preference {pref})", n.text), r.data_at, r.data_len))
        }
        RData::Txt(t) => Some(Field::new("Text", t.clone(), r.data_at, r.data_len)),
        RData::Srv { port, target, .. } => {
            Some(Field::new("Service", format!("{} port {port}", target.text), r.data_at, r.data_len))
        }
        RData::Svcb { priority, target } => Some(
            Field::new("Settings", format!("priority {priority}, target {}", target.text), r.data_at, r.data_len)
                .explain("Tells apps how best to connect, e.g. that the site supports HTTP/3."),
        ),
        RData::Soa(n) => Some(Field::new("Primary name server", n.text.clone(), n.at, n.len)),
        RData::Other if r.data_len > 0 => Some(Field::new("Data", byte_count(r.data_len), r.data_at, r.data_len)),
        RData::Other => None,
    };
    if let Some(d) = data {
        field = field.child(d);
    }
    field
}
