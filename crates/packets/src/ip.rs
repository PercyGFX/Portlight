//! IPv4 and IPv6 headers, including IPv6 extension headers.

use std::net::IpAddr;

use crate::bytes::{ipv4_at, ipv6_at, u8_at, u16_at, u32_at};
use crate::names;
use crate::text::{byte_count, hex};
use crate::{Field, Layer};

/// At most this many IPv6 extension headers are walked.
pub(crate) const MAX_EXT: usize = 8;

#[derive(Debug, Clone, Copy)]
pub(crate) struct Frag {
    /// In 8-byte units, as on the wire.
    pub offset: u16,
    pub id: u32,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Ext {
    pub kind: u8,
    pub at: usize,
    pub len: usize,
    pub next: u8,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Ip {
    pub v6: bool,
    pub at: usize,
    /// IPv4: the header with options. IPv6: 40 bytes plus extension headers.
    pub header_len: usize,
    pub src: IpAddr,
    pub dst: IpAddr,
    /// The protocol of the payload, after any extension headers.
    pub proto: u8,
    pub payload_at: usize,
    /// The end of this packet within the captured bytes.
    pub end: usize,
    pub ttl: u8,
    pub frag: Option<Frag>,
}

impl Ip {
    /// True unless this is a later piece of a fragmented packet (which has no transport header).
    pub(crate) fn first_fragment(&self) -> bool {
        self.frag.is_none_or(|f| f.offset == 0)
    }
}

/// Walks IPv6 extension headers, stopping at the upper-layer protocol, at a
/// header that does not fit, or after the fragment header of a later piece.
pub(crate) struct Exts<'a> {
    frame: &'a [u8],
    pos: usize,
    end: usize,
    next: u8,
    count: usize,
    done: bool,
}

impl Iterator for Exts<'_> {
    type Item = Ext;

    fn next(&mut self) -> Option<Ext> {
        if self.done || self.count == MAX_EXT {
            return None;
        }
        let (pos, kind) = (self.pos, self.next);
        let len = match kind {
            0 | 43 | 60 | 135 => (usize::from(u8_at(self.frame, pos + 1)?) + 1) * 8,
            51 => (usize::from(u8_at(self.frame, pos + 1)?) + 2) * 4,
            44 => 8,
            _ => return None,
        };
        if pos + len > self.end {
            return None;
        }
        let next = u8_at(self.frame, pos)?;
        if kind == 44 && u16_at(self.frame, pos + 2)? >> 3 != 0 {
            self.done = true;
        }
        self.pos += len;
        self.next = next;
        self.count += 1;
        Some(Ext { kind, at: pos, len, next })
    }
}

pub(crate) fn parse_v4(frame: &[u8], at: usize) -> Option<Ip> {
    let b0 = u8_at(frame, at)?;
    if b0 >> 4 != 4 {
        return None;
    }
    let src = ipv4_at(frame, at + 12)?;
    let dst = ipv4_at(frame, at + 16)?;
    let header_len = (usize::from(b0 & 0x0f) * 4).max(20);
    let total = usize::from(u16_at(frame, at + 2)?);
    // A total length of 0 is common for outgoing packets captured before the
    // network card splits them up (segmentation offload): use what was captured.
    let declared_end = if total >= header_len { at + total } else { frame.len() };
    let end = declared_end.min(frame.len());
    let flags_frag = u16_at(frame, at + 6)?;
    let offset = flags_frag & 0x1fff;
    let more = flags_frag & 0x2000 != 0;
    let frag = (offset != 0 || more).then(|| Frag { offset, id: u32::from(u16_at(frame, at + 4).unwrap_or(0)) });
    Some(Ip {
        v6: false,
        at,
        header_len,
        src: IpAddr::V4(src),
        dst: IpAddr::V4(dst),
        proto: u8_at(frame, at + 9)?,
        payload_at: (at + header_len).min(end),
        end,
        ttl: u8_at(frame, at + 8)?,
        frag,
    })
}

pub(crate) fn parse_v6(frame: &[u8], at: usize) -> Option<Ip> {
    if u8_at(frame, at)? >> 4 != 6 {
        return None;
    }
    let src = ipv6_at(frame, at + 8)?;
    let dst = ipv6_at(frame, at + 24)?;
    let payload_len = usize::from(u16_at(frame, at + 4)?);
    let declared_end = if payload_len == 0 { frame.len() } else { at + 40 + payload_len };
    let end = declared_end.min(frame.len());
    let proto = u8_at(frame, at + 6)?;
    let ttl = u8_at(frame, at + 7)?;

    let mut walk = Exts { frame, pos: at + 40, end, next: proto, count: 0, done: false };
    let mut frag = None;
    for ext in walk.by_ref() {
        if ext.kind == 44 {
            let field = u16_at(frame, ext.at + 2).unwrap_or(0);
            frag = Some(Frag { offset: field >> 3, id: u32_at(frame, ext.at + 4).unwrap_or(0) });
        }
    }
    let (pos, proto) = (walk.pos, walk.next);
    let payload_at = pos.min(end);
    Some(Ip {
        v6: true,
        at,
        header_len: payload_at - at,
        src: IpAddr::V6(src),
        dst: IpAddr::V6(dst),
        proto,
        payload_at,
        end,
        ttl,
        frag,
    })
}

const IPV4_EXPLAIN: &str = "IP carries the packet across the internet, from the sending computer's address to the \
    receiving one. Routers along the way read only this part to decide where to send it next.";
const IPV6_EXPLAIN: &str = "IPv6 is the newer version of IP, with much longer addresses. It carries the packet from \
    the sending computer's address to the receiving one; routers read only this part.";
const TTL_EXPLAIN: &str = "Each router on the way takes one off. At zero the packet is dropped, so a packet lost in a \
    loop does not travel forever. Windows starts at 128, Linux and phones at 64.";

pub(crate) fn layer(frame: &[u8], ip: &Ip) -> Layer {
    if ip.v6 { layer_v6(frame, ip) } else { layer_v4(frame, ip) }
}

fn layer_v4(frame: &[u8], ip: &Ip) -> Layer {
    let at = ip.at;
    let mut layer = Layer::new(
        "ip",
        "Internet Protocol version 4",
        format!("From {} to {}", ip.src, ip.dst),
        IPV4_EXPLAIN,
        at,
        (at + ip.header_len).min(frame.len()),
    );
    let b0 = u8_at(frame, at).unwrap_or(0);
    layer.push(Field::new("Version", "4", at, 1));
    layer.push(Field::new("Header length", format!("{} bytes", usize::from(b0 & 0x0f) * 4), at, 1));
    if let Some(tos) = u8_at(frame, at + 1) {
        let dscp = tos >> 2;
        let ecn = tos & 3;
        layer.push(
            Field::new("Differentiated services", format!("0x{tos:02x} (DSCP {dscp}, ECN {ecn})"), at + 1, 1).explain(
                "Asks routers for a kind of treatment, e.g. low delay for calls. Most traffic leaves this at 0.",
            ),
        );
    }
    if let Some(total) = u16_at(frame, at + 2) {
        let value = if total == 0 {
            "0 (left blank: the network card fills it in when it splits the data into packets)".to_string()
        } else {
            byte_count(usize::from(total))
        };
        layer
            .push(Field::new("Total length", value, at + 2, 2).explain("The size of this IP packet, header included."));
    }
    if let Some(id) = u16_at(frame, at + 4) {
        layer.push(
            Field::new("Identification", format!("0x{id:04x} ({id})"), at + 4, 2)
                .explain("Ties the pieces of a fragmented packet together."),
        );
    }
    if let Some(ff) = u16_at(frame, at + 6) {
        let flags = ff >> 13;
        let mut words = Vec::new();
        if flags & 2 != 0 {
            words.push("Don't fragment");
        }
        if flags & 1 != 0 {
            words.push("More fragments");
        }
        let words = if words.is_empty() { String::new() } else { format!(" ({})", words.join(", ")) };
        let set = |bit: u16| if flags & bit != 0 { "Set" } else { "Not set" };
        layer.push(
            Field::new("Flags", format!("0x{flags:x}{words}"), at + 6, 1)
                .explain("Whether routers may split this packet into smaller pieces, and whether more pieces follow.")
                .child(Field::new("Reserved bit", set(4), at + 6, 1))
                .child(Field::new("Don't fragment", set(2), at + 6, 1).explain(
                    "Routers must not split this packet. If it is too big for a link they drop it and say so instead.",
                ))
                .child(Field::new("More fragments", set(1), at + 6, 1).explain("More pieces of this packet follow.")),
        );
        let offset = ff & 0x1fff;
        layer.push(
            Field::new("Fragment offset", format!("{} bytes", u32::from(offset) * 8), at + 6, 2)
                .explain("Where this piece goes in the original packet. 0 for unsplit packets."),
        );
    }
    layer.push(
        Field::new("Time to live", format!("{} (hops left before the packet is dropped)", ip.ttl), at + 8, 1)
            .explain(TTL_EXPLAIN),
    );
    layer.push(
        Field::new("Protocol", names::ip_proto_value(ip.proto), at + 9, 1)
            .explain("What is inside: TCP for connections, UDP for quick messages, ICMP for network messages."),
    );
    if let Some(sum) = u16_at(frame, at + 10) {
        layer.push(checksum_field(at + 10, sum));
    }
    layer.push(
        Field::new("Source address", ip.src.to_string(), at + 12, 4)
            .explain("The address of the computer that sent this packet."),
    );
    layer.push(
        Field::new("Destination address", ip.dst.to_string(), at + 16, 4)
            .explain("The address of the computer this packet is for."),
    );
    if ip.header_len > 20 && frame.len() > at + 20 {
        let len = (ip.header_len - 20).min(frame.len() - (at + 20));
        layer.push(
            Field::new("Options", byte_count(len), at + 20, len).explain("Rarely used extra instructions for routers."),
        );
    }
    layer
}

fn layer_v6(frame: &[u8], ip: &Ip) -> Layer {
    let at = ip.at;
    let mut layer = Layer::new(
        "ipv6",
        "Internet Protocol version 6",
        format!("From {} to {}", ip.src, ip.dst),
        IPV6_EXPLAIN,
        at,
        (at + ip.header_len).min(frame.len()),
    );
    layer.push(Field::new("Version", "6", at, 1));
    if let Some(word) = u32_at(frame, at) {
        let class = (word >> 20) & 0xff;
        let flow = word & 0x000f_ffff;
        layer.push(
            Field::new("Traffic class", format!("0x{class:02x}"), at, 2)
                .explain("Asks routers for a kind of treatment, e.g. low delay for calls."),
        );
        layer.push(
            Field::new("Flow label", format!("0x{flow:05x}"), at + 1, 3)
                .explain("Lets routers keep the packets of one conversation together."),
        );
    }
    if let Some(len) = u16_at(frame, at + 4) {
        layer.push(
            Field::new("Payload length", byte_count(usize::from(len)), at + 4, 2)
                .explain("The size of everything after this 40-byte header."),
        );
    }
    let first_next = u8_at(frame, at + 6).unwrap_or(ip.proto);
    layer.push(
        Field::new("Next header", names::ip_proto_value(first_next), at + 6, 1)
            .explain("What follows: TCP, UDP, ICMPv6, or an extension header with extra instructions."),
    );
    layer.push(
        Field::new("Hop limit", format!("{} (hops left before the packet is dropped)", ip.ttl), at + 7, 1)
            .explain(TTL_EXPLAIN),
    );
    layer.push(
        Field::new("Source address", ip.src.to_string(), at + 8, 16)
            .explain("The address of the computer that sent this packet."),
    );
    layer.push(
        Field::new("Destination address", ip.dst.to_string(), at + 24, 16)
            .explain("The address of the computer this packet is for."),
    );
    let walk = Exts { frame, pos: at + 40, end: ip.end, next: first_next, count: 0, done: false };
    for ext in walk {
        layer.push(ext_field(frame, &ext));
    }
    layer
}

fn ext_field(frame: &[u8], ext: &Ext) -> Field {
    let (label, explain) = match ext.kind {
        0 => {
            ("Hop-by-hop options", "Instructions every router on the way must read. Used for multicast group messages.")
        }
        43 => ("Routing header", "Asks for the packet to go through particular routers."),
        44 => ("Fragment header", "This packet is one piece of a bigger one that was split up."),
        60 => ("Destination options", "Extra instructions for the receiving computer."),
        51 => ("Authentication header", "Part of IPsec: proves the packet was not changed on the way."),
        _ => ("Mobility header", "Used by devices that keep their address while moving between networks."),
    };
    let mut field = Field::new(label, byte_count(ext.len), ext.at, ext.len).explain(explain).child(Field::new(
        "Next header",
        names::ip_proto_value(ext.next),
        ext.at,
        1,
    ));
    if ext.kind == 44
        && let (Some(off), Some(id)) = (u16_at(frame, ext.at + 2), u32_at(frame, ext.at + 4))
    {
        field = field
            .child(Field::new("Fragment offset", format!("{} bytes", u32::from(off >> 3) * 8), ext.at + 2, 2))
            .child(Field::new("More fragments", if off & 1 != 0 { "Yes" } else { "No" }, ext.at + 3, 1))
            .child(Field::new("Identification", format!("0x{id:08x}"), ext.at + 4, 4));
    } else if ext.len > 2 {
        let data = crate::bytes::range(frame, ext.at + 2, ext.at + ext.len);
        field = field.child(Field::new("Options", hex(data, 16), ext.at + 2, ext.len - 2));
    }
    field
}

pub(crate) fn checksum_field(at: usize, sum: u16) -> Field {
    Field::new("Checksum", format!("0x{sum:04x} (not checked)"), at, 2).explain(
        "Detects damaged packets. Windows often leaves this to the network card, so outgoing packets \
         can show a wrong value here; that is normal.",
    )
}

/// One line for the fragment case, shared by summary and dissect.
pub(crate) fn fragment_info(ip: &Ip) -> String {
    let proto = names::ip_proto(ip.proto).unwrap_or("unknown");
    let (offset, id) = ip.frag.map_or((0, 0), |f| (u32::from(f.offset) * 8, f.id));
    format!("Fragmented IP protocol (proto={proto} {}, off={offset}, ID={id:04x})", ip.proto)
}
