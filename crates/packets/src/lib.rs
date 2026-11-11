//! Reads captured packets for Portlight.
//!
//! Given the raw bytes of one captured frame, this crate says what is in it:
//!
//! - [`endpoints`] picks out the addresses and ports, cheaply, so the caller
//!   can tell which app a packet belongs to.
//! - [`summarize`] makes one row of the packet list: a Wireshark-style
//!   technical line and a plain-English sentence.
//! - [`dissect`] breaks the frame into layers and fields, each tied to its
//!   bytes for the hex view.
//! - [`filter`] is the display-filter language of the capture page's search box.
//! - [`pcapng`] writes capture files that Wireshark opens.
//!
//! There is no operating-system code here. Capturing happens elsewhere, and
//! what the bytes do not say (which addresses are this PC's, which names apps
//! looked up) comes in through [`Context`]. Captured bytes are untrusted and
//! often cut short: every read is checked, and a malformed frame is described
//! as far as it goes, never a panic.

use std::net::IpAddr;

use serde::Serialize;

mod arp;
mod bytes;
mod dhcp;
mod dissect;
mod dns;
mod http;
mod icmp;
mod ip;
mod layer;
mod link;
mod misc;
mod names;
mod packet;
mod quic;
mod summary;
mod tcp;
mod text;
mod tls;
mod udp;

pub mod filter;
pub mod pcapng;

#[cfg(test)]
mod tests;

pub use dissect::dissect;
pub use packet::endpoints;
pub use summary::summarize;

/// How a frame starts, from the pcap link-type (DLT) number of the interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkType {
    /// Ethernet II, the usual case for wired and Wi-Fi adapters.
    Ethernet,
    /// BSD loopback: a 4-byte address family, then an IP packet.
    Null,
    /// A bare IP packet; the version comes from its first byte.
    Raw,
    /// Anything else. Such frames are kept but not decoded.
    Unknown(u32),
}

impl LinkType {
    /// Reads a pcap DLT number.
    pub fn from_dlt(dlt: u32) -> Self {
        match dlt {
            1 => LinkType::Ethernet,
            0 => LinkType::Null,
            12 | 14 | 101 | 228 | 229 => LinkType::Raw,
            other => LinkType::Unknown(other),
        }
    }

    /// The link-type number used in capture files (raw IP is written as 101, LINKTYPE_RAW).
    pub fn dlt(self) -> u32 {
        match self {
            LinkType::Ethernet => 1,
            LinkType::Null => 0,
            LinkType::Raw => 101,
            LinkType::Unknown(dlt) => dlt,
        }
    }
}

/// The transport protocol inside an IP packet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Transport {
    Tcp,
    Udp,
    /// ICMP or ICMPv6.
    Icmp,
    /// Any other IP protocol number.
    Other(u8),
}

/// The IP addresses and ports of a frame, read as cheaply as possible (no allocation).
///
/// Used on every captured packet to decide which app it belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Endpoints {
    pub transport: Transport,
    pub src: IpAddr,
    pub dst: IpAddr,
    /// TCP and UDP only, and not for the later pieces of a fragmented packet.
    pub src_port: Option<u16>,
    pub dst_port: Option<u16>,
}

/// What the caller knows that packets don't say.
pub trait Context {
    /// True for this PC's own addresses.
    fn is_local(&self, ip: IpAddr) -> bool;
    /// A friendly name for a remote address (domain the app asked for), if known.
    fn name_of(&self, ip: IpAddr) -> Option<String>;
}

/// Which way a packet went, relative to this PC.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    Out,
    In,
}

/// One row of the packet list: `PacketRow` in the contract without `number`,
/// `time` and `app`, which the caller knows.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    /// IP address, or MAC address for frames without one.
    pub source: String,
    pub destination: String,
    /// "This PC" for local addresses, else the name from [`Context::name_of`].
    pub source_name: Option<String>,
    pub destination_name: Option<String>,
    pub source_port: Option<u16>,
    pub destination_port: Option<u16>,
    /// The highest layer understood: "TLS", "DNS", "TCP", "ARP"...
    pub protocol: String,
    /// Bytes on the wire.
    pub length: u32,
    /// Wireshark-style technical line.
    pub info: String,
    /// What happened, in plain English.
    pub summary: String,
    pub direction: Option<Direction>,
}

/// Facts about the frame that come from the capture, not the bytes.
#[derive(Debug, Clone, Copy)]
pub struct FrameInfo<'a> {
    pub number: u32,
    /// Seconds since the capture started.
    pub time: f64,
    /// Unix time in milliseconds.
    pub timestamp_ms: f64,
    pub original_len: u32,
    pub interface_name: &'a str,
    pub app: Option<&'a str>,
}

/// One layer of a packet's breakdown: `PacketLayer` in the contract.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Layer {
    /// "frame", "eth", "ip", "tcp", "tls", "data"...
    pub id: String,
    pub title: String,
    pub summary: String,
    /// What this layer is for, in plain words.
    pub explain: String,
    /// Byte offset in the frame.
    pub start: usize,
    pub length: usize,
    pub fields: Vec<Field>,
}

/// One line of a layer: `PacketField` in the contract.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Field {
    pub label: String,
    pub value: String,
    pub explain: Option<String>,
    /// Byte offset in the frame.
    pub start: usize,
    /// Number of bytes; 0 when the field is not tied to bytes.
    pub length: usize,
    pub children: Vec<Field>,
}
