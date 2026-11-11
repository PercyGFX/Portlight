//! Tests a parsed filter against one packet, decoding only what the filter needs.

use std::cell::OnceCell;
use std::net::IpAddr;

use super::PacketRef;
use super::fields::{FieldId, Proto};
use super::lexer::Cmp;
use super::parser::{Expr, Test, Value};
use crate::packet::{App, DnsKind, Headers, L4, Net};
use crate::text::{contains_ci, eq_ci, mac};
use crate::{Summary, dns, http, summarize, tcp, tls};

/// One value of a field in a packet.
enum Val<'s> {
    Num(u64),
    Ip(IpAddr),
    Mac([u8; 6]),
    Bool(bool),
    Text(&'s str),
}

/// A packet's headers, plus the deeper parts decoded on first use.
pub(crate) struct Facts<'p, 'a> {
    p: &'p PacketRef<'a>,
    h: Headers,
    dns: OnceCell<Option<dns::Message>>,
    tls: OnceCell<Option<tls::Tls>>,
    http: OnceCell<Option<http::Http>>,
    names: OnceCell<Vec<String>>,
    row: OnceCell<Summary>,
}

impl<'p, 'a> Facts<'p, 'a> {
    pub(crate) fn new(p: &'p PacketRef<'a>) -> Self {
        Facts {
            p,
            h: Headers::parse(p.link, p.frame),
            dns: OnceCell::new(),
            tls: OnceCell::new(),
            http: OnceCell::new(),
            names: OnceCell::new(),
            row: OnceCell::new(),
        }
    }

    fn dns(&self) -> Option<&dns::Message> {
        self.dns
            .get_or_init(|| match self.h.app {
                App::Dns(_) => self.h.payload_info().and_then(|pl| dns::parse_payload(self.p.frame, &pl)),
                _ => None,
            })
            .as_ref()
    }

    fn tls(&self) -> Option<&tls::Tls> {
        self.tls
            .get_or_init(|| match self.h.app {
                App::Tls => self.h.payload_info().map(|pl| tls::parse(self.p.frame, pl.at, pl.end)),
                _ => None,
            })
            .as_ref()
    }

    fn http(&self) -> Option<&http::Http> {
        self.http
            .get_or_init(|| match self.h.app {
                App::Http => self
                    .h
                    .payload_info()
                    .map(|pl| http::parse(self.p.frame, pl.at, pl.end))
                    .filter(|h| !matches!(h.start, http::Start::Continuation)),
                _ => None,
            })
            .as_ref()
    }

    fn row(&self) -> &Summary {
        self.row.get_or_init(|| summarize(self.p.link, self.p.frame, self.p.original_len, self.p.ctx))
    }

    /// Names for `host`: "This PC", looked-up names, the TLS server name, the HTTP host.
    fn names(&self) -> &[String] {
        self.names.get_or_init(|| {
            let mut out = Vec::new();
            for ip in self.ips() {
                if self.p.ctx.is_local(ip) {
                    out.push("This PC".to_string());
                } else if let Some(name) = self.p.ctx.name_of(ip) {
                    out.push(name);
                }
            }
            if let Some(sni) = self.tls().and_then(tls::sni) {
                out.push(sni.to_string());
            }
            if let Some(host) = self.http().and_then(|h| h.header("host")) {
                // "example.com:8080" → "example.com".
                let host =
                    host.rsplit_once(':').filter(|(_, port)| port.parse::<u16>().is_ok()).map_or(host, |(h, _)| h);
                out.push(host.to_string());
            }
            out
        })
    }

    /// The packet's IP addresses, from IP or ARP.
    fn ips(&self) -> Vec<IpAddr> {
        match &self.h.net {
            Net::Ip(ip) => vec![ip.src, ip.dst],
            Net::Arp(a) => a.addrs.map_or(Vec::new(), |a| vec![IpAddr::V4(a.spa), IpAddr::V4(a.tpa)]),
            Net::None => Vec::new(),
        }
    }

    fn tcp(&self) -> Option<&tcp::Tcp> {
        match &self.h.l4 {
            L4::Tcp(t) => Some(t),
            _ => None,
        }
    }

    fn has_proto(&self, p: Proto) -> bool {
        let h = &self.h;
        let ip = h.ip();
        match p {
            Proto::Frame => true,
            Proto::Eth => h.link.eth().is_some(),
            Proto::Arp => matches!(h.net, Net::Arp(_)),
            Proto::Ip => ip.is_some_and(|ip| !ip.v6),
            Proto::Ipv6 => ip.is_some_and(|ip| ip.v6),
            Proto::Icmp => matches!(h.l4, L4::Icmp(i) if !i.v6),
            Proto::Icmpv6 => matches!(h.l4, L4::Icmp(i) if i.v6),
            Proto::Igmp => matches!(h.l4, L4::Igmp { .. }),
            Proto::Tcp => matches!(h.l4, L4::Tcp(_)) || ip.is_some_and(|ip| ip.proto == 6),
            Proto::Udp => matches!(h.l4, L4::Udp(_)) || ip.is_some_and(|ip| ip.proto == 17),
            Proto::Dns => matches!(h.app, App::Dns(_)),
            Proto::Mdns => h.app == App::Dns(DnsKind::Mdns),
            Proto::Llmnr => h.app == App::Dns(DnsKind::Llmnr),
            Proto::Tls => h.app == App::Tls,
            Proto::Http => h.app == App::Http,
            Proto::Quic => h.app == App::Quic,
            Proto::Dhcp => h.app == App::Dhcp,
            Proto::Ntp => h.app == App::Ntp,
            Proto::Ssdp => h.app == App::Ssdp,
            Proto::Nbns => matches!(h.app, App::Nbns | App::Nbds),
        }
    }

    fn values(&self, f: FieldId) -> Vec<Val<'_>> {
        use FieldId::*;
        let h = &self.h;
        let ip = h.ip();
        let v6 = ip.filter(|ip| ip.v6);
        let tcp = self.tcp();
        let udp = match &h.l4 {
            L4::Udp(u) => Some(u),
            _ => None,
        };
        let icmp = match &h.l4 {
            L4::Icmp(i) => Some(i),
            _ => None,
        };
        let eth = h.link.eth();
        let num = |n: Option<u64>| n.map(Val::Num).into_iter().collect::<Vec<_>>();
        let flag = |bit: u16| tcp.map(|t| Val::Bool(t.has(bit))).into_iter().collect::<Vec<_>>();
        match f {
            FrameNumber => vec![Val::Num(u64::from(self.p.number))],
            FrameLen => {
                let len =
                    if self.p.original_len == 0 { self.p.frame.len() as u64 } else { u64::from(self.p.original_len) };
                vec![Val::Num(len)]
            }
            FrameCapLen => vec![Val::Num(self.p.frame.len() as u64)],
            IpSrc => ip.map(|i| Val::Ip(i.src)).into_iter().collect(),
            IpDst => ip.map(|i| Val::Ip(i.dst)).into_iter().collect(),
            IpAddr => ip.map_or(Vec::new(), |i| vec![Val::Ip(i.src), Val::Ip(i.dst)]),
            IpTtl => num(ip.map(|i| u64::from(i.ttl))),
            IpProto => num(ip.map(|i| u64::from(i.proto))),
            Ipv6Src => v6.map(|i| Val::Ip(i.src)).into_iter().collect(),
            Ipv6Dst => v6.map(|i| Val::Ip(i.dst)).into_iter().collect(),
            Ipv6Addr => v6.map_or(Vec::new(), |i| vec![Val::Ip(i.src), Val::Ip(i.dst)]),
            EthSrc => eth.map(|e| Val::Mac(e.src)).into_iter().collect(),
            EthDst => eth.map(|e| Val::Mac(e.dst)).into_iter().collect(),
            EthAddr => eth.map_or(Vec::new(), |e| vec![Val::Mac(e.src), Val::Mac(e.dst)]),
            TcpPort => tcp.map_or(Vec::new(), |t| vec![Val::Num(t.src_port.into()), Val::Num(t.dst_port.into())]),
            TcpSrcPort => num(tcp.map(|t| t.src_port.into())),
            TcpDstPort => num(tcp.map(|t| t.dst_port.into())),
            TcpLen => num(tcp.map(|t| t.payload_len() as u64)),
            TcpSeq => num(tcp.map(|t| t.seq.into())),
            TcpAck => num(tcp.filter(|t| t.has(tcp::ACK)).map(|t| t.ack.into())),
            TcpFlags => num(tcp.map(|t| t.flags.into())),
            TcpWindow => num(tcp.map(|t| t.window.into())),
            TcpSyn => flag(tcp::SYN),
            TcpAckFlag => flag(tcp::ACK),
            TcpFin => flag(tcp::FIN),
            TcpRst => flag(tcp::RST),
            TcpPush => flag(tcp::PSH),
            UdpPort => udp.map_or(Vec::new(), |u| vec![Val::Num(u.src_port.into()), Val::Num(u.dst_port.into())]),
            UdpSrcPort => num(udp.map(|u| u.src_port.into())),
            UdpDstPort => num(udp.map(|u| u.dst_port.into())),
            UdpLength => num(udp.map(|u| u.length.into())),
            IcmpType => num(icmp.filter(|i| !i.v6).map(|i| i.kind.into())),
            IcmpCode => num(icmp.filter(|i| !i.v6).map(|i| i.code.into())),
            Icmpv6Type => num(icmp.filter(|i| i.v6).map(|i| i.kind.into())),
            DnsQryName => {
                self.dns().map_or(Vec::new(), |m| m.questions.iter().map(|q| Val::Text(&q.name.text)).collect())
            }
            DnsQryType => {
                self.dns().map_or(Vec::new(), |m| m.questions.iter().map(|q| Val::Num(q.qtype.into())).collect())
            }
            DnsResponse => self.dns().map(|m| Val::Bool(m.is_response())).into_iter().collect(),
            DnsId => num(self.dns().map(|m| m.id.into())),
            DnsA => self.dns().map_or(Vec::new(), |m| m.addresses().map(Val::Ip).collect()),
            TlsSni => self.tls().and_then(tls::sni).map(Val::Text).into_iter().collect(),
            TlsHandshakeType => {
                self.tls().map_or(Vec::new(), |t| tls::handshake_types(t).map(|n| Val::Num(n.into())).collect())
            }
            HttpHost => self.http().and_then(|h| h.header("host")).map(Val::Text).into_iter().collect(),
            HttpMethod => match self.http().map(|h| &h.start) {
                Some(http::Start::Request { method, .. }) => vec![Val::Text(method)],
                _ => Vec::new(),
            },
            HttpUri => match self.http().map(|h| &h.start) {
                Some(http::Start::Request { uri, .. }) => vec![Val::Text(uri)],
                _ => Vec::new(),
            },
            HttpCode => match self.http().map(|h| &h.start) {
                Some(http::Start::Response { code, .. }) => vec![Val::Num((*code).into())],
                _ => Vec::new(),
            },
            App => self.p.app.map(Val::Text).into_iter().collect(),
            Port => h.ports().map_or(Vec::new(), |(s, d)| vec![Val::Num(s.into()), Val::Num(d.into())]),
            Host => {
                let mut out: Vec<Val> = self.ips().into_iter().map(Val::Ip).collect();
                out.extend(self.names().iter().map(|n| Val::Text(n)));
                out
            }
        }
    }

    fn text_search(&self, needle: &str) -> bool {
        let row = self.row();
        let names = [row.source_name.as_deref(), row.destination_name.as_deref(), self.p.app];
        [&row.source, &row.destination, &row.protocol, &row.info, &row.summary].iter().any(|s| contains_ci(s, needle))
            || names.iter().flatten().any(|s| contains_ci(s, needle))
            || self.h.link.eth().is_some_and(|e| {
                // MAC addresses as typed with dashes, as Windows shows them.
                let dashed = needle.replace('-', ":");
                needle.len() >= 8 && (mac(&e.src).contains(&dashed) || mac(&e.dst).contains(&dashed))
            })
    }
}

pub(crate) fn eval(expr: &Expr, facts: &Facts) -> bool {
    match expr {
        Expr::And(terms) => terms.iter().all(|t| eval(t, facts)),
        Expr::Or(terms) => terms.iter().any(|t| eval(t, facts)),
        Expr::Not(inner) => !eval(inner, facts),
        Expr::Test(test) => match test {
            Test::Proto(p) => facts.has_proto(*p),
            Test::Has(f) => {
                let vals = facts.values(*f);
                if vals.iter().any(|v| matches!(v, Val::Bool(_))) {
                    vals.iter().any(|v| matches!(v, Val::Bool(true)))
                } else {
                    !vals.is_empty()
                }
            }
            Test::Cmp(f, op, value) => compare(&facts.values(*f), *op, value),
            Test::Contains(f, needle) => facts.values(*f).iter().any(|v| match v {
                Val::Text(s) => contains_ci(s, needle),
                Val::Ip(ip) => ip.to_string().contains(needle.as_str()),
                Val::Mac(m) => mac(m).contains(needle.as_str()),
                Val::Num(_) | Val::Bool(_) => false,
            }),
            Test::Text(needle) => facts.text_search(needle),
        },
    }
}

/// "==": any value matches. "!=": there are values and none matches. Order: any value.
fn compare(vals: &[Val], op: Cmp, value: &Value) -> bool {
    if vals.is_empty() {
        return false;
    }
    match op {
        Cmp::Eq => vals.iter().any(|v| equal(v, value)),
        Cmp::Ne => !vals.iter().any(|v| equal(v, value)),
        _ => vals.iter().any(|v| match (v, value) {
            (Val::Num(a), Value::Num(b)) => match op {
                Cmp::Gt => a > b,
                Cmp::Lt => a < b,
                Cmp::Ge => a >= b,
                _ => a <= b,
            },
            _ => false,
        }),
    }
}

fn equal(v: &Val, value: &Value) -> bool {
    match (v, value) {
        (Val::Num(a), Value::Num(b)) => a == b,
        (Val::Ip(a), Value::Net(net)) => net.contains(*a),
        (Val::Mac(a), Value::Mac(b)) => a == b,
        (Val::Bool(a), Value::Bool(b)) => a == b,
        (Val::Text(s), Value::Text(t)) => eq_ci(s.trim_end_matches('.'), t.trim_end_matches('.')),
        _ => false,
    }
}
