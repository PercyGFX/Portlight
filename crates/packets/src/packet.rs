//! One pass over a frame's headers, without allocating: link, network,
//! transport, and a guess at the application protocol from ports and the
//! first payload bytes. Everything else builds on this.

use crate::arp::{self, Arp};
use crate::bytes::{range, u16_at};
use crate::icmp::{self, Icmp};
use crate::ip::{self, Ip};
use crate::link::{self, Link};
use crate::tcp::{self, Tcp};
use crate::udp::{self, Udp};
use crate::{Endpoints, LinkType, Transport, http, tls};

#[derive(Debug, Clone, Copy)]
pub(crate) enum Net {
    Ip(Ip),
    Arp(Arp),
    None,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum L4 {
    Tcp(Tcp),
    Udp(Udp),
    Icmp(Icmp),
    Igmp { at: usize, end: usize },
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DnsKind {
    Dns,
    Mdns,
    Llmnr,
}

/// The application protocol a payload most likely carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum App {
    None,
    Dns(DnsKind),
    Tls,
    Http,
    Quic,
    Dhcp,
    Ntp,
    Ssdp,
    Nbns,
    Nbds,
}

/// Where the application data of a TCP or UDP packet is.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Payload {
    pub at: usize,
    pub end: usize,
    pub tcp: bool,
    pub dst_port: u16,
}

impl Payload {
    pub(crate) fn len(&self) -> usize {
        self.end.saturating_sub(self.at)
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Headers {
    pub link_type: LinkType,
    pub link: Link,
    pub net: Net,
    pub l4: L4,
    pub app: App,
}

impl Headers {
    pub(crate) fn parse(link_type: LinkType, frame: &[u8]) -> Headers {
        let link = link::parse(link_type, frame);
        let net = match link.net {
            link::Net::Ipv4 => ip::parse_v4(frame, link.net_at).map_or(Net::None, Net::Ip),
            link::Net::Ipv6 => ip::parse_v6(frame, link.net_at).map_or(Net::None, Net::Ip),
            link::Net::Arp => arp::parse(frame, link.net_at).map_or(Net::None, Net::Arp),
            _ => Net::None,
        };
        let l4 = match &net {
            Net::Ip(ip) if ip.first_fragment() => transport(frame, ip),
            _ => L4::None,
        };
        let app = match &l4 {
            L4::Tcp(t) => classify_tcp(frame, t),
            L4::Udp(u) => classify_udp(frame, u),
            _ => App::None,
        };
        Headers { link_type, link, net, l4, app }
    }

    pub(crate) fn ip(&self) -> Option<&Ip> {
        match &self.net {
            Net::Ip(ip) => Some(ip),
            _ => None,
        }
    }

    /// The TCP or UDP data, for the application protocols.
    pub(crate) fn payload_info(&self) -> Option<Payload> {
        match &self.l4 {
            L4::Tcp(t) => Some(Payload { at: t.payload_at, end: t.end, tcp: true, dst_port: t.dst_port }),
            L4::Udp(u) => Some(Payload { at: u.payload_at, end: u.end, tcp: false, dst_port: u.dst_port }),
            _ => None,
        }
    }

    pub(crate) fn ports(&self) -> Option<(u16, u16)> {
        match &self.l4 {
            L4::Tcp(t) => Some((t.src_port, t.dst_port)),
            L4::Udp(u) => Some((u.src_port, u.dst_port)),
            _ => None,
        }
    }
}

fn transport(frame: &[u8], ip: &Ip) -> L4 {
    let (at, end) = (ip.payload_at, ip.end);
    let parsed = match ip.proto {
        6 => tcp::parse(frame, at, end).map(L4::Tcp),
        17 => udp::parse(frame, at, end).map(L4::Udp),
        1 if !ip.v6 => icmp::parse(frame, at, end, false).map(L4::Icmp),
        58 if ip.v6 => icmp::parse(frame, at, end, true).map(L4::Icmp),
        2 if !ip.v6 && at < end => Some(L4::Igmp { at, end }),
        _ => None,
    };
    parsed.unwrap_or(L4::None)
}

fn classify_tcp(frame: &[u8], t: &Tcp) -> App {
    let payload = range(frame, t.payload_at, t.end);
    if payload.is_empty() {
        return App::None;
    }
    if t.has_port(53) {
        return App::Dns(DnsKind::Dns);
    }
    if tls::looks_like_record(payload) {
        return App::Tls;
    }
    if http::looks_like_start(payload) {
        return App::Http;
    }
    for port in ordered(t.src_port, t.dst_port) {
        match port {
            443 | 8443 => return App::Tls,
            80 | 8080 => return App::Http,
            _ => {}
        }
    }
    App::None
}

fn classify_udp(frame: &[u8], u: &Udp) -> App {
    if range(frame, u.payload_at, u.end).is_empty() {
        return App::None;
    }
    for port in ordered(u.src_port, u.dst_port) {
        let app = match port {
            53 => App::Dns(DnsKind::Dns),
            5353 => App::Dns(DnsKind::Mdns),
            5355 => App::Dns(DnsKind::Llmnr),
            67 | 68 => App::Dhcp,
            123 => App::Ntp,
            1900 => App::Ssdp,
            137 => App::Nbns,
            138 => App::Nbds,
            443 => App::Quic,
            _ => continue,
        };
        return app;
    }
    App::None
}

/// The lower port first: servers usually have the lower number.
fn ordered(a: u16, b: u16) -> [u16; 2] {
    if a <= b { [a, b] } else { [b, a] }
}

/// The IP addresses and ports of a frame, without decoding anything else.
pub fn endpoints(link: LinkType, frame: &[u8]) -> Option<Endpoints> {
    let link = link::parse(link, frame);
    let ip = match link.net {
        link::Net::Ipv4 => ip::parse_v4(frame, link.net_at)?,
        link::Net::Ipv6 => ip::parse_v6(frame, link.net_at)?,
        _ => return None,
    };
    let transport = match ip.proto {
        6 => Transport::Tcp,
        17 => Transport::Udp,
        1 | 58 => Transport::Icmp,
        p => Transport::Other(p),
    };
    let mut ends = Endpoints { transport, src: ip.src, dst: ip.dst, src_port: None, dst_port: None };
    if matches!(transport, Transport::Tcp | Transport::Udp) && ip.first_fragment() {
        let b = frame.get(..ip.end)?;
        ends.src_port = u16_at(b, ip.payload_at);
        ends.dst_port = u16_at(b, ip.payload_at + 2);
        if ends.dst_port.is_none() {
            ends.src_port = None;
        }
    }
    Some(ends)
}
