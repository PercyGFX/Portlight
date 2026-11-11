//! The display filter.

use super::samples::{self, Ctx};
use crate::LinkType;
use crate::filter::{Filter, FilterError, PacketRef};

fn packet<'a>(frame: &'a [u8], app: Option<&'a str>) -> PacketRef<'a> {
    PacketRef { link: LinkType::Ethernet, frame, original_len: frame.len() as u32, number: 3, app, ctx: &Ctx }
}

/// Which samples a filter matches, by name.
fn matching(text: &str) -> Vec<&'static str> {
    let filter = Filter::parse(text).unwrap_or_else(|e| panic!("{text}: {e}"));
    samples::all()
        .into_iter()
        .filter(|(name, link, frame)| {
            let app = (*name == "syn" || *name == "client_hello").then_some("Google Chrome");
            let p = PacketRef { link: *link, frame, original_len: frame.len() as u32, number: 3, app, ctx: &Ctx };
            filter.matches(&p)
        })
        .map(|(name, _, _)| name)
        .collect()
}

fn error(text: &str) -> FilterError {
    match Filter::parse(text) {
        Ok(_) => panic!("{text} parsed"),
        Err(e) => e,
    }
}

#[test]
fn empty_filter_matches_everything() {
    for text in ["", "   ", "\t"] {
        let f = Filter::parse(text).unwrap();
        assert!(f.is_empty());
        assert!(f.matches(&packet(&samples::syn(), None)));
    }
    assert!(!Filter::parse("tcp").unwrap().is_empty());
}

#[test]
fn protocols() {
    assert_eq!(matching("arp"), ["arp"]);
    assert_eq!(matching("dns"), ["dns_query", "dns_response"]);
    assert_eq!(matching("tls"), ["client_hello", "app_data"]);
    assert_eq!(matching("http"), ["http_get", "http_response"]);
    assert_eq!(matching("quic"), ["quic"]);
    assert_eq!(matching("dhcp"), ["dhcp"]);
    assert_eq!(matching("icmp"), ["ping"]);
    assert_eq!(matching("ipv6"), ["ipv6_udp"]);
    assert_eq!(matching("arp or icmp"), ["arp", "ping"]);
    assert_eq!(matching("udp and not dns and not ip"), ["ipv6_udp"]);
    assert_eq!(matching("TCP && !(tls || http)"), ["syn", "syn_ack", "ack", "rst", "fin", "vlan", "raw"]);
    assert!(matching("eth").contains(&"arp"));
    assert!(!matching("eth").contains(&"loopback"));
    assert_eq!(matching("mdns"), Vec::<&str>::new());
}

#[test]
fn numeric_fields() {
    assert_eq!(matching("tcp.port == 80"), ["http_get", "http_response"]);
    assert_eq!(matching("tcp.dstport eq 80"), ["http_get"]);
    assert_eq!(matching("udp.port == 0x35"), ["dns_query", "dns_response"]);
    assert_eq!(matching("port 443"), matching("tcp.port == 443 or udp.port == 443"));
    assert!(matching("port == 443").contains(&"quic"));
    assert_eq!(matching("frame.len > 1000"), ["app_data", "quic"]);
    assert_eq!(matching("frame.len ge 1294"), ["app_data"]);
    assert_eq!(matching("frame.number == 3").len(), samples::all().len());
    assert_eq!(matching("tcp.len > 0 and tcp.port == 443"), ["client_hello", "app_data"]);
    assert_eq!(matching("http.response.code >= 400"), ["http_response"]);
}

#[test]
fn address_fields() {
    assert_eq!(matching("ip.dst == 8.8.8.8"), ["ping", "fragment"]);
    assert!(matching("ip.addr == 140.82.112.0/24").contains(&"syn"));
    assert!(!matching("ip.addr != 140.82.112.3").contains(&"syn"));
    assert!(matching("ip.addr != 140.82.112.3").contains(&"dns_query"));
    // A field the packet does not have matches nothing, "!=" included.
    assert!(!matching("ip.addr != 140.82.112.3").contains(&"arp"));
    assert_eq!(matching("ipv6.addr == 2606:4700::/32"), ["ipv6_udp"]);
    assert_eq!(matching("ip.addr == 2001:db8::10"), ["ipv6_udp"]);
    assert_eq!(matching("eth.dst == ff:ff:ff:ff:ff:ff"), ["arp", "dhcp"]);
    assert!(matching("eth.addr == AA-BB-CC-DD-EE-FF").contains(&"syn"));
}

#[test]
fn flags_and_booleans() {
    assert_eq!(matching("tcp.flags.syn"), ["syn", "syn_ack", "vlan", "raw"]);
    assert_eq!(matching("tcp.flags.syn and not tcp.flags.ack"), ["syn", "vlan", "raw"]);
    assert_eq!(matching("tcp.flags.syn == 1 && tcp.flags.ack == 0"), ["syn", "vlan", "raw"]);
    assert_eq!(matching("tcp.flags.reset == true"), ["rst"]);
    assert_eq!(matching("tcp.flags.fin"), ["fin"]);
    assert_eq!(matching("dns.flags.response == 1"), ["dns_response"]);
    assert_eq!(matching("dns && !dns.flags.response"), ["dns_query"]);
}

#[test]
fn text_fields() {
    assert_eq!(matching("dns.qry.name == github.com"), ["dns_query", "dns_response"]);
    assert_eq!(matching("dns.qry.name == GitHub.com."), ["dns_query", "dns_response"]);
    assert_eq!(matching("dns.qry.name contains HUB"), ["dns_query", "dns_response"]);
    assert_eq!(matching("dns.a == 140.82.112.3"), ["dns_response"]);
    assert_eq!(matching("tls.sni == github.com"), ["client_hello"]);
    assert_eq!(matching("tls.handshake.extensions_server_name contains \"git\""), ["client_hello"]);
    assert_eq!(matching("http.request.method == GET"), ["http_get"]);
    assert_eq!(matching("http.host == 'example.com'"), ["http_get"]);
    assert_eq!(matching("http.request.uri contains index"), ["http_get"]);
}

#[test]
fn app_and_host() {
    assert_eq!(matching("app == \"google chrome\""), ["syn", "client_hello"]);
    assert_eq!(matching("app contains chrome"), ["syn", "client_hello"]);
    assert_eq!(matching("app chrome"), Vec::<&str>::new(), "shorthand means equals");
    assert_eq!(matching("app"), ["syn", "client_hello"]);
    // github.com is known by name for its address, and named in the TLS hello.
    let github = matching("host == github.com");
    for name in ["syn", "syn_ack", "ack", "rst", "fin", "client_hello", "app_data", "vlan", "raw"] {
        assert!(github.contains(&name), "{name} in {github:?}");
    }
    assert!(!github.contains(&"dns_query"));
    assert_eq!(matching("host example.com"), ["http_get"]);
    assert!(matching("host == 192.168.1.1").contains(&"arp"));
    assert!(matching("host contains google").contains(&"quic"));
}

#[test]
fn free_text_search() {
    // Bare words search source, destination, names, protocol, info, summary and app.
    let github = matching("github");
    assert!(github.contains(&"syn") && github.contains(&"dns_query"), "{github:?}");
    assert_eq!(matching("chrome"), ["syn", "client_hello"]);
    assert_eq!(matching("\"who has\""), ["arp"]);
    assert_eq!(matching("percy-pc"), ["dhcp"]);
    assert!(matching("github.com").contains(&"dns_response"), "a site name is a text search");
    assert!(matching("192.168.1.1").contains(&"arp"));
    assert_eq!(matching("ping 8.8.8.8"), ["ping"]);
    assert_eq!(matching("nothing-like-this"), Vec::<&str>::new());
}

#[test]
fn friendly_errors() {
    let e = error("tcp.prot == 443");
    assert_eq!(e.message, "\"tcp.prot\" is not a field I know. Did you mean tcp.port?");
    assert_eq!(e.position, 0);

    let e = error("tcp.prot");
    assert!(e.message.starts_with("\"tcp.prot\" is not a field I know. Did you mean tcp.port?"), "{}", e.message);
    assert!(e.message.contains("put it in quotes"));

    let e = error("tcp.port ==");
    assert_eq!(e.message, "Expected a value after \"==\".");
    assert_eq!(e.position, 11);

    let e = error("udp and (tcp or dns");
    assert_eq!((e.message.as_str(), e.position), ("Unclosed parenthesis.", 8));

    let e = error("tcp)");
    assert_eq!((e.message.as_str(), e.position), ("There is a \")\" without a matching \"(\".", 3));

    assert!(error("tcp.foo").message.contains("Fields for tcp include tcp.port"));
    assert!(error("ip.addr == github.com").message.contains("host == github.com"));
    assert!(error("tcp.port == abc").message.starts_with("\"abc\" is not a number."));
    assert_eq!(error("app == \"chrome").message, "This quote is never closed.");
    assert_eq!(error("app == \"chrome").position, 7);
    assert!(error("tcp.flags.syn == maybe").message.contains("yes/no"));
    assert_eq!(error("tcp and").message, "Expected something after \"and\".");
    assert!(error("ip.addr > 1.2.3.4").message.contains("only compares numbers"));
    assert!(error("tcp == 1").message.contains("is a protocol, not a field"));
    assert!(error("tcp.port contains 4").message.contains("works with text"));
    assert!(error("== 4").message.starts_with("Expected a field name"));
    assert!(error("tcp & udp").message.contains("&&"));
    assert!(error("()").message.contains("nothing inside"));
    assert!(error(&"(".repeat(200)).message.contains("nested too deeply"));
    assert!(error("ip.addr == 10.0.0.0/33").message.contains("not an IP address"));
    assert!(error("eth.src == zz").message.contains("MAC address"));
    assert_eq!(error("tcp.port == 443 )").to_string(), "There is a \")\" without a matching \"(\".");
}

#[test]
fn long_filters_do_not_nest() {
    let long = vec!["tcp"; 50_000].join(" or ");
    let f = Filter::parse(&long).unwrap();
    assert!(f.matches(&packet(&samples::syn(), None)));
    let words = vec!["github"; 50_000].join(" ");
    assert!(Filter::parse(&words).unwrap().matches(&packet(&samples::syn(), None)));
}
