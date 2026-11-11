//! One row of the packet list: who talks to whom, the highest protocol
//! understood, a technical line and a plain-English sentence.

use std::net::IpAddr;

use crate::bytes::u8_at;
use crate::link::{self, BROADCAST};
use crate::packet::{App, Headers, L4, Net, Payload};
use crate::text::mac;
use crate::{Context, Direction, LinkType, Summary, arp, dhcp, dns, http, icmp, ip, misc, names, quic, tcp, tls, udp};

/// What a packet is: protocol column, technical line, plain sentence.
pub(crate) struct Desc {
    pub protocol: &'static str,
    pub info: String,
    pub summary: String,
}

impl Desc {
    pub(crate) fn new(protocol: &'static str, info: String, summary: String) -> Self {
        Desc { protocol, info, summary }
    }
}

/// How to call the two ends of a packet in a sentence.
pub(crate) struct Who<'a> {
    /// "This PC", a name the app looked up, or the address.
    pub src: String,
    pub dst: String,
    pub src_ip: Option<IpAddr>,
    pub ctx: &'a dyn Context,
}

impl<'a> Who<'a> {
    pub(crate) fn new(h: &Headers, ctx: &'a dyn Context) -> Self {
        let (src_ip, dst_ip) = match &h.net {
            Net::Ip(ip) => (Some(ip.src), Some(ip.dst)),
            Net::Arp(a) => (a.addrs.map(|a| IpAddr::V4(a.spa)), a.addrs.map(|a| IpAddr::V4(a.tpa))),
            Net::None => (None, None),
        };
        let mut who = Who { src: String::new(), dst: String::new(), src_ip, ctx };
        match (src_ip, dst_ip, h.link.eth()) {
            (Some(s), Some(d), _) => {
                who.src = who.name(s);
                who.dst = who.name(d);
            }
            (_, _, Some(eth)) => {
                who.src = mac_phrase(&eth.src);
                who.dst = mac_phrase(&eth.dst);
            }
            _ => {
                who.src = "A device".to_string();
                who.dst = "another device".to_string();
            }
        }
        who
    }

    /// "This PC", the name an app looked up, or a phrase for the address.
    pub(crate) fn name(&self, ip: IpAddr) -> String {
        self.name_opt(ip).unwrap_or_else(|| address_phrase(ip))
    }

    /// "This PC" or the name an app looked up.
    pub(crate) fn name_opt(&self, ip: IpAddr) -> Option<String> {
        if self.ctx.is_local(ip) {
            return Some("This PC".to_string());
        }
        self.ctx.name_of(ip)
    }
}

fn address_phrase(ip: IpAddr) -> String {
    match ip {
        IpAddr::V4(v4) if v4.is_broadcast() => "everyone on the local network".to_string(),
        ip if ip.is_multicast() => format!("the multicast group {ip}"),
        ip => ip.to_string(),
    }
}

fn mac_phrase(m: &[u8; 6]) -> String {
    if *m == BROADCAST { "everyone on the local network".to_string() } else { mac(m) }
}

/// The name column: "This PC", a looked-up name, or "Broadcast".
fn display_name(ip: IpAddr, ctx: &dyn Context) -> Option<String> {
    if ctx.is_local(ip) {
        return Some("This PC".to_string());
    }
    if let Some(name) = ctx.name_of(ip) {
        return Some(name);
    }
    matches!(ip, IpAddr::V4(v4) if v4.is_broadcast()).then(|| "Broadcast".to_string())
}

/// One row of the packet list.
pub fn summarize(link: LinkType, frame: &[u8], original_len: u32, ctx: &dyn Context) -> Summary {
    let h = Headers::parse(link, frame);
    let who = Who::new(&h, ctx);
    let desc = describe(&h, frame, &who);
    let mut row = Summary {
        source: String::new(),
        destination: String::new(),
        source_name: None,
        destination_name: None,
        source_port: None,
        destination_port: None,
        protocol: desc.protocol.to_string(),
        length: if original_len == 0 { frame.len().try_into().unwrap_or(u32::MAX) } else { original_len },
        info: desc.info,
        summary: desc.summary,
        direction: None,
    };
    match (&h.net, h.link.eth()) {
        (Net::Ip(ip), _) => {
            row.source = ip.src.to_string();
            row.destination = ip.dst.to_string();
            row.source_name = display_name(ip.src, ctx);
            row.destination_name = display_name(ip.dst, ctx);
            row.direction = direction(ctx, ip.src, ip.dst);
            if let Some((sp, dp)) = h.ports() {
                row.source_port = Some(sp);
                row.destination_port = Some(dp);
            }
        }
        (Net::Arp(arp), eth) => {
            let (src_mac, dst_mac) = match (arp.addrs, eth) {
                (_, Some(eth)) => (eth.src, eth.dst),
                (Some(a), None) => (a.sha, a.tha),
                (None, None) => ([0; 6], [0; 6]),
            };
            row.source = mac(&src_mac);
            row.destination = mac(&dst_mac);
            if let Some(a) = arp.addrs {
                let (spa, tpa) = (IpAddr::V4(a.spa), IpAddr::V4(a.tpa));
                row.source_name = who.name_opt(spa);
                row.destination_name =
                    if dst_mac == BROADCAST { Some("Broadcast".to_string()) } else { who.name_opt(tpa) };
                row.direction = direction(ctx, spa, tpa);
            }
        }
        (Net::None, Some(eth)) => {
            row.source = mac(&eth.src);
            row.destination = mac(&eth.dst);
            if eth.dst == BROADCAST {
                row.destination_name = Some("Broadcast".to_string());
            }
        }
        (Net::None, None) => {}
    }
    row
}

fn direction(ctx: &dyn Context, src: IpAddr, dst: IpAddr) -> Option<Direction> {
    if ctx.is_local(src) {
        Some(Direction::Out)
    } else if ctx.is_local(dst) {
        Some(Direction::In)
    } else {
        None
    }
}

/// Describes a parsed frame.
pub(crate) fn describe(h: &Headers, frame: &[u8], who: &Who) -> Desc {
    match &h.net {
        Net::Arp(a) => arp::describe(a, who),
        Net::Ip(ip) => describe_ip(h, ip, frame, who),
        Net::None => describe_link(h, frame, who),
    }
}

fn describe_ip(h: &Headers, ip: &ip::Ip, frame: &[u8], who: &Who) -> Desc {
    let family = if ip.v6 { "IPv6" } else { "IPv4" };
    if !ip.first_fragment() {
        return Desc::new(
            family,
            ip::fragment_info(ip),
            format!("{} sends {} the next piece of a packet that was split up on the way", who.src, who.dst),
        );
    }
    match &h.l4 {
        L4::Tcp(t) => h
            .payload_info()
            .and_then(|p| describe_app(h, &p, frame, who))
            .unwrap_or_else(|| tcp::describe(frame, t, who)),
        L4::Udp(u) => {
            h.payload_info().and_then(|p| describe_app(h, &p, frame, who)).unwrap_or_else(|| udp::describe(u, who))
        }
        L4::Icmp(i) => icmp::describe(frame, i, who),
        L4::Igmp { at, .. } => icmp::igmp_describe(frame, *at, who),
        L4::None => {
            let name = names::ip_proto(ip.proto);
            let (protocol, info) = match (ip.proto, name) {
                (6, _) => ("TCP", "TCP header cut short in the capture".to_string()),
                (17, _) => ("UDP", "UDP header cut short in the capture".to_string()),
                (_, Some(n)) => (family, format!("{n} ({})", ip.proto)),
                (p, None) => (family, format!("IP protocol {p}")),
            };
            let what = name.unwrap_or("IP");
            Desc::new(protocol, info, format!("{} sends a {what} packet to {}", who.src, who.dst))
        }
    }
}

fn describe_app(h: &Headers, p: &Payload, frame: &[u8], who: &Who) -> Option<Desc> {
    match h.app {
        App::None => None,
        App::Dns(kind) => dns::describe(frame, p, kind, who),
        App::Tls => Some(tls::describe(frame, p, who)),
        App::Http => Some(http::describe(frame, p, who)),
        App::Quic => quic::describe(frame, p, who),
        App::Dhcp => dhcp::describe(frame, p, who),
        App::Ntp => misc::ntp_describe(frame, p, who),
        App::Ssdp => Some(misc::ssdp_describe(frame, p, who)),
        App::Nbns | App::Nbds => Some(misc::netbios_describe(h.app, p, who)),
    }
}

fn describe_link(h: &Headers, frame: &[u8], who: &Who) -> Desc {
    let eth = match &h.link.kind {
        link::Kind::Ethernet(eth) => eth,
        link::Kind::Unreadable => {
            return match h.link_type {
                LinkType::Unknown(dlt) => Desc::new(
                    "Unknown",
                    format!("Link type {dlt} is not supported"),
                    "A packet from a kind of network Portlight cannot read".to_string(),
                ),
                _ => Desc::new(
                    "Unknown",
                    format!("Frame too short ({})", crate::text::byte_count(frame.len())),
                    "A damaged or cut-short packet".to_string(),
                ),
            };
        }
        _ => {
            return Desc::new("Unknown", "Not an IP packet".to_string(), "A packet Portlight cannot read".to_string());
        }
    };
    match h.link.net {
        link::Net::Ipv4 | link::Net::Ipv6 | link::Net::Arp => {
            let protocol = match h.link.net {
                link::Net::Ipv4 => "IPv4",
                link::Net::Ipv6 => "IPv6",
                _ => "ARP",
            };
            Desc::new(
                protocol,
                format!("{protocol} header cut short or damaged"),
                "A damaged or cut-short packet".to_string(),
            )
        }
        link::Net::Llc => {
            let dsap = u8_at(frame, h.link.net_at);
            if dsap == Some(0x42) {
                Desc::new(
                    "STP",
                    "Spanning tree (network switches)".to_string(),
                    "A network switch talks to other switches to keep the network free of loops".to_string(),
                )
            } else {
                Desc::new(
                    "LLC",
                    format!("802.3 frame, {} long", eth.ethertype),
                    format!("{} sends a network equipment frame", who.src),
                )
            }
        }
        link::Net::Other(t) => {
            let summary = match t {
                0x888e => format!("{} and the Wi-Fi access point check the network password or sign-in", who.src),
                0x88cc => format!("{} (network equipment) announces itself to its neighbours", who.src),
                _ => format!(
                    "{} sends a {} frame on the local network",
                    who.src,
                    names::ethertype(t).unwrap_or("non-IP")
                ),
            };
            Desc::new(
                names::ethertype(t).unwrap_or("Ethernet"),
                format!("EtherType {}", names::ethertype_value(t)),
                summary,
            )
        }
        link::Net::None => {
            Desc::new("Ethernet", "Ethernet frame cut short".to_string(), "A damaged or cut-short packet".to_string())
        }
    }
}
