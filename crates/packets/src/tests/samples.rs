//! Hand-built frames for tests, and a test [`Context`].

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use crate::Context;

pub const PC_MAC: [u8; 6] = [0x00, 0x1a, 0x2b, 0x3c, 0x4d, 0x5e];
pub const ROUTER_MAC: [u8; 6] = [0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff];
pub const PC: [u8; 4] = [192, 168, 1, 10];
pub const ROUTER: [u8; 4] = [192, 168, 1, 1];
pub const GITHUB: [u8; 4] = [140, 82, 112, 3];
pub const GOOGLE_DNS: [u8; 4] = [8, 8, 8, 8];

/// This PC is 192.168.1.10 and 2001:db8::10; 140.82.112.3 is github.com.
pub struct Ctx;

impl Context for Ctx {
    fn is_local(&self, ip: IpAddr) -> bool {
        ip == IpAddr::V4(Ipv4Addr::from(PC)) || ip == IpAddr::V6("2001:db8::10".parse().unwrap())
    }

    fn name_of(&self, ip: IpAddr) -> Option<String> {
        match ip {
            IpAddr::V4(v4) if v4.octets() == GITHUB => Some("github.com".to_string()),
            IpAddr::V4(v4) if v4.octets() == [142, 250, 1, 1] => Some("www.google.com".to_string()),
            _ => None,
        }
    }
}

pub fn eth(dst: [u8; 6], src: [u8; 6], ethertype: u16, payload: &[u8]) -> Vec<u8> {
    let mut f = Vec::new();
    f.extend_from_slice(&dst);
    f.extend_from_slice(&src);
    f.extend_from_slice(&ethertype.to_be_bytes());
    f.extend_from_slice(payload);
    // Ethernet's minimum frame size (without the checksum) is 60 bytes.
    while f.len() < 60 {
        f.push(0);
    }
    f
}

pub fn ipv4(src: [u8; 4], dst: [u8; 4], proto: u8, payload: &[u8]) -> Vec<u8> {
    let total = (20 + payload.len()) as u16;
    let mut p = vec![0x45, 0x00];
    p.extend_from_slice(&total.to_be_bytes());
    p.extend_from_slice(&[0x12, 0x34, 0x40, 0x00, 128, proto, 0, 0]);
    p.extend_from_slice(&src);
    p.extend_from_slice(&dst);
    let sum = checksum(&p);
    p[10..12].copy_from_slice(&sum.to_be_bytes());
    p.extend_from_slice(payload);
    p
}

fn checksum(b: &[u8]) -> u16 {
    let mut sum: u32 = b.chunks(2).map(|c| u32::from(u16::from_be_bytes([c[0], *c.get(1).unwrap_or(&0)]))).sum();
    while sum > 0xffff {
        sum = (sum & 0xffff) + (sum >> 16);
    }
    !(sum as u16)
}

/// IPv6 with an optional hop-by-hop options header in front of the payload.
pub fn ipv6(src: Ipv6Addr, dst: Ipv6Addr, next: u8, hop_by_hop: bool, payload: &[u8]) -> Vec<u8> {
    let mut body = Vec::new();
    let first_next = if hop_by_hop {
        // Next header, length 0 (8 bytes), router alert option, padding.
        body.extend_from_slice(&[next, 0, 5, 2, 0, 0, 1, 0]);
        0
    } else {
        next
    };
    body.extend_from_slice(payload);
    let mut p = vec![0x60, 0, 0, 0];
    p.extend_from_slice(&(body.len() as u16).to_be_bytes());
    p.push(first_next);
    p.push(64);
    p.extend_from_slice(&src.octets());
    p.extend_from_slice(&dst.octets());
    p.extend_from_slice(&body);
    p
}

#[allow(clippy::too_many_arguments)]
pub fn tcp(sp: u16, dp: u16, seq: u32, ack: u32, flags: u16, win: u16, options: &[u8], payload: &[u8]) -> Vec<u8> {
    assert_eq!(options.len() % 4, 0);
    let words = (20 + options.len()) / 4;
    let mut t = Vec::new();
    t.extend_from_slice(&sp.to_be_bytes());
    t.extend_from_slice(&dp.to_be_bytes());
    t.extend_from_slice(&seq.to_be_bytes());
    t.extend_from_slice(&ack.to_be_bytes());
    t.extend_from_slice(&(((words as u16) << 12) | flags).to_be_bytes());
    t.extend_from_slice(&win.to_be_bytes());
    t.extend_from_slice(&[0xab, 0xcd, 0, 0]);
    t.extend_from_slice(options);
    t.extend_from_slice(payload);
    t
}

pub fn udp(sp: u16, dp: u16, payload: &[u8]) -> Vec<u8> {
    let mut u = Vec::new();
    u.extend_from_slice(&sp.to_be_bytes());
    u.extend_from_slice(&dp.to_be_bytes());
    u.extend_from_slice(&((8 + payload.len()) as u16).to_be_bytes());
    u.extend_from_slice(&[0x12, 0x34]);
    u.extend_from_slice(payload);
    u
}

pub fn tcp4(src: [u8; 4], dst: [u8; 4], segment: &[u8]) -> Vec<u8> {
    let (dmac, smac) = if src == PC { (ROUTER_MAC, PC_MAC) } else { (PC_MAC, ROUTER_MAC) };
    eth(dmac, smac, 0x0800, &ipv4(src, dst, 6, segment))
}

pub fn udp4(src: [u8; 4], dst: [u8; 4], datagram: &[u8]) -> Vec<u8> {
    let (dmac, smac) = if src == PC { (ROUTER_MAC, PC_MAC) } else { (PC_MAC, ROUTER_MAC) };
    eth(dmac, smac, 0x0800, &ipv4(src, dst, 17, datagram))
}

/// MSS 1460, NOP, window scale 8, NOP, NOP, SACK permitted.
pub const SYN_OPTIONS: [u8; 12] = [2, 4, 0x05, 0xb4, 1, 3, 3, 8, 1, 1, 4, 2];

pub fn syn() -> Vec<u8> {
    tcp4(PC, GITHUB, &tcp(51234, 443, 0, 0, 0x002, 64240, &SYN_OPTIONS, &[]))
}

pub fn syn_ack() -> Vec<u8> {
    tcp4(GITHUB, PC, &tcp(443, 51234, 0, 1, 0x012, 65535, &SYN_OPTIONS, &[]))
}

pub fn ack() -> Vec<u8> {
    tcp4(PC, GITHUB, &tcp(51234, 443, 1, 1, 0x010, 1026, &[], &[]))
}

pub fn rst() -> Vec<u8> {
    tcp4(GITHUB, PC, &tcp(443, 51234, 1, 0, 0x004, 0, &[], &[]))
}

pub fn fin() -> Vec<u8> {
    tcp4(GITHUB, PC, &tcp(443, 51234, 5000, 900, 0x011, 501, &[], &[]))
}

fn dns_name(name: &str) -> Vec<u8> {
    let mut out = Vec::new();
    for label in name.split('.') {
        out.push(label.len() as u8);
        out.extend_from_slice(label.as_bytes());
    }
    out.push(0);
    out
}

/// A DNS question for github.com, type A.
pub fn dns_query_message() -> Vec<u8> {
    let mut m = vec![0x1a, 0x2b, 0x01, 0x00, 0, 1, 0, 0, 0, 0, 0, 0];
    m.extend(dns_name("github.com"));
    m.extend_from_slice(&[0, 1, 0, 1]);
    m
}

pub fn dns_query() -> Vec<u8> {
    udp4(PC, ROUTER, &udp(53000, 53, &dns_query_message()))
}

/// The answer, with the answer's name compressed to a pointer at the question (offset 12).
pub fn dns_response_message() -> Vec<u8> {
    let mut m = vec![0x1a, 0x2b, 0x81, 0x80, 0, 1, 0, 1, 0, 0, 0, 0];
    m.extend(dns_name("github.com"));
    m.extend_from_slice(&[0, 1, 0, 1]);
    m.extend_from_slice(&[0xc0, 0x0c, 0, 1, 0, 1, 0, 0, 0, 60, 0, 4]);
    m.extend_from_slice(&GITHUB);
    m
}

pub fn dns_response() -> Vec<u8> {
    udp4(ROUTER, PC, &udp(53, 53000, &dns_response_message()))
}

/// Offset of the DNS message in a [`udp4`] frame.
pub const UDP4_PAYLOAD: usize = 14 + 20 + 8;
/// Offset of the TCP payload in a [`tcp4`] frame without TCP options.
pub const TCP4_PAYLOAD: usize = 14 + 20 + 20;

fn ext(kind: u16, data: &[u8]) -> Vec<u8> {
    let mut e = kind.to_be_bytes().to_vec();
    e.extend_from_slice(&(data.len() as u16).to_be_bytes());
    e.extend_from_slice(data);
    e
}

/// A TLS record holding a ClientHello for `sni`, offering h2 and http/1.1, TLS 1.3 and 1.2.
pub fn client_hello_record(sni: &str) -> Vec<u8> {
    let mut sni_ext = Vec::new();
    sni_ext.extend_from_slice(&((sni.len() + 3) as u16).to_be_bytes());
    sni_ext.push(0);
    sni_ext.extend_from_slice(&(sni.len() as u16).to_be_bytes());
    sni_ext.extend_from_slice(sni.as_bytes());
    let alpn = [0, 12, 2, b'h', b'2', 8, b'h', b't', b't', b'p', b'/', b'1', b'.', b'1'];
    let versions = [6, 0x7a, 0x7a, 0x03, 0x04, 0x03, 0x03];
    let mut exts = Vec::new();
    exts.extend(ext(0, &sni_ext));
    exts.extend(ext(10, &[0, 4, 0, 0x1d, 0, 0x17]));
    exts.extend(ext(16, &alpn));
    exts.extend(ext(43, &versions));

    let mut body = vec![0x03, 0x03];
    body.extend_from_slice(&[0x11; 32]);
    body.push(32);
    body.extend_from_slice(&[0x22; 32]);
    body.extend_from_slice(&[0, 6, 0x13, 0x01, 0x13, 0x02, 0xc0, 0x2f]);
    body.extend_from_slice(&[1, 0]);
    body.extend_from_slice(&(exts.len() as u16).to_be_bytes());
    body.extend(exts);

    let mut hs = vec![1];
    hs.extend_from_slice(&(body.len() as u32).to_be_bytes()[1..]);
    hs.extend(body);
    let mut rec = vec![22, 3, 1];
    rec.extend_from_slice(&(hs.len() as u16).to_be_bytes());
    rec.extend(hs);
    rec
}

pub fn client_hello() -> Vec<u8> {
    tcp4(PC, GITHUB, &tcp(51234, 443, 1, 1, 0x018, 1026, &[], &client_hello_record("github.com")))
}

pub fn app_data() -> Vec<u8> {
    let mut rec = vec![23, 3, 3, 0x04, 0xd3];
    rec.extend(std::iter::repeat_n(0x5a, 1235));
    tcp4(PC, GITHUB, &tcp(51234, 443, 600, 5000, 0x018, 1026, &[], &rec))
}

pub const HTTP_GET: &[u8] = b"GET /index.html HTTP/1.1\r\nHost: example.com\r\nUser-Agent: test\r\n\r\n";

pub fn http_get() -> Vec<u8> {
    tcp4(PC, [93, 184, 216, 34], &tcp(51300, 80, 1, 1, 0x018, 1026, &[], HTTP_GET))
}

pub fn http_response() -> Vec<u8> {
    let body = b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n";
    tcp4([93, 184, 216, 34], PC, &tcp(80, 51300, 1, 70, 0x018, 501, &[], body))
}

pub fn arp_request() -> Vec<u8> {
    let mut a = vec![0, 1, 8, 0, 6, 4, 0, 1];
    a.extend_from_slice(&PC_MAC);
    a.extend_from_slice(&PC);
    a.extend_from_slice(&[0; 6]);
    a.extend_from_slice(&ROUTER);
    eth([0xff; 6], PC_MAC, 0x0806, &a)
}

pub fn ping() -> Vec<u8> {
    let mut i = vec![8, 0, 0xf7, 0xfe, 0, 1, 0, 1];
    i.extend_from_slice(b"abcdefghijklmnopqrstuvwabcdefghi");
    eth(ROUTER_MAC, PC_MAC, 0x0800, &ipv4(PC, GOOGLE_DNS, 1, &i))
}

pub fn ipv6_udp() -> Vec<u8> {
    let src: Ipv6Addr = "2001:db8::10".parse().unwrap();
    let dst: Ipv6Addr = "2606:4700::1111".parse().unwrap();
    eth(ROUTER_MAC, PC_MAC, 0x86dd, &ipv6(src, dst, 17, true, &udp(50000, 9999, b"hello over ipv6")))
}

pub fn quic_initial() -> Vec<u8> {
    let mut q = vec![0xc3, 0, 0, 0, 1, 8, 0x83, 0x94, 0xc8, 0xf0, 0x3e, 0x51, 0x57, 0x08, 0, 0];
    q.resize(1200, 0x33);
    udp4(PC, [142, 250, 1, 1], &udp(55000, 443, &q))
}

pub fn dhcp_discover() -> Vec<u8> {
    let mut d = vec![1, 1, 6, 0, 0x39, 0x03, 0xf3, 0x26, 0, 0, 0x80, 0];
    d.extend_from_slice(&[0; 16]);
    d.extend_from_slice(&PC_MAC);
    d.extend_from_slice(&[0; 10 + 64 + 128]);
    d.extend_from_slice(&[99, 130, 83, 99]);
    d.extend_from_slice(&[53, 1, 1]);
    d.extend_from_slice(&[50, 4, 192, 168, 1, 10]);
    d.extend_from_slice(&[12, 8]);
    d.extend_from_slice(b"percy-pc");
    d.extend_from_slice(&[55, 3, 1, 3, 6, 255]);
    eth([0xff; 6], PC_MAC, 0x0800, &ipv4([0, 0, 0, 0], [255, 255, 255, 255], 17, &udp(68, 67, &d)))
}

pub fn vlan_syn() -> Vec<u8> {
    let plain = syn();
    let mut f = plain[..12].to_vec();
    f.extend_from_slice(&[0x81, 0x00, 0x00, 0x64]);
    f.extend_from_slice(&plain[12..]);
    f
}

pub fn loopback_udp() -> Vec<u8> {
    let mut f = 2u32.to_le_bytes().to_vec();
    f.extend(ipv4([127, 0, 0, 1], [127, 0, 0, 1], 17, &udp(5000, 6000, b"ping")));
    f
}

pub fn fragment_tail() -> Vec<u8> {
    let mut ip = ipv4(PC, GOOGLE_DNS, 17, &[0x55; 100]);
    // Offset 185 × 8 = 1480 bytes, no more fragments.
    ip[6] = 0x00;
    ip[7] = 185;
    eth(ROUTER_MAC, PC_MAC, 0x0800, &ip)
}

/// Every sample, by name, with its link type.
pub fn all() -> Vec<(&'static str, crate::LinkType, Vec<u8>)> {
    use crate::LinkType::*;
    vec![
        ("syn", Ethernet, syn()),
        ("syn_ack", Ethernet, syn_ack()),
        ("ack", Ethernet, ack()),
        ("rst", Ethernet, rst()),
        ("fin", Ethernet, fin()),
        ("dns_query", Ethernet, dns_query()),
        ("dns_response", Ethernet, dns_response()),
        ("client_hello", Ethernet, client_hello()),
        ("app_data", Ethernet, app_data()),
        ("http_get", Ethernet, http_get()),
        ("http_response", Ethernet, http_response()),
        ("arp", Ethernet, arp_request()),
        ("ping", Ethernet, ping()),
        ("ipv6_udp", Ethernet, ipv6_udp()),
        ("quic", Ethernet, quic_initial()),
        ("dhcp", Ethernet, dhcp_discover()),
        ("vlan", Ethernet, vlan_syn()),
        ("loopback", Null, loopback_udp()),
        ("fragment", Ethernet, fragment_tail()),
        ("raw", Raw, syn()[14..].to_vec()),
    ]
}
