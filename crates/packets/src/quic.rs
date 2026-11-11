//! QUIC: encrypted web traffic (HTTP/3) over UDP. Only the header is readable.

use crate::bytes::{range, u8_at, u32_at};
use crate::packet::Payload;
use crate::summary::{Desc, Who};
use crate::text::{byte_count, hex};
use crate::{Field, Layer};

const V2: u32 = 0x6b33_43cf;

#[derive(Debug, Clone, Copy)]
pub(crate) struct Quic {
    pub long: bool,
    pub version: Option<u32>,
    pub kind: &'static str,
    pub dcid: Option<(usize, usize)>,
    pub scid: Option<(usize, usize)>,
    /// Where the encrypted part starts, as far as the header can be read.
    pub rest_at: usize,
}

pub(crate) fn parse(frame: &[u8], at: usize, end: usize) -> Option<Quic> {
    let b = frame.get(..end)?;
    let first = u8_at(b, at)?;
    if first & 0x80 == 0 {
        let kp = if first & 0x04 != 0 { "Protected Payload (KP1)" } else { "Protected Payload (KP0)" };
        return Some(Quic { long: false, version: None, kind: kp, dcid: None, scid: None, rest_at: at + 1 });
    }
    let version = u32_at(b, at + 1)?;
    let kind = if version == 0 {
        "Version Negotiation"
    } else {
        let bits = (first >> 4) & 3;
        let order = if version == V2 {
            ["Retry", "Initial", "0-RTT", "Handshake"]
        } else {
            ["Initial", "0-RTT", "Handshake", "Retry"]
        };
        order[usize::from(bits)]
    };
    let mut q = Quic { long: true, version: Some(version), kind, dcid: None, scid: None, rest_at: at + 5 };
    let Some(dlen) = u8_at(b, at + 5) else { return Some(q) };
    let d_at = at + 6;
    if d_at + usize::from(dlen) > end {
        return Some(q);
    }
    q.dcid = Some((d_at, usize::from(dlen)));
    let s_len_at = d_at + usize::from(dlen);
    let Some(slen) = u8_at(b, s_len_at) else { return Some(q) };
    if s_len_at + 1 + usize::from(slen) > end {
        return Some(q);
    }
    q.scid = Some((s_len_at + 1, usize::from(slen)));
    q.rest_at = s_len_at + 1 + usize::from(slen);
    Some(q)
}

fn version_text(v: u32) -> String {
    match v {
        0 => "0 (version negotiation)".to_string(),
        1 => "1".to_string(),
        V2 => "2".to_string(),
        v if v & 0x0f0f_0f0f == 0x0a0a_0a0a => format!("0x{v:08x} (reserved, for testing)"),
        v if v >> 8 == 0x00ff_0000 => format!("draft-{}", v & 0xff),
        v => format!("0x{v:08x}"),
    }
}

pub(crate) fn info(frame: &[u8], q: &Quic) -> String {
    let mut s = q.kind.to_string();
    for (label, cid) in [("DCID", q.dcid), ("SCID", q.scid)] {
        if let Some((at, len)) = cid
            && len > 0
        {
            s.push_str(&format!(", {label}={}", hex(range(frame, at, at + len), 20)));
        }
    }
    s
}

pub(crate) fn describe(frame: &[u8], p: &Payload, who: &Who) -> Option<Desc> {
    let q = parse(frame, p.at, p.end)?;
    let (src, dst) = (&who.src, &who.dst);
    let to_server = p.dst_port == 443;
    let summary = match q.kind {
        "Initial" if to_server => format!("{src} starts a QUIC (HTTP/3) connection with {dst}"),
        "Initial" => format!("{src} answers the QUIC connection from {dst}"),
        "Handshake" => format!("{src} and {dst} set up encryption for the QUIC connection"),
        "0-RTT" => format!("{src} sends early encrypted data to {dst}"),
        "Retry" => format!("{src} asks {dst} to try the connection again"),
        "Version Negotiation" => format!("{src} tells {dst} which QUIC versions it supports"),
        _ => format!("{src} sends {} of encrypted QUIC data to {dst}", byte_count(p.len())),
    };
    Some(Desc::new("QUIC", info(frame, &q), summary))
}

pub(crate) fn layer(frame: &[u8], p: &Payload) -> Option<Layer> {
    let q = parse(frame, p.at, p.end)?;
    let at = p.at;
    let mut layer = Layer::new(
        "quic",
        "QUIC",
        info(frame, &q),
        "QUIC is a newer way to carry web traffic (HTTP/3) over UDP. It is encrypted from the first packet, \
         so only a few header details are visible.",
        p.at,
        p.end,
    );
    let form = if q.long { "Long header" } else { "Short header" };
    layer.push(
        Field::new("Header form", format!("{form} ({})", q.kind), at, 1).explain(
            "Long headers are used while a connection is being set up; short headers carry the data afterwards.",
        ),
    );
    if let Some(v) = q.version {
        layer.push(
            Field::new("Version", version_text(v), at + 1, 4).explain("Which version of QUIC this connection uses."),
        );
    }
    if let Some((d, len)) = q.dcid {
        layer.push(Field::new("Destination connection ID length", len.to_string(), d - 1, 1));
        layer.push(
            Field::new("Destination connection ID", hex(range(frame, d, d + len), 20), d, len)
                .explain("Names the connection, so it survives a change of network (e.g. Wi-Fi to mobile data)."),
        );
    }
    if let Some((s, len)) = q.scid {
        layer.push(Field::new("Source connection ID length", len.to_string(), s - 1, 1));
        layer.push(Field::new("Source connection ID", hex(range(frame, s, s + len), 20), s, len));
    }
    let rest = p.end.saturating_sub(q.rest_at);
    if rest > 0 {
        layer.push(
            Field::new("Encrypted payload", byte_count(rest), q.rest_at, rest)
                .explain("Encrypted — the contents are private. Even the packet numbers are hidden."),
        );
    }
    Some(layer)
}
