//! ARP: finding the hardware address behind an IPv4 address on the local network.

use std::net::{IpAddr, Ipv4Addr};

use crate::bytes::{array, ipv4_at, u8_at, u16_at};
use crate::summary::{Desc, Who};
use crate::text::mac;
use crate::{Field, Layer};

#[derive(Debug, Clone, Copy)]
pub(crate) struct Arp {
    pub at: usize,
    pub op: u16,
    /// Sender and target addresses, when this is the usual Ethernet + IPv4 kind.
    pub addrs: Option<Addrs>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Addrs {
    pub sha: [u8; 6],
    pub spa: Ipv4Addr,
    pub tha: [u8; 6],
    pub tpa: Ipv4Addr,
}

pub(crate) fn parse(frame: &[u8], at: usize) -> Option<Arp> {
    let htype = u16_at(frame, at)?;
    let ptype = u16_at(frame, at + 2)?;
    let hlen = u8_at(frame, at + 4)?;
    let plen = u8_at(frame, at + 5)?;
    let op = u16_at(frame, at + 6)?;
    let addrs = if htype == 1 && ptype == 0x0800 && hlen == 6 && plen == 4 {
        (|| {
            Some(Addrs {
                sha: array::<6>(frame, at + 8)?,
                spa: ipv4_at(frame, at + 14)?,
                tha: array::<6>(frame, at + 18)?,
                tpa: ipv4_at(frame, at + 24)?,
            })
        })()
    } else {
        None
    };
    Some(Arp { at, op, addrs })
}

fn op_name(op: u16) -> &'static str {
    match op {
        1 => "request",
        2 => "reply",
        3 => "RARP request",
        4 => "RARP reply",
        8 => "InARP request",
        9 => "InARP reply",
        _ => "unknown",
    }
}

pub(crate) fn describe(arp: &Arp, who: &Who) -> Desc {
    let Some(a) = arp.addrs else {
        return Desc::new("ARP", format!("ARP {}", op_name(arp.op)), "An address question on the local network".into());
    };
    let sender = who.name(IpAddr::V4(a.spa));
    match arp.op {
        1 if a.spa == a.tpa => Desc::new(
            "ARP",
            format!("ARP Announcement for {}", a.spa),
            format!("{sender} tells the local network that it has the address {}", a.spa),
        ),
        1 if a.spa.is_unspecified() => Desc::new(
            "ARP",
            format!("Who has {}? (ARP Probe)", a.tpa),
            format!("A device checks that nobody on the local network already uses {}", a.tpa),
        ),
        1 => Desc::new(
            "ARP",
            format!("Who has {}? Tell {}", a.tpa, a.spa),
            format!("{sender} asks who on the local network has {}", a.tpa),
        ),
        2 => Desc::new(
            "ARP",
            format!("{} is at {}", a.spa, mac(&a.sha)),
            format!("{sender} answers: {} is at {}", a.spa, mac(&a.sha)),
        ),
        op => Desc::new("ARP", format!("ARP {}", op_name(op)), format!("{sender} sends an ARP message")),
    }
}

pub(crate) fn layer(frame: &[u8], arp: &Arp) -> Layer {
    let at = arp.at;
    let summary = match arp.addrs {
        Some(a) if arp.op == 1 => format!("Who has {}? Tell {}", a.tpa, a.spa),
        Some(a) if arp.op == 2 => format!("{} is at {}", a.spa, mac(&a.sha)),
        _ => format!("ARP {}", op_name(arp.op)),
    };
    let end = if arp.addrs.is_some() { at + 28 } else { (at + 8).min(frame.len()) };
    let mut layer = Layer::new(
        "arp",
        "Address Resolution Protocol (ARP)",
        summary,
        "ARP finds the hardware (MAC) address that belongs to an IP address on the local network, \
         so packets can be delivered there. One device asks everyone; the owner of the address answers.",
        at,
        end,
    );
    if let Some(h) = u16_at(frame, at) {
        layer.push(Field::new("Hardware type", if h == 1 { "1 (Ethernet)".into() } else { h.to_string() }, at, 2));
    }
    if let Some(p) = u16_at(frame, at + 2) {
        layer.push(Field::new("Protocol type", crate::names::ethertype_value(p), at + 2, 2));
    }
    layer.push(Field::new("Hardware address size", u8_at(frame, at + 4).unwrap_or(0).to_string(), at + 4, 1));
    layer.push(Field::new("Protocol address size", u8_at(frame, at + 5).unwrap_or(0).to_string(), at + 5, 1));
    layer.push(
        Field::new("Opcode", format!("{} ({})", arp.op, op_name(arp.op)), at + 6, 2)
            .explain("1 = a question (who has this address?), 2 = the answer."),
    );
    if let Some(a) = arp.addrs {
        layer.push(Field::new("Sender MAC address", mac(&a.sha), at + 8, 6));
        layer.push(Field::new("Sender IP address", a.spa.to_string(), at + 14, 4));
        let target_mac = if arp.op == 1 && a.tha == [0; 6] {
            format!("{} (unknown yet: this is what is being asked)", mac(&a.tha))
        } else {
            mac(&a.tha)
        };
        layer.push(Field::new("Target MAC address", target_mac, at + 18, 6));
        layer.push(Field::new("Target IP address", a.tpa.to_string(), at + 24, 4));
    }
    layer
}
