//! The protocol and field names the filter knows.

/// What kind of value a field holds, which decides how it is compared.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Num,
    Ip,
    Mac,
    Bool,
    Text,
    /// Addresses and names together (`host`).
    Host,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FieldId {
    FrameNumber,
    FrameLen,
    FrameCapLen,
    IpSrc,
    IpDst,
    IpAddr,
    IpTtl,
    IpProto,
    Ipv6Src,
    Ipv6Dst,
    Ipv6Addr,
    EthSrc,
    EthDst,
    EthAddr,
    TcpPort,
    TcpSrcPort,
    TcpDstPort,
    TcpLen,
    TcpSeq,
    TcpAck,
    TcpFlags,
    TcpWindow,
    TcpSyn,
    TcpAckFlag,
    TcpFin,
    TcpRst,
    TcpPush,
    UdpPort,
    UdpSrcPort,
    UdpDstPort,
    UdpLength,
    IcmpType,
    IcmpCode,
    Icmpv6Type,
    DnsQryName,
    DnsQryType,
    DnsResponse,
    DnsId,
    DnsA,
    TlsSni,
    TlsHandshakeType,
    HttpHost,
    HttpMethod,
    HttpUri,
    HttpCode,
    App,
    Port,
    Host,
}

use FieldId::*;
use Kind::*;

pub(crate) const FIELDS: &[(&str, FieldId, Kind)] = &[
    ("frame.number", FrameNumber, Num),
    ("frame.len", FrameLen, Num),
    ("frame.cap_len", FrameCapLen, Num),
    ("ip.src", IpSrc, Ip),
    ("ip.dst", IpDst, Ip),
    ("ip.addr", IpAddr, Ip),
    ("ip.ttl", IpTtl, Num),
    ("ip.proto", IpProto, Num),
    ("ipv6.src", Ipv6Src, Ip),
    ("ipv6.dst", Ipv6Dst, Ip),
    ("ipv6.addr", Ipv6Addr, Ip),
    ("eth.src", EthSrc, Mac),
    ("eth.dst", EthDst, Mac),
    ("eth.addr", EthAddr, Mac),
    ("tcp.port", TcpPort, Num),
    ("tcp.srcport", TcpSrcPort, Num),
    ("tcp.dstport", TcpDstPort, Num),
    ("tcp.len", TcpLen, Num),
    ("tcp.seq", TcpSeq, Num),
    ("tcp.ack", TcpAck, Num),
    ("tcp.flags", TcpFlags, Num),
    ("tcp.window_size", TcpWindow, Num),
    ("tcp.flags.syn", TcpSyn, Bool),
    ("tcp.flags.ack", TcpAckFlag, Bool),
    ("tcp.flags.fin", TcpFin, Bool),
    ("tcp.flags.reset", TcpRst, Bool),
    ("tcp.flags.push", TcpPush, Bool),
    ("udp.port", UdpPort, Num),
    ("udp.srcport", UdpSrcPort, Num),
    ("udp.dstport", UdpDstPort, Num),
    ("udp.length", UdpLength, Num),
    ("icmp.type", IcmpType, Num),
    ("icmp.code", IcmpCode, Num),
    ("icmpv6.type", Icmpv6Type, Num),
    ("dns.qry.name", DnsQryName, Text),
    ("dns.qry.type", DnsQryType, Num),
    ("dns.flags.response", DnsResponse, Bool),
    ("dns.id", DnsId, Num),
    ("dns.a", DnsA, Ip),
    ("tls.handshake.extensions_server_name", TlsSni, Text),
    ("tls.sni", TlsSni, Text),
    ("tls.handshake.type", TlsHandshakeType, Num),
    ("http.host", HttpHost, Text),
    ("http.request.method", HttpMethod, Text),
    ("http.request.uri", HttpUri, Text),
    ("http.response.code", HttpCode, Num),
    ("app", App, Text),
    ("port", Port, Num),
    ("host", FieldId::Host, Kind::Host),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Proto {
    Frame,
    Eth,
    Arp,
    Ip,
    Ipv6,
    Icmp,
    Icmpv6,
    Igmp,
    Tcp,
    Udp,
    Dns,
    Mdns,
    Llmnr,
    Tls,
    Http,
    Quic,
    Dhcp,
    Ntp,
    Ssdp,
    Nbns,
}

pub(crate) const PROTOCOLS: &[(&str, Proto)] = &[
    ("frame", Proto::Frame),
    ("eth", Proto::Eth),
    ("arp", Proto::Arp),
    ("ip", Proto::Ip),
    ("ipv6", Proto::Ipv6),
    ("icmp", Proto::Icmp),
    ("icmpv6", Proto::Icmpv6),
    ("igmp", Proto::Igmp),
    ("tcp", Proto::Tcp),
    ("udp", Proto::Udp),
    ("dns", Proto::Dns),
    ("mdns", Proto::Mdns),
    ("llmnr", Proto::Llmnr),
    ("tls", Proto::Tls),
    ("ssl", Proto::Tls),
    ("http", Proto::Http),
    ("quic", Proto::Quic),
    ("dhcp", Proto::Dhcp),
    ("bootp", Proto::Dhcp),
    ("ntp", Proto::Ntp),
    ("ssdp", Proto::Ssdp),
    ("nbns", Proto::Nbns),
    ("netbios", Proto::Nbns),
];

pub(crate) fn field(name: &str) -> Option<(FieldId, Kind, &'static str)> {
    FIELDS.iter().find(|(n, _, _)| n.eq_ignore_ascii_case(name)).map(|&(n, id, kind)| (id, kind, n))
}

pub(crate) fn protocol(name: &str) -> Option<Proto> {
    PROTOCOLS.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|&(_, p)| p)
}

/// The closest known name, if it is close enough to be a likely typo.
pub(crate) fn suggest(word: &str) -> Option<&'static str> {
    let word = word.to_ascii_lowercase();
    let limit = match word.chars().count() {
        0..=4 => 1,
        5..=8 => 2,
        _ => 3,
    };
    FIELDS
        .iter()
        .map(|(n, _, _)| *n)
        .chain(PROTOCOLS.iter().map(|(n, _)| *n))
        .map(|n| (distance(&word, n), n))
        .filter(|&(d, _)| d <= limit)
        .min_by_key(|&(d, _)| d)
        .map(|(_, n)| n)
}

/// Field names that start with `prefix.`, for "fields for tcp include…".
pub(crate) fn fields_of(prefix: &str) -> Vec<&'static str> {
    let dotted = format!("{}.", prefix.to_ascii_lowercase());
    let mut out: Vec<&'static str> = FIELDS.iter().map(|(n, _, _)| *n).filter(|n| n.starts_with(&dotted)).collect();
    out.dedup();
    out
}

/// Edit distance counting a swap of two neighbouring letters as one edit.
fn distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut d = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for (i, row) in d.iter_mut().enumerate() {
        row[0] = i;
    }
    for (j, cell) in d[0].iter_mut().enumerate() {
        *cell = j;
    }
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            let mut best = (d[i - 1][j] + 1).min(d[i][j - 1] + 1).min(d[i - 1][j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                best = best.min(d[i - 2][j - 2] + 1);
            }
            d[i][j] = best;
        }
    }
    d[a.len()][b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suggests_close_names() {
        assert_eq!(distance("tcp.prot", "tcp.port"), 1);
        assert_eq!(suggest("tcp.prot"), Some("tcp.port"));
        assert_eq!(suggest("dns.qry.nam"), Some("dns.qry.name"));
        assert_eq!(suggest("github.com"), None);
    }
}
