//! TCP: reliable connections between two programs.

use std::fmt::Write;

use crate::bytes::{u8_at, u16_at, u32_at};
use crate::names::port_value;
use crate::summary::{Desc, Who};
use crate::text::{byte_count, thousands};
use crate::{Field, Layer};

pub(crate) const FIN: u16 = 0x001;
pub(crate) const SYN: u16 = 0x002;
pub(crate) const RST: u16 = 0x004;
pub(crate) const PSH: u16 = 0x008;
pub(crate) const ACK: u16 = 0x010;
pub(crate) const URG: u16 = 0x020;
pub(crate) const ECE: u16 = 0x040;
pub(crate) const CWR: u16 = 0x080;
pub(crate) const AE: u16 = 0x100;

/// Flag names in Wireshark's order, with what each means.
const FLAGS: [(u16, &str, &str); 9] = [
    (FIN, "FIN", "The sender has nothing more to send and wants to end the connection."),
    (
        SYN,
        "SYN",
        "Asks to open a connection (or agrees to, together with ACK) and sets the starting number for the data.",
    ),
    (
        RST,
        "RST",
        "Ends the connection at once, without the polite goodbye. Sent when something went wrong or a port is closed.",
    ),
    (PSH, "PSH", "Asks the receiver to hand the data to the program now instead of waiting for more."),
    (ACK, "ACK", "The acknowledgment number is valid: the sender confirms what it has received so far."),
    (URG, "URG", "Marks urgent data. Almost never used today."),
    (ECE, "ECE", "Congestion signal: the network is getting busy, please slow down."),
    (CWR, "CWR", "The sender has slowed down in answer to a congestion signal."),
    (AE, "AE", "Accurate congestion signalling (rarely used)."),
];

#[derive(Debug, Clone, Copy)]
pub(crate) struct Tcp {
    pub at: usize,
    pub src_port: u16,
    pub dst_port: u16,
    pub seq: u32,
    pub ack: u32,
    pub header_len: usize,
    pub flags: u16,
    pub window: u16,
    pub payload_at: usize,
    pub end: usize,
}

impl Tcp {
    pub(crate) fn has(&self, flag: u16) -> bool {
        self.flags & flag != 0
    }

    pub(crate) fn payload_len(&self) -> usize {
        self.end.saturating_sub(self.payload_at)
    }

    pub(crate) fn has_port(&self, port: u16) -> bool {
        self.src_port == port || self.dst_port == port
    }
}

/// Reads a TCP header that ends no later than `end` (the end of the IP packet).
pub(crate) fn parse(frame: &[u8], at: usize, end: usize) -> Option<Tcp> {
    let b = frame.get(..end)?;
    let off_flags = u16_at(b, at + 12)?;
    // A data offset below 5 is invalid; read it as the minimum header.
    let header_len = (usize::from(off_flags >> 12) * 4).max(20);
    let tcp = Tcp {
        at,
        src_port: u16_at(b, at)?,
        dst_port: u16_at(b, at + 2)?,
        seq: u32_at(b, at + 4)?,
        ack: u32_at(b, at + 8)?,
        header_len,
        flags: off_flags & 0x01ff,
        window: u16_at(b, at + 14)?,
        payload_at: (at + header_len).min(end),
        end,
    };
    u16_at(b, at + 18)?;
    Some(tcp)
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Opt {
    pub kind: u8,
    pub at: usize,
    /// The whole option, kind and length bytes included.
    pub len: usize,
    /// The length byte was impossible or the option runs past the header.
    pub bad: bool,
}

/// The options between the fixed header and the payload.
pub(crate) fn options(frame: &[u8], t: &Tcp) -> Vec<Opt> {
    let end = (t.at + t.header_len).min(t.end).min(frame.len());
    let mut out = Vec::new();
    let mut pos = t.at + 20;
    while pos < end && out.len() < 40 {
        let Some(kind) = u8_at(frame, pos) else { break };
        match kind {
            0 => {
                out.push(Opt { kind, at: pos, len: 1, bad: false });
                break;
            }
            1 => {
                out.push(Opt { kind, at: pos, len: 1, bad: false });
                pos += 1;
            }
            _ => {
                let len = u8_at(frame, pos + 1).map_or(0, usize::from);
                if len < 2 || pos + len > end {
                    out.push(Opt { kind, at: pos, len: end - pos, bad: true });
                    break;
                }
                out.push(Opt { kind, at: pos, len, bad: false });
                pos += len;
            }
        }
    }
    out
}

/// "[SYN, ACK]".
pub(crate) fn flag_list(flags: u16) -> String {
    let names: Vec<&str> = FLAGS.iter().filter(|(bit, _, _)| flags & bit != 0).map(|(_, name, _)| *name).collect();
    if names.is_empty() { "<None>".to_string() } else { names.join(", ") }
}

/// Short option notes for the info line: "MSS=1460 WS=256 SACK_PERM".
fn option_info(frame: &[u8], t: &Tcp) -> String {
    let mut out = String::new();
    for opt in options(frame, t) {
        if opt.bad {
            continue;
        }
        match (opt.kind, opt.len) {
            (2, 4) => {
                let _ = write!(out, " MSS={}", u16_at(frame, opt.at + 2).unwrap_or(0));
            }
            (3, 3) => {
                let shift = u8_at(frame, opt.at + 2).unwrap_or(0).min(14);
                let _ = write!(out, " WS={}", 1u32 << shift);
            }
            (4, 2) => out.push_str(" SACK_PERM"),
            (5, len) if len >= 10 => {
                let _ = write!(
                    out,
                    " SLE={} SRE={}",
                    u32_at(frame, opt.at + 2).unwrap_or(0),
                    u32_at(frame, opt.at + 6).unwrap_or(0)
                );
            }
            (8, 10) => {
                let _ = write!(
                    out,
                    " TSval={} TSecr={}",
                    u32_at(frame, opt.at + 2).unwrap_or(0),
                    u32_at(frame, opt.at + 6).unwrap_or(0)
                );
            }
            _ => {}
        }
    }
    out
}

/// "51234 → 443 [SYN] Seq=0 Win=64240 Len=0 MSS=1460".
pub(crate) fn info(frame: &[u8], t: &Tcp) -> String {
    let mut s = format!("{} → {} [{}] Seq={}", t.src_port, t.dst_port, flag_list(t.flags), t.seq);
    if t.has(ACK) {
        let _ = write!(s, " Ack={}", t.ack);
    }
    let _ = write!(s, " Win={} Len={}", t.window, t.payload_len());
    s.push_str(&option_info(frame, t));
    s
}

pub(crate) fn describe(frame: &[u8], t: &Tcp, who: &Who) -> Desc {
    let len = t.payload_len();
    let (src, dst) = (&who.src, &who.dst);
    let summary = if t.has(RST) {
        format!("Connection reset — {src} gave up abruptly")
    } else if t.has(SYN) && !t.has(ACK) {
        format!("{src} asks {dst} to open a connection")
    } else if t.has(SYN) {
        format!("{src} agrees to the connection")
    } else if t.has(FIN) && len > 0 {
        format!("{src} sends {} to {dst} and ends the connection", byte_count(len))
    } else if t.has(FIN) {
        format!("{src} ends the connection")
    } else if len > 0 {
        format!("{src} sends {} to {dst}", byte_count(len))
    } else if t.window == 0 && t.has(ACK) {
        format!("{src} is full and asks {dst} to wait")
    } else if t.has(ACK) {
        format!("{src} confirms to {dst} that everything arrived")
    } else {
        format!("{src} sends a TCP packet to {dst}")
    };
    Desc::new("TCP", info(frame, t), summary)
}

pub(crate) fn layer(frame: &[u8], t: &Tcp) -> Layer {
    let at = t.at;
    let len = t.payload_len();
    let mut summary = format!("From port {} to port {} [{}]", t.src_port, port_value(t.dst_port), flag_list(t.flags));
    if len > 0 {
        let _ = write!(summary, ", {} of data", byte_count(len));
    }
    let mut layer = Layer::new(
        "tcp",
        "Transmission Control Protocol (TCP)",
        summary,
        "TCP keeps a conversation between two programs reliable: it numbers the data, resends anything lost \
         and puts it back in order. Ports say which program on each side the data is for.",
        at,
        (at + t.header_len).min(t.end),
    );
    layer.push(Field::new("Source port", port_value(t.src_port), at, 2).explain(
        "Which program on the sending computer this belongs to. Apps usually get a random high number; \
         servers use a fixed, well-known one.",
    ));
    layer.push(Field::new("Destination port", port_value(t.dst_port), at + 2, 2).explain(
        "Which program on the receiving computer this is for. Well-known numbers name the service: \
         443 is HTTPS (secure web), 80 is plain web.",
    ));
    layer
        .push(Field::new("Sequence number", t.seq.to_string(), at + 4, 4).explain(
            "Numbers the bytes sent, so the receiver can put data back in order and notice anything missing.",
        ));
    if t.has(ACK) {
        layer.push(
            Field::new("Acknowledgment number", t.ack.to_string(), at + 8, 4)
                .explain("The next byte number expected from the other side: everything before it arrived."),
        );
    }
    layer.push(Field::new("Header length", byte_count(t.header_len), at + 12, 1));
    let children = FLAGS
        .iter()
        .rev()
        .map(|&(bit, name, explain)| {
            let (start, length) = if bit == AE { (at + 12, 1) } else { (at + 13, 1) };
            Field::new(name, if t.flags & bit != 0 { "Set" } else { "Not set" }, start, length).explain(explain)
        })
        .collect();
    layer.push(
        Field::new("Flags", format!("0x{:03x} ({})", t.flags, flag_list(t.flags)), at + 12, 2)
            .explain("Switches that say what this packet does: open (SYN), confirm (ACK), close (FIN) or abort (RST) a connection.")
            .children(children),
    );
    layer.push(
        Field::new("Window", t.window.to_string(), at + 14, 2)
            .explain("How much more data the sender can take in right now. 0 means \"wait, I'm full\"."),
    );
    if let Some(sum) = u16_at(frame, at + 16) {
        layer.push(crate::ip::checksum_field(at + 16, sum));
    }
    if let Some(urgent) = u16_at(frame, at + 18)
        && (t.has(URG) || urgent != 0)
    {
        layer.push(Field::new("Urgent pointer", urgent.to_string(), at + 18, 2));
    }
    let opts = options(frame, t);
    if !opts.is_empty() {
        let start = at + 20;
        let end = opts.last().map_or(start, |o| o.at + o.len);
        let children: Vec<Field> = opts.iter().map(|o| option_field(frame, o)).collect();
        layer.push(
            Field::new("Options", byte_count(end - start), start, end - start)
                .explain("Extra settings, mostly agreed when the connection opens.")
                .children(children),
        );
    }
    if len > 0 {
        layer.push(
            Field::new("Payload", byte_count(len), t.payload_at, len)
                .explain("The data this packet carries for the program."),
        );
    }
    layer
}

fn option_field(frame: &[u8], o: &Opt) -> Field {
    if o.bad {
        return Field::new("Damaged option", format!("kind {}", o.kind), o.at, o.len)
            .explain("This option's length does not fit, so the rest of the options cannot be read.");
    }
    let num16 = |off: usize| u16_at(frame, o.at + off).unwrap_or(0);
    let num32 = |off: usize| u32_at(frame, o.at + off).unwrap_or(0);
    match (o.kind, o.len) {
        (0, _) => Field::new("End of options", "", o.at, 1),
        (1, _) => Field::new("No-operation", "padding", o.at, 1).explain("Filler that lines the next option up."),
        (2, 4) => Field::new("Maximum segment size", format!("{} bytes", thousands(u64::from(num16(2)))), o.at, 4)
            .explain("The largest piece of data this side wants to receive in one packet."),
        (3, 3) => {
            let shift = u8_at(frame, o.at + 2).unwrap_or(0).min(14);
            Field::new("Window scale", format!("{shift} (multiply by {})", 1u32 << shift), o.at, 3)
                .explain("Lets the window grow beyond 64 KB: window values are multiplied by this.")
        }
        (4, 2) => Field::new("SACK permitted", "Yes", o.at, 2)
            .explain("This side can say exactly which pieces arrived, so only missing pieces are sent again."),
        (5, len) => {
            let blocks: Vec<String> =
                (0..(len - 2) / 8).map(|i| format!("{}–{}", num32(2 + i * 8), num32(6 + i * 8))).collect();
            Field::new("Selective acknowledgment", blocks.join(", "), o.at, len)
                .explain("Ranges of data that arrived even though something before them is missing.")
        }
        (8, 10) => Field::new("Timestamps", format!("TSval={}, TSecr={}", num32(2), num32(6)), o.at, 10)
            .explain("Helps measure how long a round trip takes and spot old, repeated packets."),
        (28, 4) => {
            let v = num16(2);
            let unit = if v & 0x8000 != 0 { "minutes" } else { "seconds" };
            Field::new("User timeout", format!("{} {unit}", v & 0x7fff), o.at, 4)
        }
        (30, len) => Field::new("Multipath TCP", byte_count(len), o.at, len)
            .explain("Lets one connection use several network paths at once (e.g. Wi-Fi and mobile data)."),
        (34, len) => Field::new("TCP Fast Open cookie", byte_count(len), o.at, len)
            .explain("Lets a returning client send data with its very first packet."),
        (kind, len) => Field::new("Option", format!("kind {kind}, {}", byte_count(len)), o.at, len),
    }
}
