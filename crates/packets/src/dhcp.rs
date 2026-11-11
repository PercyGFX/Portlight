//! DHCP: how a device gets its address and network settings.

use std::net::{IpAddr, Ipv4Addr};

use crate::bytes::{array, ipv4_at, range, u8_at, u16_at, u32_at};
use crate::packet::Payload;
use crate::summary::{Desc, Who};
use crate::text::{byte_count, duration, lossy, mac};
use crate::{Field, Layer};

const COOKIE: [u8; 4] = [99, 130, 83, 99];
const MAX_OPTIONS: usize = 128;

#[derive(Debug, Clone, Copy)]
pub(crate) struct Opt {
    pub code: u8,
    pub at: usize,
    pub len: usize,
}

#[derive(Debug, Clone)]
pub(crate) struct Dhcp {
    pub at: usize,
    pub op: u8,
    pub xid: u32,
    pub ciaddr: Ipv4Addr,
    pub yiaddr: Ipv4Addr,
    pub chaddr: Option<[u8; 6]>,
    pub cookie: bool,
    pub options: Vec<Opt>,
    pub msg_type: Option<u8>,
    pub requested: Option<Ipv4Addr>,
    pub hostname: Option<String>,
}

pub(crate) fn parse(frame: &[u8], at: usize, end: usize) -> Option<Dhcp> {
    let b = frame.get(..end)?;
    let op = u8_at(b, at)?;
    let hlen = u8_at(b, at + 2)?;
    let mut d = Dhcp {
        at,
        op,
        xid: u32_at(b, at + 4)?,
        ciaddr: ipv4_at(b, at + 12)?,
        yiaddr: ipv4_at(b, at + 16)?,
        chaddr: if hlen == 6 { array::<6>(b, at + 28) } else { None },
        cookie: array::<4>(b, at + 236) == Some(COOKIE),
        options: Vec::new(),
        msg_type: None,
        requested: None,
        hostname: None,
    };
    if !d.cookie {
        return Some(d);
    }
    let mut pos = at + 240;
    while pos < end && d.options.len() < MAX_OPTIONS {
        let Some(code) = u8_at(b, pos) else { break };
        match code {
            0 => pos += 1,
            255 => {
                d.options.push(Opt { code, at: pos, len: 1 });
                break;
            }
            _ => {
                let Some(len) = u8_at(b, pos + 1) else { break };
                let len = usize::from(len);
                if pos + 2 + len > end {
                    break;
                }
                let data_at = pos + 2;
                match code {
                    53 => d.msg_type = u8_at(b, data_at).filter(|_| len >= 1),
                    50 if len == 4 => d.requested = ipv4_at(b, data_at),
                    12 => d.hostname = Some(lossy(range(b, data_at, data_at + len), 64)),
                    _ => {}
                }
                d.options.push(Opt { code, at: pos, len: len + 2 });
                pos = data_at + len;
            }
        }
    }
    Some(d)
}

fn type_name(t: u8) -> &'static str {
    match t {
        1 => "Discover",
        2 => "Offer",
        3 => "Request",
        4 => "Decline",
        5 => "ACK",
        6 => "NAK",
        7 => "Release",
        8 => "Inform",
        _ => "Unknown",
    }
}

pub(crate) fn info(d: &Dhcp) -> String {
    match d.msg_type {
        Some(t) => format!("DHCP {} - Transaction ID 0x{:08x}", type_name(t), d.xid),
        None if d.op == 2 => format!("Boot Reply - Transaction ID 0x{:08x}", d.xid),
        None => format!("Boot Request - Transaction ID 0x{:08x}", d.xid),
    }
}

pub(crate) fn describe(frame: &[u8], p: &Payload, who: &Who) -> Option<Desc> {
    let d = parse(frame, p.at, p.end)?;
    // A device without an address yet sends from 0.0.0.0: name it by its host name if it gave one.
    let device = if who.src == "This PC" {
        who.src.clone()
    } else if let Some(h) = &d.hostname {
        h.clone()
    } else if who.src_ip.is_none_or(|ip| ip.is_unspecified()) {
        "A device".to_string()
    } else {
        who.src.clone()
    };
    let addr =
        |a: Option<Ipv4Addr>| a.filter(|a| !a.is_unspecified()).map_or("an address".to_string(), |a| a.to_string());
    let summary = match d.msg_type {
        Some(1) => format!("{device} looks for a DHCP server to get an address"),
        Some(2) => format!("The DHCP server offers the address {}", addr(Some(d.yiaddr))),
        Some(3) => format!("{device} asks to use the address {}", addr(d.requested.or(Some(d.ciaddr)))),
        Some(4) => format!("{device} turns down the offered address (it is already in use)"),
        Some(5) => format!(
            "The DHCP server confirms the address {}",
            addr(Some(d.yiaddr).filter(|a| !a.is_unspecified()).or(Some(d.ciaddr)))
        ),
        Some(6) => "The DHCP server refuses the request".to_string(),
        Some(7) => format!("{device} gives back its address {}", addr(Some(d.ciaddr))),
        Some(8) => format!("{device} asks the DHCP server for network settings"),
        _ if d.op == 2 => format!("{} answers an address request", who.src),
        _ => format!("{device} asks for an address"),
    };
    Some(Desc::new("DHCP", info(&d), summary))
}

fn opt_name(code: u8) -> &'static str {
    match code {
        1 => "Subnet mask",
        3 => "Router",
        6 => "DNS servers",
        12 => "Host name",
        15 => "Domain name",
        28 => "Broadcast address",
        42 => "Time servers",
        43 => "Vendor-specific information",
        50 => "Requested IP address",
        51 => "Lease time",
        53 => "DHCP message type",
        54 => "DHCP server",
        55 => "Parameter request list",
        57 => "Maximum message size",
        58 => "Renewal time",
        59 => "Rebinding time",
        60 => "Vendor class",
        61 => "Client identifier",
        81 => "Client full name",
        116 => "Auto-configuration",
        119 => "Domain search list",
        121 => "Classless static routes",
        252 => "Web proxy auto-discovery",
        255 => "End",
        _ => "Option",
    }
}

fn option_field(frame: &[u8], o: &Opt) -> Field {
    let data_at = o.at + 2;
    let data_len = o.len.saturating_sub(2);
    let data = range(frame, data_at, data_at + data_len);
    let ips = || data.as_chunks::<4>().0.iter().map(|c| Ipv4Addr::from(*c).to_string()).collect::<Vec<_>>().join(", ");
    let name = opt_name(o.code);
    let (value, explain) = match o.code {
        255 => (String::new(), None),
        53 => {
            let t = data.first().copied().unwrap_or(0);
            (
                format!("{t} ({})", type_name(t)),
                Some("What this message is: Discover → Offer → Request → ACK is the usual order."),
            )
        }
        50 => (ips(), Some("The address the device would like, often the one it had before.")),
        12 => (lossy(data, 64), Some("The name the device calls itself.")),
        54 => (ips(), Some("Which DHCP server this is about (usually the router).")),
        51 | 58 | 59 => {
            let secs = u32_at(data, 0).unwrap_or(0);
            let text = if secs == u32::MAX {
                "forever".to_string()
            } else {
                format!("{secs} ({})", duration(u64::from(secs)))
            };
            (text, (o.code == 51).then_some("How long the device may keep the address before asking again."))
        }
        1 | 3 | 6 | 28 | 42 => (
            ips(),
            match o.code {
                3 => Some("The router: the way out of the local network to the internet."),
                6 => Some("The DNS servers the device should use for name lookups."),
                _ => None,
            },
        ),
        15 | 60 => (lossy(data, 64), None),
        57 => (u16_at(data, 0).map_or(String::new(), |n| byte_count(usize::from(n))), None),
        55 => {
            (format!("{} settings asked for", data.len()), Some("Which settings the device wants the server to send."))
        }
        _ => (byte_count(data_len), None),
    };
    let label = if name == "Option" { format!("Option {}", o.code) } else { name.to_string() };
    let mut field = Field::new(&label, value, o.at, o.len);
    if let Some(e) = explain {
        field = field.explain(e);
    }
    field
}

pub(crate) fn layer(frame: &[u8], p: &Payload) -> Option<Layer> {
    let d = parse(frame, p.at, p.end)?;
    let at = d.at;
    let mut layer = Layer::new(
        "dhcp",
        "Dynamic Host Configuration Protocol (DHCP)",
        info(&d),
        "DHCP is how a device gets its IP address and network settings (router, DNS servers) when it joins a network.",
        p.at,
        p.end,
    );
    let op = if d.op == 2 { "2 (Boot Reply)" } else { "1 (Boot Request)" };
    layer.push(Field::new("Message type", op, at, 1).explain("1 = from a device, 2 = from a DHCP server."));
    layer.push(Field::new(
        "Hardware type",
        u8_at(frame, at + 1).map_or(String::new(), |h| if h == 1 { "1 (Ethernet)".into() } else { h.to_string() }),
        at + 1,
        1,
    ));
    layer.push(Field::new("Hardware address length", u8_at(frame, at + 2).unwrap_or(0).to_string(), at + 2, 1));
    layer.push(Field::new("Hops", u8_at(frame, at + 3).unwrap_or(0).to_string(), at + 3, 1));
    layer.push(
        Field::new("Transaction ID", format!("0x{:08x}", d.xid), at + 4, 4)
            .explain("A random number that ties the replies to this request."),
    );
    if let Some(secs) = u16_at(frame, at + 8) {
        layer.push(Field::new("Seconds elapsed", secs.to_string(), at + 8, 2));
    }
    if let Some(flags) = u16_at(frame, at + 10) {
        let text =
            if flags & 0x8000 != 0 { format!("0x{flags:04x} (Broadcast)") } else { format!("0x{flags:04x} (Unicast)") };
        layer.push(
            Field::new("Flags", text, at + 10, 2)
                .explain("Broadcast asks the server to answer to everyone, because the device has no address yet."),
        );
    }
    let addresses = [
        ("Client IP address", 12, "The device's current address, if it has one."),
        ("Your (client) IP address", 16, "The address the server is giving the device."),
        ("Next server IP address", 20, ""),
        ("Relay agent IP address", 24, ""),
    ];
    for (label, off, explain) in addresses {
        if let Some(ip) = ipv4_at(frame, at + off) {
            let mut f = Field::new(label, IpAddr::V4(ip).to_string(), at + off, 4);
            if !explain.is_empty() {
                f = f.explain(explain);
            }
            layer.push(f);
        }
    }
    if let Some(m) = d.chaddr {
        layer.push(
            Field::new("Client MAC address", mac(&m), at + 28, 6).explain("The hardware address of the device asking."),
        );
    }
    for (label, off, len) in [("Server host name", 44, 64), ("Boot file name", 108, 128)] {
        let bytes = range(frame, at + off, at + off + len);
        if bytes.iter().any(|&c| c != 0) {
            let text = bytes.split(|&c| c == 0).next().unwrap_or(&[]);
            layer.push(Field::new(label, lossy(text, 128), at + off, len));
        }
    }
    if d.cookie {
        layer.push(Field::new("Magic cookie", "DHCP", at + 236, 4).explain("Marks the start of the DHCP options."));
        for o in &d.options {
            layer.push(option_field(frame, o));
        }
    }
    Some(layer)
}
