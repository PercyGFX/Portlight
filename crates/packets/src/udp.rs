//! UDP: single messages between two programs, without a connection.

use crate::bytes::u16_at;
use crate::names::port_value;
use crate::summary::{Desc, Who};
use crate::text::byte_count;
use crate::{Field, Layer};

#[derive(Debug, Clone, Copy)]
pub(crate) struct Udp {
    pub at: usize,
    pub src_port: u16,
    pub dst_port: u16,
    pub length: u16,
    pub payload_at: usize,
    pub end: usize,
}

impl Udp {
    pub(crate) fn payload_len(&self) -> usize {
        self.end.saturating_sub(self.payload_at)
    }
}

/// Reads a UDP header that ends no later than `end` (the end of the IP packet).
pub(crate) fn parse(frame: &[u8], at: usize, end: usize) -> Option<Udp> {
    let b = frame.get(..end)?;
    let length = u16_at(b, at + 4)?;
    u16_at(b, at + 6)?;
    let declared = usize::from(length);
    let end = if declared >= 8 { (at + declared).min(end) } else { end };
    Some(Udp { at, src_port: u16_at(b, at)?, dst_port: u16_at(b, at + 2)?, length, payload_at: at + 8, end })
}

pub(crate) fn info(u: &Udp) -> String {
    format!("{} → {} Len={}", u.src_port, u.dst_port, u.payload_len())
}

pub(crate) fn describe(u: &Udp, who: &Who) -> Desc {
    let summary = format!("{} sends {} to {} over UDP", who.src, byte_count(u.payload_len()), who.dst);
    Desc::new("UDP", info(u), summary)
}

pub(crate) fn layer(frame: &[u8], u: &Udp) -> Layer {
    let at = u.at;
    let mut layer = Layer::new(
        "udp",
        "User Datagram Protocol (UDP)",
        format!("From port {} to port {}, {} of data", u.src_port, port_value(u.dst_port), byte_count(u.payload_len())),
        "UDP sends single messages between two programs, with no connection and no promise of delivery. \
         It is quick, which suits name lookups, games, calls and video. Ports say which program on each side the data is for.",
        at,
        at + 8,
    );
    layer.push(Field::new("Source port", port_value(u.src_port), at, 2).explain(
        "Which program on the sending computer this belongs to. Apps usually get a random high number; \
         servers use a fixed, well-known one.",
    ));
    layer.push(Field::new("Destination port", port_value(u.dst_port), at + 2, 2).explain(
        "Which program on the receiving computer this is for. Well-known numbers name the service: \
         53 is DNS (name lookups), 443 is QUIC (web).",
    ));
    layer.push(
        Field::new(
            "Length",
            format!("{} (8 header + {} data)", byte_count(usize::from(u.length)), u.length.saturating_sub(8)),
            at + 4,
            2,
        )
        .explain("The size of this UDP message, header included."),
    );
    if let Some(sum) = u16_at(frame, at + 6) {
        if sum == 0 {
            layer.push(
                Field::new("Checksum", "0x0000 (none)", at + 6, 2).explain("UDP over IPv4 may skip the checksum."),
            );
        } else {
            layer.push(crate::ip::checksum_field(at + 6, sum));
        }
    }
    layer
}
