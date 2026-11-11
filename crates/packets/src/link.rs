//! The link layer: Ethernet (with VLAN tags), loopback and raw IP.

use crate::bytes::{array, u8_at, u16_at, u32_le_at};
use crate::names;
use crate::text::mac;
use crate::{Field, Layer, LinkType};

/// At most this many VLAN tags are read (802.1ad "Q-in-Q" uses two).
pub(crate) const MAX_VLANS: usize = 2;

pub(crate) const BROADCAST: [u8; 6] = [0xff; 6];

#[derive(Debug, Clone, Copy)]
pub(crate) struct Vlan {
    pub at: usize,
    pub tpid: u16,
    pub tci: u16,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Eth {
    pub dst: [u8; 6],
    pub src: [u8; 6],
    pub vlans: [Option<Vlan>; MAX_VLANS],
    /// The EtherType (or 802.3 length) that says what follows.
    pub ethertype: u16,
    pub type_at: usize,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum Kind {
    Ethernet(Eth),
    Null {
        family: u32,
    },
    Raw,
    /// Too short, or a link type this crate does not read.
    Unreadable,
}

/// What the link layer says comes next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Net {
    Ipv4,
    Ipv6,
    Arp,
    /// An 802.3 frame with an LLC header (spanning tree and other switch traffic).
    Llc,
    Other(u16),
    None,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Link {
    pub kind: Kind,
    /// Where the network layer starts.
    pub net_at: usize,
    pub net: Net,
}

impl Link {
    pub(crate) fn eth(&self) -> Option<&Eth> {
        match &self.kind {
            Kind::Ethernet(eth) => Some(eth),
            _ => None,
        }
    }
}

pub(crate) fn parse(link: LinkType, frame: &[u8]) -> Link {
    let unreadable = Link { kind: Kind::Unreadable, net_at: 0, net: Net::None };
    match link {
        LinkType::Ethernet => parse_ethernet(frame).unwrap_or(unreadable),
        LinkType::Null => match u32_le_at(frame, 0) {
            Some(family) => {
                let net = family_net(family).or_else(|| family_net(family.swap_bytes())).unwrap_or(Net::None);
                Link { kind: Kind::Null { family }, net_at: 4, net }
            }
            None => unreadable,
        },
        LinkType::Raw => {
            let net = match u8_at(frame, 0).map(|b| b >> 4) {
                Some(4) => Net::Ipv4,
                Some(6) => Net::Ipv6,
                _ => Net::None,
            };
            Link { kind: Kind::Raw, net_at: 0, net }
        }
        LinkType::Unknown(_) => unreadable,
    }
}

fn parse_ethernet(frame: &[u8]) -> Option<Link> {
    let dst = array::<6>(frame, 0)?;
    let src = array::<6>(frame, 6)?;
    let mut type_at = 12;
    let mut ethertype = u16_at(frame, type_at)?;
    let mut vlans = [None; MAX_VLANS];
    let mut count = 0;
    let mut net = None;
    while matches!(ethertype, 0x8100 | 0x88a8 | 0x9100) {
        if count == MAX_VLANS {
            net = Some(Net::Other(ethertype));
            break;
        }
        let (Some(tci), Some(inner)) = (u16_at(frame, type_at + 2), u16_at(frame, type_at + 4)) else {
            net = Some(Net::None);
            break;
        };
        vlans[count] = Some(Vlan { at: type_at, tpid: ethertype, tci });
        count += 1;
        type_at += 4;
        ethertype = inner;
    }
    let net = net.unwrap_or(match ethertype {
        0x0800 => Net::Ipv4,
        0x86dd => Net::Ipv6,
        0x0806 => Net::Arp,
        t if t <= 1500 => Net::Llc,
        t => Net::Other(t),
    });
    Some(Link { kind: Kind::Ethernet(Eth { dst, src, vlans, ethertype, type_at }), net_at: type_at + 2, net })
}

/// Address families seen in loopback headers (Windows, BSD, Linux, macOS).
fn family_net(family: u32) -> Option<Net> {
    match family {
        2 => Some(Net::Ipv4),
        10 | 23 | 24 | 28 | 30 => Some(Net::Ipv6),
        _ => None,
    }
}

/// How a MAC address reads to a person.
pub(crate) fn mac_value(m: &[u8; 6]) -> String {
    if *m == BROADCAST {
        return format!("{} (broadcast: everyone on the local network)", mac(m));
    }
    if m[0] == 0x01 && m[1] == 0x00 && m[2] == 0x5e {
        return format!("{} (IPv4 multicast)", mac(m));
    }
    if m[0] == 0x33 && m[1] == 0x33 {
        return format!("{} (IPv6 multicast)", mac(m));
    }
    mac(m)
}

pub(crate) fn layer(frame: &[u8], link: &Link) -> Option<Layer> {
    match &link.kind {
        Kind::Ethernet(eth) => Some(ethernet_layer(frame, eth, link.net_at)),
        Kind::Null { family } => {
            let mut layer = Layer::new(
                "loopback",
                "Loopback",
                "From one program on this PC to another",
                "This packet never left this PC: it went from one program to another through the loopback interface.",
                0,
                4,
            );
            let what = match family_net(*family).or_else(|| family_net(family.swap_bytes())) {
                Some(Net::Ipv4) => "IPv4",
                Some(Net::Ipv6) => "IPv6",
                _ => "unknown",
            };
            layer.push(Field::new("Address family", format!("{family} ({what})"), 0, 4));
            Some(layer)
        }
        Kind::Raw | Kind::Unreadable => None,
    }
}

fn ethernet_layer(frame: &[u8], eth: &Eth, header_end: usize) -> Layer {
    let mut layer = Layer::new(
        "eth",
        "Ethernet",
        format!("From {} to {}", mac(&eth.src), mac(&eth.dst)),
        "Ethernet carries the packet across the local network, from one network card to the next. \
         Its addresses are hardware (MAC) addresses, which only matter on this network, not on the internet.",
        0,
        header_end.min(frame.len()),
    );
    layer.push(
        Field::new("Destination", mac_value(&eth.dst), 0, 6).explain(
            "The hardware address of the network card this frame is for on the local network, often the router.",
        ),
    );
    layer.push(
        Field::new("Source", mac_value(&eth.src), 6, 6)
            .explain("The hardware address of the network card that sent this frame."),
    );
    for vlan in eth.vlans.iter().flatten() {
        let id = vlan.tci & 0x0fff;
        let priority = vlan.tci >> 13;
        layer.push(
            Field::new("VLAN tag", format!("VLAN {id}, priority {priority}"), vlan.at, 4)
                .explain("Splits one physical network into separate virtual ones. The number says which.")
                .child(Field::new("Tag type", names::ethertype_value(vlan.tpid), vlan.at, 2))
                .child(Field::new("Priority", priority.to_string(), vlan.at + 2, 1))
                .child(Field::new("VLAN ID", id.to_string(), vlan.at + 2, 2)),
        );
    }
    if u16_at(frame, eth.type_at).is_some() {
        let value = if eth.ethertype <= 1500 {
            format!("Length {} (an 802.3 frame)", eth.ethertype)
        } else {
            names::ethertype_value(eth.ethertype)
        };
        layer.push(
            Field::new("Type", value, eth.type_at, 2)
                .explain("Which protocol is inside this frame, e.g. IPv4, IPv6 or ARP."),
        );
    }
    layer
}

/// A field for bytes after the end of the IP packet: Ethernet pads short frames to 60 bytes.
pub(crate) fn padding_field(frame: &[u8], packet_end: usize) -> Option<Field> {
    let extra = frame.len().checked_sub(packet_end).filter(|&n| n > 0)?;
    Some(
        Field::new("Padding", crate::text::byte_count(extra), packet_end, extra).explain(
            "Filler bytes after the packet. Ethernet frames have a minimum size, so short packets are padded.",
        ),
    )
}
