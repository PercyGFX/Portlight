//! ICMP and ICMPv6 (network messages, ping, neighbour discovery) and IGMP.

use std::fmt::Write;
use std::net::{IpAddr, Ipv4Addr};

use crate::bytes::{array, ipv4_at, ipv6_at, range, u8_at, u16_at, u32_at};
use crate::names;
use crate::summary::{Desc, Who};
use crate::text::{byte_count, mac};
use crate::{Field, Layer};

#[derive(Debug, Clone, Copy)]
pub(crate) struct Icmp {
    pub v6: bool,
    pub at: usize,
    pub kind: u8,
    pub code: u8,
    pub end: usize,
}

pub(crate) fn parse(frame: &[u8], at: usize, end: usize, v6: bool) -> Option<Icmp> {
    let b = frame.get(..end)?;
    u16_at(b, at + 2)?;
    Some(Icmp { v6, at, kind: u8_at(b, at)?, code: u8_at(b, at + 1)?, end })
}

fn type_name(i: &Icmp) -> &'static str {
    if i.v6 {
        match i.kind {
            1 => "Destination unreachable",
            2 => "Packet too big",
            3 => "Time exceeded",
            4 => "Parameter problem",
            128 => "Echo (ping) request",
            129 => "Echo (ping) reply",
            130 => "Multicast Listener Query",
            131 => "Multicast Listener Report",
            132 => "Multicast Listener Done",
            133 => "Router Solicitation",
            134 => "Router Advertisement",
            135 => "Neighbor Solicitation",
            136 => "Neighbor Advertisement",
            137 => "Redirect",
            143 => "Multicast Listener Report Message v2",
            _ => "Unknown",
        }
    } else {
        match i.kind {
            0 => "Echo (ping) reply",
            3 => "Destination unreachable",
            4 => "Source quench",
            5 => "Redirect",
            8 => "Echo (ping) request",
            9 => "Router advertisement",
            10 => "Router solicitation",
            11 => "Time-to-live exceeded",
            12 => "Parameter problem",
            13 => "Timestamp request",
            14 => "Timestamp reply",
            _ => "Unknown",
        }
    }
}

fn code_name(i: &Icmp) -> Option<&'static str> {
    Some(match (i.v6, i.kind, i.code) {
        (false, 3, 0) => "Network unreachable",
        (false, 3, 1) => "Host unreachable",
        (false, 3, 2) => "Protocol unreachable",
        (false, 3, 3) => "Port unreachable",
        (false, 3, 4) => "Fragmentation needed",
        (false, 3, 6) => "Destination network unknown",
        (false, 3, 7) => "Destination host unknown",
        (false, 3, 9 | 10 | 13) => "Communication administratively prohibited",
        (false, 5, 0) => "Redirect for network",
        (false, 5, 1) => "Redirect for host",
        (false, 11, 0) => "Time to live exceeded in transit",
        (false, 11, 1) => "Fragment reassembly time exceeded",
        (true, 1, 0) => "No route to destination",
        (true, 1, 1) => "Administratively prohibited",
        (true, 1, 3) => "Address unreachable",
        (true, 1, 4) => "Port unreachable",
        (true, 3, 0) => "Hop limit exceeded in transit",
        (true, 3, 1) => "Fragment reassembly time exceeded",
        _ => return None,
    })
}

fn is_echo(i: &Icmp) -> Option<bool> {
    match (i.v6, i.kind) {
        (false, 8) | (true, 128) => Some(true),
        (false, 0) | (true, 129) => Some(false),
        _ => None,
    }
}

/// The packet quoted inside an error message.
struct Quoted {
    dst: IpAddr,
    /// "From A to B, UDP, port 5000 → 53".
    text: String,
    /// "B port 53".
    target: String,
}

fn quoted_packet(frame: &[u8], at: usize, end: usize) -> Option<Quoted> {
    let b = frame.get(..end)?;
    let version = u8_at(b, at)? >> 4;
    let (dst, src, proto, l4) = if version == 4 {
        let ihl = usize::from(u8_at(b, at)? & 0x0f) * 4;
        (IpAddr::V4(ipv4_at(b, at + 16)?), IpAddr::V4(ipv4_at(b, at + 12)?), u8_at(b, at + 9)?, at + ihl.max(20))
    } else if version == 6 {
        (IpAddr::V6(ipv6_at(b, at + 24)?), IpAddr::V6(ipv6_at(b, at + 8)?), u8_at(b, at + 6)?, at + 40)
    } else {
        return None;
    };
    let mut text = format!("From {src} to {dst}, {}", names::ip_proto(proto).unwrap_or("unknown protocol"));
    let mut target = dst.to_string();
    if matches!(proto, 6 | 17)
        && let (Some(sp), Some(dp)) = (u16_at(b, l4), u16_at(b, l4 + 2))
    {
        let _ = write!(text, ", port {sp} → {dp}");
        target = format!("{dst} port {dp}");
    }
    Some(Quoted { dst, text, target })
}

fn echo_ids(frame: &[u8], i: &Icmp) -> (u16, u16) {
    (u16_at(frame, i.at + 4).unwrap_or(0), u16_at(frame, i.at + 6).unwrap_or(0))
}

/// The link-layer address option of a neighbour discovery message.
fn ndp_mac(frame: &[u8], i: &Icmp, wanted: u8) -> Option<[u8; 6]> {
    let start = ndp_options_at(i)?;
    ndp_options(frame, start, i.end)
        .into_iter()
        .find(|o| o.0 == wanted && o.2 >= 8)
        .and_then(|o| array::<6>(frame, o.1 + 2))
}

fn ndp_options_at(i: &Icmp) -> Option<usize> {
    match i.kind {
        133 => Some(i.at + 8),
        134 => Some(i.at + 16),
        135 | 136 => Some(i.at + 24),
        137 => Some(i.at + 40),
        _ => None,
    }
}

/// (type, start, length) of each option; at most 16.
fn ndp_options(frame: &[u8], mut pos: usize, end: usize) -> Vec<(u8, usize, usize)> {
    let mut out = Vec::new();
    while out.len() < 16 {
        let (Some(kind), Some(units)) = (u8_at(frame, pos), u8_at(frame, pos + 1)) else { break };
        let len = usize::from(units) * 8;
        if len == 0 || pos + len > end {
            break;
        }
        out.push((kind, pos, len));
        pos += len;
    }
    out
}

pub(crate) fn info(frame: &[u8], i: &Icmp) -> String {
    let name = type_name(i);
    if is_echo(i).is_some() {
        let (id, seq) = echo_ids(frame, i);
        return format!("{name} id=0x{id:04x}, seq={seq}");
    }
    if i.v6 {
        let target = ipv6_at(frame, i.at + 8);
        match (i.kind, target) {
            (135, Some(t)) => {
                let mut s = format!("Neighbor Solicitation for {t}");
                if let Some(m) = ndp_mac(frame, i, 1) {
                    let _ = write!(s, " from {}", mac(&m));
                }
                return s;
            }
            (136, Some(t)) => {
                let flags = u8_at(frame, i.at + 4).unwrap_or(0);
                let words: Vec<&str> = [(0x80, "rtr"), (0x40, "sol"), (0x20, "ovr")]
                    .iter()
                    .filter(|(bit, _)| flags & bit != 0)
                    .map(|(_, w)| *w)
                    .collect();
                let mut s = format!("Neighbor Advertisement {t}");
                if !words.is_empty() {
                    let _ = write!(s, " ({})", words.join(", "));
                }
                if let Some(m) = ndp_mac(frame, i, 2) {
                    let _ = write!(s, " is at {}", mac(&m));
                }
                return s;
            }
            (133 | 134, _) => {
                let mut s = name.to_string();
                if let Some(m) = ndp_mac(frame, i, 1) {
                    let _ = write!(s, " from {}", mac(&m));
                }
                return s;
            }
            _ => {}
        }
    }
    match code_name(i) {
        Some(code) => format!("{name} ({code})"),
        None if name == "Unknown" => format!("Type {}, code {}", i.kind, i.code),
        None => name.to_string(),
    }
}

pub(crate) fn describe(frame: &[u8], i: &Icmp, who: &Who) -> Desc {
    let protocol = if i.v6 { "ICMPv6" } else { "ICMP" };
    let (src, dst) = (&who.src, &who.dst);
    let quoted = quoted_packet(frame, i.at + 8, i.end);
    let target = |fallback: &str| match &quoted {
        Some(q) => match who.name_opt(q.dst) {
            Some(name) => q.target.replacen(&q.dst.to_string(), &name, 1),
            None => q.target.clone(),
        },
        None => fallback.to_string(),
    };
    let summary = match (i.v6, i.kind) {
        _ if is_echo(i) == Some(true) => format!("{src} pings {dst}"),
        _ if is_echo(i) == Some(false) => format!("{src} replies to the ping"),
        (false, 3) | (true, 1) => {
            let why = code_name(i).map(|c| c.to_lowercase()).unwrap_or_else(|| "unreachable".to_string());
            format!("{src} reports that {} could not be reached ({why})", target("the destination"))
        }
        (false, 11) | (true, 3) => format!(
            "{src} reports that a packet to {} ran out of hops on the way (traceroute does this on purpose)",
            target("its destination")
        ),
        (true, 2) => {
            let mtu = u32_at(frame, i.at + 4).unwrap_or(0);
            format!(
                "{src} says a packet was too big for the path (the limit is {} bytes)",
                crate::text::thousands(u64::from(mtu))
            )
        }
        (false, 5) | (true, 137) => format!("{src} tells {dst} to use a different router"),
        (false, 12) | (true, 4) => format!("{src} reports a damaged packet header"),
        (true, 133) => format!("{src} looks for routers on the local network"),
        (true, 134) => format!("{src} (a router) announces itself and the network's settings"),
        (true, 135) => match ipv6_at(frame, i.at + 8) {
            Some(t) if who.src_ip.is_some_and(|ip| ip.is_unspecified()) => {
                format!("A device checks that nobody else on the local network uses {t}")
            }
            Some(t) => format!("{src} asks who on the local network has {t}"),
            None => format!("{src} asks who on the local network has an address"),
        },
        (true, 136) => match (ipv6_at(frame, i.at + 8), ndp_mac(frame, i, 2)) {
            (Some(t), Some(m)) => format!("{src} answers: {t} is at {}", mac(&m)),
            (Some(t), None) => format!("{src} answers: it has the address {t}"),
            _ => format!("{src} answers a neighbour question"),
        },
        (true, 130) => format!("{src} asks which multicast groups devices want to receive"),
        (true, 131 | 143) => format!("{src} tells the network which multicast groups it wants to receive"),
        (true, 132) => format!("{src} leaves a multicast group"),
        _ => format!("{src} sends a network message ({}) to {dst}", type_name(i).to_lowercase()),
    };
    Desc::new(protocol, info(frame, i), summary)
}

pub(crate) fn layer(frame: &[u8], i: &Icmp) -> Layer {
    let at = i.at;
    let (id, title, explain) = if i.v6 {
        (
            "icmpv6",
            "Internet Control Message Protocol v6 (ICMPv6)",
            "ICMPv6 carries short control messages for IPv6: ping, error reports, and how devices on the local \
             network find each other and the router.",
        )
    } else {
        (
            "icmp",
            "Internet Control Message Protocol (ICMP)",
            "ICMP carries short messages about the network itself: ping requests and replies, and reports that \
             a packet could not be delivered.",
        )
    };
    let mut layer = Layer::new(id, title, info(frame, i), explain, at, i.end);
    layer.push(
        Field::new("Type", format!("{} ({})", i.kind, type_name(i)), at, 1).explain("What kind of message this is."),
    );
    let code = code_name(i).map_or(i.code.to_string(), |c| format!("{} ({c})", i.code));
    layer.push(
        Field::new("Code", code, at + 1, 1)
            .explain("More detail for the message type, e.g. why something was unreachable."),
    );
    if let Some(sum) = u16_at(frame, at + 2) {
        layer.push(crate::ip::checksum_field(at + 2, sum));
    }
    if is_echo(i).is_some() {
        let (ident, seq) = echo_ids(frame, i);
        if u16_at(frame, at + 6).is_some() {
            layer.push(
                Field::new("Identifier", format!("0x{ident:04x} ({ident})"), at + 4, 2)
                    .explain("Matches replies to requests when several pings run at once."),
            );
            layer.push(Field::new("Sequence number", seq.to_string(), at + 6, 2).explain("Counts the pings: 1, 2, 3…"));
        }
        let data = i.end.saturating_sub(at + 8);
        if data > 0 {
            layer.push(
                Field::new("Data", byte_count(data), at + 8, data).explain("Filler the reply sends back unchanged."),
            );
        }
        return layer;
    }
    match (i.v6, i.kind) {
        (false, 3 | 11 | 12) | (true, 1..=4) => {
            if i.v6
                && i.kind == 2
                && let Some(mtu) = u32_at(frame, at + 4)
            {
                layer.push(
                    Field::new("MTU", format!("{mtu} bytes"), at + 4, 4)
                        .explain("The biggest packet the next link can carry."),
                );
            } else if !i.v6
                && i.kind == 3
                && i.code == 4
                && let Some(mtu) = u16_at(frame, at + 6)
            {
                layer.push(
                    Field::new("Next-hop MTU", format!("{mtu} bytes"), at + 6, 2)
                        .explain("The biggest packet the next link can carry."),
                );
            }
            if let Some(q) = quoted_packet(frame, at + 8, i.end) {
                let len = i.end.saturating_sub(at + 8);
                layer.push(Field::new("Original packet", q.text, at + 8, len).explain(
                    "The start of the packet this message is about, sent back so the sender knows which one it was.",
                ));
            }
        }
        (false, 5) => {
            if let Some(gw) = ipv4_at(frame, at + 4) {
                layer.push(Field::new("Better router", gw.to_string(), at + 4, 4));
            }
        }
        (true, 133..=137) => push_ndp(frame, i, &mut layer),
        _ => {
            let rest = i.end.saturating_sub(at + 4);
            if rest > 0 {
                layer.push(Field::new("Message body", byte_count(rest), at + 4, rest));
            }
        }
    }
    layer
}

fn push_ndp(frame: &[u8], i: &Icmp, layer: &mut Layer) {
    let at = i.at;
    match i.kind {
        134 => {
            if let Some(hops) = u8_at(frame, at + 4) {
                layer.push(Field::new("Hop limit for devices", hops.to_string(), at + 4, 1));
            }
            if let Some(flags) = u8_at(frame, at + 5) {
                let managed = flags & 0x80 != 0;
                layer.push(
                    Field::new("Flags", format!("0x{flags:02x}"), at + 5, 1)
                        .explain("Whether devices should also ask a DHCPv6 server for an address or settings.")
                        .child(Field::new(
                            "Managed address configuration",
                            if managed { "Set" } else { "Not set" },
                            at + 5,
                            1,
                        ))
                        .child(Field::new(
                            "Other configuration",
                            if flags & 0x40 != 0 { "Set" } else { "Not set" },
                            at + 5,
                            1,
                        )),
                );
            }
            if let Some(life) = u16_at(frame, at + 6) {
                layer.push(
                    Field::new("Router lifetime", crate::text::duration(u64::from(life)), at + 6, 2)
                        .explain("How long devices may use this router as their way out. 0 = not a default router."),
                );
            }
        }
        135..=137 => {
            if let Some(t) = ipv6_at(frame, at + 8) {
                layer.push(
                    Field::new("Target address", t.to_string(), at + 8, 16)
                        .explain("The address being asked or answered about."),
                );
            }
            if i.kind == 136
                && let Some(flags) = u8_at(frame, at + 4)
            {
                let set = |bit: u8| if flags & bit != 0 { "Set" } else { "Not set" };
                layer.push(
                    Field::new("Flags", format!("0x{flags:02x}"), at + 4, 4)
                        .child(Field::new("Router", set(0x80), at + 4, 1).explain("The sender is a router."))
                        .child(
                            Field::new("Solicited", set(0x40), at + 4, 1)
                                .explain("This answers a question (rather than an announcement)."),
                        )
                        .child(
                            Field::new("Override", set(0x20), at + 4, 1)
                                .explain("Replace any address remembered before."),
                        ),
                );
            }
        }
        _ => {}
    }
    let Some(start) = ndp_options_at(i) else { return };
    for (kind, pos, len) in ndp_options(frame, start, i.end) {
        let field = match kind {
            1 | 2 => {
                let label = if kind == 1 { "Source link-layer address" } else { "Target link-layer address" };
                let value = array::<6>(frame, pos + 2)
                    .map_or_else(|| crate::text::hex(range(frame, pos + 2, pos + len), 8), |m| mac(&m));
                Field::new(label, value, pos, len)
                    .explain("The hardware (MAC) address that goes with the IPv6 address.")
            }
            3 => {
                let prefix_len = u8_at(frame, pos + 2).unwrap_or(0);
                let prefix = ipv6_at(frame, pos + 16).map_or("?".to_string(), |p| p.to_string());
                Field::new("Prefix information", format!("{prefix}/{prefix_len}"), pos, len)
                    .explain("The network's address range. Devices build their own address inside it.")
            }
            5 => {
                let mtu = u32_at(frame, pos + 4).unwrap_or(0);
                Field::new("MTU", format!("{mtu} bytes"), pos, len).explain("The biggest packet this network carries.")
            }
            25 => {
                let servers: Vec<String> = (0..(len.saturating_sub(8)) / 16)
                    .filter_map(|n| ipv6_at(frame, pos + 8 + n * 16))
                    .map(|a| a.to_string())
                    .collect();
                Field::new("DNS servers", servers.join(", "), pos, len)
                    .explain("Which DNS servers devices should use for name lookups.")
            }
            _ => Field::new("Option", format!("type {kind}, {}", byte_count(len)), pos, len),
        };
        layer.push(field);
    }
}

// IGMP

fn igmp_name(kind: u8) -> &'static str {
    match kind {
        0x11 => "Membership Query",
        0x12 => "Membership Report (v1)",
        0x16 => "Membership Report",
        0x17 => "Leave Group",
        0x22 => "Membership Report (v3)",
        _ => "Unknown",
    }
}

pub(crate) fn igmp_describe(frame: &[u8], at: usize, who: &Who) -> Desc {
    let kind = u8_at(frame, at).unwrap_or(0);
    let group = ipv4_at(frame, at + 4).filter(|g| !g.is_unspecified() && kind != 0x22);
    let mut info = igmp_name(kind).to_string();
    if let Some(g) = group {
        let _ = write!(info, " group {g}");
    }
    let src = &who.src;
    let summary = match kind {
        0x11 => format!("{src} (a router) asks which multicast groups devices want to receive"),
        0x17 => format!("{src} leaves the multicast group {}", group.unwrap_or(Ipv4Addr::UNSPECIFIED)),
        0x12 | 0x16 | 0x22 => format!("{src} tells the network which multicast groups it wants to receive"),
        _ => format!("{src} sends a multicast group message"),
    };
    Desc::new("IGMP", info, summary)
}

pub(crate) fn igmp_layer(frame: &[u8], at: usize, end: usize) -> Layer {
    let kind = u8_at(frame, at).unwrap_or(0);
    let mut layer = Layer::new(
        "igmp",
        "Internet Group Management Protocol (IGMP)",
        igmp_name(kind),
        "IGMP tells the router which multicast groups (shared streams, such as device discovery on the local \
         network) this device wants to receive.",
        at,
        end,
    );
    layer.push(Field::new("Type", format!("0x{kind:02x} ({})", igmp_name(kind)), at, 1));
    if let Some(sum) = u16_at(frame, at + 2) {
        layer.push(crate::ip::checksum_field(at + 2, sum));
    }
    if kind != 0x22
        && let Some(g) = ipv4_at(frame, at + 4)
    {
        layer.push(
            Field::new("Group address", g.to_string(), at + 4, 4).explain("The multicast group this message is about."),
        );
    }
    layer
}
