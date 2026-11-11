//! endpoints, summarize and dissect.

use std::net::IpAddr;

use super::samples::{self, Ctx};
use crate::{Direction, Field, FrameInfo, Layer, LinkType, Summary, Transport, dissect, endpoints, summarize};

fn row(frame: &[u8]) -> Summary {
    summarize(LinkType::Ethernet, frame, 0, &Ctx)
}

fn layers(frame: &[u8]) -> Vec<Layer> {
    let info = FrameInfo {
        number: 7,
        time: 1.5,
        timestamp_ms: 1_762_943_465_123.0,
        original_len: frame.len() as u32,
        interface_name: "Wi-Fi",
        app: Some("Google Chrome"),
    };
    dissect(LinkType::Ethernet, frame, &info, &Ctx)
}

fn layer<'a>(layers: &'a [Layer], id: &str) -> &'a Layer {
    layers.iter().find(|l| l.id == id).unwrap_or_else(|| panic!("no {id} layer in {:?}", ids(layers)))
}

fn ids(layers: &[Layer]) -> Vec<&str> {
    layers.iter().map(|l| l.id.as_str()).collect()
}

/// Finds a field by label anywhere below `fields`, depth first.
fn find<'a>(fields: &'a [Field], label: &str) -> Option<&'a Field> {
    for f in fields {
        if f.label == label {
            return Some(f);
        }
        if let Some(found) = find(&f.children, label) {
            return Some(found);
        }
    }
    None
}

fn ip(s: &str) -> IpAddr {
    s.parse().unwrap()
}

#[test]
fn link_types_from_dlt() {
    assert_eq!(LinkType::from_dlt(1), LinkType::Ethernet);
    assert_eq!(LinkType::from_dlt(0), LinkType::Null);
    for raw in [12, 14, 101, 228, 229] {
        assert_eq!(LinkType::from_dlt(raw), LinkType::Raw);
    }
    assert_eq!(LinkType::from_dlt(147), LinkType::Unknown(147));
    assert_eq!(LinkType::Raw.dlt(), 101);
    assert_eq!(LinkType::Unknown(147).dlt(), 147);
}

#[test]
fn endpoints_of_common_frames() {
    let e = endpoints(LinkType::Ethernet, &samples::syn()).unwrap();
    assert_eq!(e.transport, Transport::Tcp);
    assert_eq!((e.src, e.dst), (ip("192.168.1.10"), ip("140.82.112.3")));
    assert_eq!((e.src_port, e.dst_port), (Some(51234), Some(443)));

    let vlan = endpoints(LinkType::Ethernet, &samples::vlan_syn()).unwrap();
    assert_eq!(vlan, e);

    let raw = endpoints(LinkType::Raw, &samples::syn()[14..]).unwrap();
    assert_eq!(raw, e);

    let lo = endpoints(LinkType::Null, &samples::loopback_udp()).unwrap();
    assert_eq!(lo.transport, Transport::Udp);
    assert_eq!((lo.src_port, lo.dst_port), (Some(5000), Some(6000)));

    let v6 = endpoints(LinkType::Ethernet, &samples::ipv6_udp()).unwrap();
    assert_eq!(v6.transport, Transport::Udp, "walks the hop-by-hop header");
    assert_eq!((v6.src, v6.dst_port), (ip("2001:db8::10"), Some(9999)));

    let ping = endpoints(LinkType::Ethernet, &samples::ping()).unwrap();
    assert_eq!((ping.transport, ping.src_port), (Transport::Icmp, None));

    let tail = endpoints(LinkType::Ethernet, &samples::fragment_tail()).unwrap();
    assert_eq!((tail.transport, tail.src_port, tail.dst_port), (Transport::Udp, None, None));

    assert_eq!(endpoints(LinkType::Ethernet, &samples::arp_request()), None);
    assert_eq!(endpoints(LinkType::Unknown(147), &samples::syn()), None);
}

#[test]
fn endpoints_respect_ip_header_length() {
    // An IPv4 header with 4 bytes of options (IHL 6).
    let mut f = samples::syn();
    f[14] = 0x46;
    f.splice(34..34, [1, 1, 1, 0]);
    let total = u16::from_be_bytes([f[16], f[17]]) + 4;
    f[16..18].copy_from_slice(&total.to_be_bytes());
    let e = endpoints(LinkType::Ethernet, &f).unwrap();
    assert_eq!((e.src_port, e.dst_port), (Some(51234), Some(443)));
}

#[test]
fn tcp_handshake_rows() {
    let syn = row(&samples::syn());
    assert_eq!(syn.protocol, "TCP");
    assert_eq!(syn.info, "51234 → 443 [SYN] Seq=0 Win=64240 Len=0 MSS=1460 WS=256 SACK_PERM");
    assert_eq!(syn.summary, "This PC asks github.com to open a connection");
    assert_eq!(syn.direction, Some(Direction::Out));
    assert_eq!(syn.source_name.as_deref(), Some("This PC"));
    assert_eq!(syn.destination_name.as_deref(), Some("github.com"));
    assert_eq!((syn.source_port, syn.destination_port), (Some(51234), Some(443)));
    assert_eq!(syn.length, 66);

    let syn_ack = row(&samples::syn_ack());
    assert_eq!(syn_ack.info, "443 → 51234 [SYN, ACK] Seq=0 Ack=1 Win=65535 Len=0 MSS=1460 WS=256 SACK_PERM");
    assert_eq!(syn_ack.summary, "github.com agrees to the connection");
    assert_eq!(syn_ack.direction, Some(Direction::In));

    assert_eq!(row(&samples::ack()).summary, "This PC confirms to github.com that everything arrived");

    let rst = row(&samples::rst());
    assert_eq!(rst.info, "443 → 51234 [RST] Seq=1 Win=0 Len=0");
    assert_eq!(rst.summary, "Connection reset — github.com gave up abruptly");

    let fin = row(&samples::fin());
    assert!(fin.info.contains("[FIN, ACK]"), "{}", fin.info);
    assert_eq!(fin.summary, "github.com ends the connection");
}

#[test]
fn dns_rows() {
    let q = row(&samples::dns_query());
    assert_eq!(q.protocol, "DNS");
    assert_eq!(q.info, "Standard query 0x1a2b A github.com");
    assert_eq!(q.summary, "This PC asks the DNS server for the address of github.com");

    let r = row(&samples::dns_response());
    assert_eq!(r.info, "Standard query response 0x1a2b A github.com A 140.82.112.3");
    assert_eq!(r.summary, "The DNS server answers: github.com is at 140.82.112.3");
    assert_eq!(r.direction, Some(Direction::In));
}

#[test]
fn tls_rows() {
    let hello = row(&samples::client_hello());
    assert_eq!(hello.protocol, "TLS");
    assert_eq!(hello.info, "Client Hello (SNI=github.com)");
    assert_eq!(hello.summary, "This PC starts a secure (TLS 1.3) session with github.com");

    let data = row(&samples::app_data());
    assert_eq!(data.info, "Application Data");
    assert_eq!(data.summary, "This PC sends 1,240 bytes of encrypted data to github.com");

    // A segment from the middle of a record.
    let middle =
        samples::tcp4(samples::GITHUB, samples::PC, &samples::tcp(443, 51234, 9, 9, 0x010, 501, &[], &[0x42; 300]));
    let cont = row(&middle);
    assert_eq!((cont.protocol.as_str(), cont.info.as_str()), ("TLS", "Continuation Data"));
    assert_eq!(cont.summary, "github.com sends 300 bytes of encrypted data to This PC");
}

#[test]
fn http_rows() {
    let get = row(&samples::http_get());
    assert_eq!(get.protocol, "HTTP");
    assert_eq!(get.info, "GET /index.html HTTP/1.1");
    assert_eq!(get.summary, "This PC asks example.com for /index.html");

    let resp = row(&samples::http_response());
    assert_eq!(resp.info, "HTTP/1.1 404 Not Found");
    assert_eq!(resp.summary, "93.184.216.34 answers: 404 Not Found");
}

#[test]
fn other_protocol_rows() {
    let arp = row(&samples::arp_request());
    assert_eq!(arp.protocol, "ARP");
    assert_eq!(arp.info, "Who has 192.168.1.1? Tell 192.168.1.10");
    assert_eq!(arp.summary, "This PC asks who on the local network has 192.168.1.1");
    assert_eq!(arp.source, "00:1a:2b:3c:4d:5e");
    assert_eq!(arp.destination_name.as_deref(), Some("Broadcast"));
    assert_eq!(arp.direction, Some(Direction::Out));

    let ping = row(&samples::ping());
    assert_eq!(ping.protocol, "ICMP");
    assert_eq!(ping.info, "Echo (ping) request id=0x0001, seq=1");
    assert_eq!(ping.summary, "This PC pings 8.8.8.8");

    let v6 = row(&samples::ipv6_udp());
    assert_eq!(v6.protocol, "UDP");
    assert_eq!(v6.info, "50000 → 9999 Len=15");
    assert_eq!(v6.summary, "This PC sends 15 bytes to 2606:4700::1111 over UDP");
    assert_eq!(v6.source, "2001:db8::10");

    let quic = row(&samples::quic_initial());
    assert_eq!(quic.protocol, "QUIC");
    assert_eq!(quic.info, "Initial, DCID=8394c8f03e515708");
    assert_eq!(quic.summary, "This PC starts a QUIC (HTTP/3) connection with www.google.com");

    let dhcp = row(&samples::dhcp_discover());
    assert_eq!(dhcp.protocol, "DHCP");
    assert_eq!(dhcp.info, "DHCP Discover - Transaction ID 0x3903f326");
    assert_eq!(dhcp.summary, "percy-pc looks for a DHCP server to get an address");
    assert_eq!(dhcp.destination_name.as_deref(), Some("Broadcast"));

    let frag = row(&samples::fragment_tail());
    assert_eq!(frag.protocol, "IPv4");
    assert_eq!(frag.info, "Fragmented IP protocol (proto=UDP 17, off=1480, ID=1234)");

    let lo = summarize(LinkType::Null, &samples::loopback_udp(), 0, &Ctx);
    assert_eq!((lo.protocol.as_str(), lo.direction), ("UDP", None));

    let unknown = summarize(LinkType::Unknown(147), &[1, 2, 3], 3, &Ctx);
    assert_eq!(unknown.protocol, "Unknown");
}

#[test]
fn dissect_layers_and_offsets() {
    let frame = samples::syn();
    let ls = layers(&frame);
    assert_eq!(ids(&ls), ["frame", "eth", "ip", "tcp"]);
    let frame_layer = layer(&ls, "frame");
    assert_eq!(frame_layer.title, "Frame 7");
    assert_eq!(frame_layer.summary, "This PC asks github.com to open a connection");
    assert_eq!(find(&frame_layer.fields, "App").unwrap().value, "Google Chrome");
    assert_eq!(find(&frame_layer.fields, "Protocols in frame").unwrap().value, "eth:ip:tcp");
    assert_eq!(find(&frame_layer.fields, "Arrival time").unwrap().value, "2025-11-12 10:31:05.123000 UTC");

    let ip = layer(&ls, "ip");
    assert_eq!((ip.start, ip.length), (14, 20));
    let ttl = find(&ip.fields, "Time to live").unwrap();
    assert_eq!((ttl.value.as_str(), ttl.start, ttl.length), ("128 (hops left before the packet is dropped)", 22, 1));
    assert!(ttl.explain.is_some());
    let dst = find(&ip.fields, "Destination address").unwrap();
    assert_eq!(&frame[dst.start..dst.start + dst.length], &samples::GITHUB);

    let tcp = layer(&ls, "tcp");
    assert_eq!((tcp.start, tcp.length), (34, 32));
    let port = find(&tcp.fields, "Destination port").unwrap();
    assert_eq!((port.value.as_str(), port.start, port.length), ("443 (HTTPS)", 36, 2));
    let flags = find(&tcp.fields, "Flags").unwrap();
    assert_eq!((flags.value.as_str(), flags.start, flags.length), ("0x002 (SYN)", 46, 2));
    let syn = flags.children.iter().find(|c| c.label == "SYN").unwrap();
    assert_eq!((syn.value.as_str(), syn.start, syn.length), ("Set", 47, 1));
    let mss = find(&tcp.fields, "Maximum segment size").unwrap();
    assert_eq!((mss.value.as_str(), mss.start, mss.length), ("1,460 bytes", 54, 4));
    assert_eq!(find(&tcp.fields, "Window scale").unwrap().value, "8 (multiply by 256)");
}

#[test]
fn dissect_points_at_the_server_name() {
    let frame = samples::client_hello();
    let ls = layers(&frame);
    assert_eq!(ids(&ls), ["frame", "eth", "ip", "tcp", "tls"]);
    let tls = layer(&ls, "tls");
    assert_eq!(tls.start, samples::TCP4_PAYLOAD);
    assert_eq!(tls.start + tls.length, frame.len());
    let sni = find(&tls.fields, "Server name").unwrap();
    assert_eq!(sni.value, "github.com");
    assert_eq!(&frame[sni.start..sni.start + sni.length], b"github.com");
    assert!(sni.explain.is_some());
    let alpn = find(&tls.fields, "Extension: application_layer_protocol_negotiation").unwrap();
    assert_eq!(alpn.value, "h2, http/1.1");
    let h2 = &alpn.children[0];
    assert_eq!(&frame[h2.start..h2.start + h2.length], b"h2");
    let versions = find(&tls.fields, "Extension: supported_versions").unwrap();
    assert_eq!(versions.value, "reserved (GREASE), TLS 1.3, TLS 1.2");

    let data = layers(&samples::app_data());
    let enc = find(&layer(&data, "tls").fields, "Encrypted data").unwrap();
    assert_eq!(enc.value, "1,235 bytes — the contents are private");
}

#[test]
fn dissect_dns_names_with_compression() {
    let frame = samples::dns_response();
    let ls = layers(&frame);
    assert_eq!(ids(&ls), ["frame", "eth", "ip", "udp", "dns"]);
    let dns = layer(&ls, "dns");
    let answers = find(&dns.fields, "Answers").unwrap();
    let record = &answers.children[0];
    let name = find(&record.children, "Name").unwrap();
    assert_eq!(name.value, "github.com");
    // The answer's name is a 2-byte pointer.
    assert_eq!(name.length, 2);
    assert_eq!(frame[name.start], 0xc0);
    let addr = find(&record.children, "Address").unwrap();
    assert_eq!(&frame[addr.start..addr.start + addr.length], &samples::GITHUB);
    let question = find(&dns.fields, "Question").unwrap();
    let qname = find(&question.children, "Name").unwrap();
    assert_eq!(qname.start, samples::UDP4_PAYLOAD + 12);
    assert_eq!(qname.length, 12);
    assert_eq!(find(&record.children, "Time to live").unwrap().value, "60 (1 minute)");
}

#[test]
fn dissect_other_frames() {
    let arp = layers(&samples::arp_request());
    assert_eq!(ids(&arp), ["frame", "eth", "arp"]);
    let eth = layer(&arp, "eth");
    let pad = find(&eth.fields, "Padding").unwrap();
    assert_eq!((pad.start, pad.length), (42, 18));

    let http = layers(&samples::http_get());
    let h = layer(&http, "http");
    let host = find(&h.fields, "Host").unwrap();
    assert_eq!(host.value, "example.com");
    let frame = samples::http_get();
    assert_eq!(&frame[host.start..host.start + host.length], b"Host: example.com");
    assert_eq!(find(&h.fields, "Method").unwrap().value, "GET");

    assert_eq!(ids(&layers(&samples::ping())), ["frame", "eth", "ip", "icmp"]);
    assert_eq!(ids(&layers(&samples::ipv6_udp())), ["frame", "eth", "ipv6", "udp", "data"]);
    assert_eq!(ids(&layers(&samples::quic_initial())), ["frame", "eth", "ip", "udp", "quic"]);
    let dhcp = layers(&samples::dhcp_discover());
    let d = layer(&dhcp, "dhcp");
    assert_eq!(find(&d.fields, "Host name").unwrap().value, "percy-pc");
    assert_eq!(find(&d.fields, "DHCP message type").unwrap().value, "1 (Discover)");
    assert_eq!(find(&d.fields, "Requested IP address").unwrap().value, "192.168.1.10");

    let vlan = layers(&samples::vlan_syn());
    let tag = find(&layer(&vlan, "eth").fields, "VLAN tag").unwrap();
    assert_eq!((tag.value.as_str(), tag.start, tag.length), ("VLAN 100, priority 0", 12, 4));
    assert_eq!(layer(&vlan, "ip").start, 18);

    let ipv6 = layers(&samples::ipv6_udp());
    let ext = find(&layer(&ipv6, "ipv6").fields, "Hop-by-hop options").unwrap();
    assert_eq!((ext.start, ext.length), (54, 8));
    let data = layer(&ipv6, "data");
    assert_eq!(find(&data.fields, "Text").unwrap().value, "hello over ipv6");
}

#[test]
fn json_shapes_are_camel_case() {
    let summary = serde_json::to_value(row(&samples::syn())).unwrap();
    let mut keys: Vec<&str> = summary.as_object().unwrap().keys().map(|k| k.as_str()).collect();
    keys.sort();
    assert_eq!(
        keys,
        [
            "destination",
            "destinationName",
            "destinationPort",
            "direction",
            "info",
            "length",
            "protocol",
            "source",
            "sourceName",
            "sourcePort",
            "summary"
        ]
    );
    assert_eq!(summary["direction"], "out");
    assert_eq!(summary["destinationPort"], 443);

    let ls = serde_json::to_value(layers(&samples::syn())).unwrap();
    let first = ls[1].as_object().unwrap();
    let mut keys: Vec<&str> = first.keys().map(|k| k.as_str()).collect();
    keys.sort();
    assert_eq!(keys, ["explain", "fields", "id", "length", "start", "summary", "title"]);
    let field = ls[1]["fields"][0].as_object().unwrap();
    let mut keys: Vec<&str> = field.keys().map(|k| k.as_str()).collect();
    keys.sort();
    assert_eq!(keys, ["children", "explain", "label", "length", "start", "value"]);

    let unknown = serde_json::to_value(summarize(LinkType::Unknown(9), &[], 0, &Ctx)).unwrap();
    assert!(unknown["direction"].is_null());
}

#[test]
fn more_protocol_rows() {
    // DNS over TCP: a 2-byte length, then the message.
    let msg = samples::dns_query_message();
    let mut payload = (msg.len() as u16).to_be_bytes().to_vec();
    payload.extend_from_slice(&msg);
    let tcp_dns =
        samples::tcp4(samples::PC, samples::ROUTER, &samples::tcp(53001, 53, 1, 1, 0x018, 1026, &[], &payload));
    let r = row(&tcp_dns);
    assert_eq!((r.protocol.as_str(), r.info.as_str()), ("DNS", "Standard query 0x1a2b A github.com"));
    let ls = layers(&tcp_dns);
    let len = find(&layer(&ls, "dns").fields, "Length").unwrap();
    assert_eq!((len.start, len.length), (samples::TCP4_PAYLOAD, 2));

    let mdns = samples::udp4(samples::PC, [224, 0, 0, 251], &samples::udp(5353, 5353, &msg));
    let r = row(&mdns);
    assert_eq!(r.protocol, "MDNS");
    assert_eq!(r.summary, "This PC asks devices on the local network for the address of github.com");

    let mut ntp = vec![0x23];
    ntp.resize(48, 0);
    let r = row(&samples::udp4(samples::PC, [17, 253, 34, 123], &samples::udp(50123, 123, &ntp)));
    assert_eq!((r.protocol.as_str(), r.info.as_str()), ("NTP", "NTP Version 4, client"));
    assert_eq!(r.summary, "This PC asks 17.253.34.123 for the current time");

    let search = b"M-SEARCH * HTTP/1.1\r\nHOST: 239.255.255.250:1900\r\nST: ssdp:all\r\n\r\n";
    let r = row(&samples::udp4(samples::PC, [239, 255, 255, 250], &samples::udp(50000, 1900, search)));
    assert_eq!((r.protocol.as_str(), r.info.as_str()), ("SSDP", "M-SEARCH * HTTP/1.1"));
    assert!(r.summary.starts_with("This PC looks for devices on the local network"));

    // Neighbor solicitation for fe80::1 with a source link-layer address option.
    let mut ns = vec![135, 0, 0, 0, 0, 0, 0, 0];
    ns.extend_from_slice(&"fe80::1".parse::<std::net::Ipv6Addr>().unwrap().octets());
    ns.extend_from_slice(&[1, 1]);
    ns.extend_from_slice(&samples::PC_MAC);
    let ip6 = samples::ipv6("2001:db8::10".parse().unwrap(), "ff02::1:ff00:1".parse().unwrap(), 58, false, &ns);
    let r = row(&samples::eth([0x33, 0x33, 0xff, 0, 0, 1], samples::PC_MAC, 0x86dd, &ip6));
    assert_eq!(r.protocol, "ICMPv6");
    assert_eq!(r.info, "Neighbor Solicitation for fe80::1 from 00:1a:2b:3c:4d:5e");
    assert_eq!(r.summary, "This PC asks who on the local network has fe80::1");
}
