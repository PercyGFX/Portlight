//! Untrusted bytes: nothing may panic, loop forever or point outside the frame.

use super::samples::{self, Ctx};
use crate::filter::{Filter, PacketRef};
use crate::{Field, FrameInfo, LinkType, dissect, endpoints, summarize};

/// A tiny deterministic generator (xorshift64*).
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

const FILTERS: &[&str] = &[
    "tcp.port == 443 or udp.port == 53",
    "dns.qry.name contains a and not dns.flags.response",
    "tls.sni == github.com || http.host contains e",
    "host == github.com and ip.addr == 10.0.0.0/8",
    "dns.a == 1.2.3.4 or tls.handshake.type == 1 or http.response.code > 0",
    "eth.addr == ff:ff:ff:ff:ff:ff or arp or icmpv6.type == 135",
    "github",
    "app contains c",
    "quic or dhcp or ntp or ssdp or mdns or nbns or igmp",
];

fn check_fields(fields: &[Field], len: usize) {
    for f in fields {
        assert!(f.start + f.length <= len, "{} at {}+{} in {len} bytes", f.label, f.start, f.length);
        check_fields(&f.children, len);
    }
}

fn exercise(link: LinkType, frame: &[u8], filters: &[Filter]) {
    let _ = endpoints(link, frame);
    let row = summarize(link, frame, frame.len() as u32 + 10, &Ctx);
    assert!(!row.protocol.is_empty());
    let info = FrameInfo {
        number: 1,
        time: 0.0,
        timestamp_ms: 0.0,
        original_len: frame.len() as u32,
        interface_name: "test",
        app: None,
    };
    let layers = dissect(link, frame, &info, &Ctx);
    assert_eq!(layers[0].id, "frame");
    for layer in &layers {
        assert!(layer.start + layer.length <= frame.len(), "{} layer out of range", layer.id);
        check_fields(&layer.fields, frame.len());
    }
    let p = PacketRef { link, frame, original_len: frame.len() as u32, number: 1, app: Some("Chrome"), ctx: &Ctx };
    for f in filters {
        let _ = f.matches(&p);
    }
}

fn filters() -> Vec<Filter> {
    FILTERS.iter().map(|f| Filter::parse(f).unwrap()).collect()
}

#[test]
fn every_truncation_of_every_sample() {
    let filters = filters();
    for (_, link, frame) in samples::all() {
        for len in 0..=frame.len() {
            exercise(link, &frame[..len], &filters);
        }
    }
}

#[test]
fn corrupted_samples() {
    let filters = filters();
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    for (_, link, frame) in samples::all() {
        for _ in 0..300 {
            let mut f = frame.clone();
            for _ in 0..1 + rng.below(6) {
                let at = rng.below(f.len());
                f[at] = rng.next() as u8;
            }
            let cut = rng.below(f.len() + 1);
            exercise(link, &f, &filters);
            exercise(link, &f[..cut], &filters);
        }
    }
}

#[test]
fn random_bytes() {
    let filters = filters();
    let mut rng = Rng(42);
    let links = [LinkType::Ethernet, LinkType::Null, LinkType::Raw, LinkType::Unknown(147)];
    for i in 0..4000 {
        let len = rng.below(400);
        let mut f: Vec<u8> = (0..len).map(|_| rng.next() as u8).collect();
        // Give some frames a plausible start so deeper layers get exercised.
        if i % 2 == 0 && f.len() > 40 {
            let header = samples::syn();
            let keep = (14 + rng.below(40)).min(f.len());
            f[..keep].copy_from_slice(&header[..keep]);
        }
        exercise(links[i % links.len()], &f, &filters);
    }
}

#[test]
fn dns_compression_loops_end() {
    // A question whose name points at itself, and one pointing at each other in a cycle.
    let mut m = vec![0, 1, 0x81, 0x80, 0, 2, 0, 1, 0, 0, 0, 0];
    m.extend_from_slice(&[0xc0, 12, 0, 1, 0, 1]);
    m.extend_from_slice(&[3, b'a', b'b', b'c', 0xc0, 18, 0, 1, 0, 1]);
    m.extend_from_slice(&[0xc0, 18, 0, 1, 0, 1, 0, 0, 0, 1, 0, 4, 1, 2, 3, 4]);
    let frame = samples::udp4(samples::ROUTER, samples::PC, &samples::udp(53, 5000, &m));
    let row = summarize(LinkType::Ethernet, &frame, 0, &Ctx);
    assert_eq!(row.protocol, "DNS");
    exercise(LinkType::Ethernet, &frame, &filters());

    // Many pointers in a row, each jumping back one step.
    let mut long = vec![0, 1, 0x81, 0x80, 0, 1, 0, 0, 0, 0, 0, 0];
    for i in 0..200u16 {
        long.extend_from_slice(&(0xc000 | (12 + i * 2)).to_be_bytes());
    }
    let frame = samples::udp4(samples::ROUTER, samples::PC, &samples::udp(53, 5000, &long));
    exercise(LinkType::Ethernet, &frame, &filters());
}

#[test]
fn dns_name_sizes_are_capped() {
    // 200 labels of 63 bytes: far beyond the 255-byte limit.
    let mut m = vec![0, 1, 0x01, 0x00, 0, 1, 0, 0, 0, 0, 0, 0];
    for _ in 0..200 {
        m.push(63);
        m.extend_from_slice(&[b'x'; 63]);
    }
    m.push(0);
    m.extend_from_slice(&[0, 1, 0, 1]);
    let frame = samples::udp4(samples::PC, samples::ROUTER, &samples::udp(5000, 53, &m));
    exercise(LinkType::Ethernet, &frame, &filters());
}
