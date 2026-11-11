//! Writes pcapng capture files, the format Wireshark opens by default.
//!
//! A file is one section: a Section Header Block, one Interface Description
//! Block per network card, then an Enhanced Packet Block per packet. Blocks
//! are written little-endian, with timestamps in microseconds.

use std::io::{self, Write};

use crate::LinkType;

const SHB: u32 = 0x0a0d_0d0a;
const IDB: u32 = 0x0000_0001;
const EPB: u32 = 0x0000_0006;
const BYTE_ORDER_MAGIC: u32 = 0x1a2b_3c4d;

const OPT_END: u16 = 0;
const OPT_COMMENT: u16 = 1;
const SHB_USERAPPL: u16 = 4;
const IF_NAME: u16 = 2;
const IF_DESCRIPTION: u16 = 3;
const IF_TSRESOL: u16 = 9;

/// Writes one pcapng section to `W`.
pub struct Writer<W: Write> {
    out: W,
    interfaces: u32,
}

impl<W: Write> Writer<W> {
    /// Starts the file with a Section Header Block naming the application.
    pub fn new(mut out: W, application: &str) -> io::Result<Self> {
        let mut body = Vec::new();
        put_u32(&mut body, BYTE_ORDER_MAGIC);
        put_u16(&mut body, 1);
        put_u16(&mut body, 0);
        // Section length unknown.
        body.extend_from_slice(&(-1i64).to_le_bytes());
        let mut opts = Options::default();
        opts.text(SHB_USERAPPL, application);
        body.extend(opts.finish());
        write_block(&mut out, SHB, &body)?;
        Ok(Writer { out, interfaces: 0 })
    }

    /// Describes a network card. Returns its id for [`Writer::write_packet`]: 0, 1, 2...
    pub fn add_interface(&mut self, link: LinkType, snaplen: u32, name: &str, description: &str) -> io::Result<u32> {
        let link_type = u16::try_from(link.dlt())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "link type does not fit a pcapng file"))?;
        let mut body = Vec::new();
        put_u16(&mut body, link_type);
        put_u16(&mut body, 0);
        put_u32(&mut body, snaplen);
        let mut opts = Options::default();
        opts.text(IF_NAME, name);
        opts.text(IF_DESCRIPTION, description);
        // 10^-6: timestamps are in microseconds.
        opts.raw(IF_TSRESOL, &[6]);
        body.extend(opts.finish());
        write_block(&mut self.out, IDB, &body)?;
        let id = self.interfaces;
        self.interfaces += 1;
        Ok(id)
    }

    /// Writes one packet. `comment` becomes the frame comment Wireshark shows.
    pub fn write_packet(
        &mut self,
        interface: u32,
        timestamp_us: u64,
        original_len: u32,
        data: &[u8],
        comment: Option<&str>,
    ) -> io::Result<()> {
        if interface >= self.interfaces {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "unknown interface id"));
        }
        let captured =
            u32::try_from(data.len()).map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "packet too large"))?;
        let mut body = Vec::with_capacity(data.len() + 48);
        put_u32(&mut body, interface);
        put_u32(&mut body, (timestamp_us >> 32) as u32);
        put_u32(&mut body, timestamp_us as u32);
        put_u32(&mut body, captured);
        put_u32(&mut body, original_len.max(captured));
        body.extend_from_slice(data);
        pad(&mut body);
        if let Some(c) = comment.filter(|c| !c.is_empty()) {
            let mut opts = Options::default();
            opts.text(OPT_COMMENT, c);
            body.extend(opts.finish());
        }
        write_block(&mut self.out, EPB, &body)
    }

    /// Flushes and hands back the output.
    pub fn finish(mut self) -> io::Result<W> {
        self.out.flush()?;
        Ok(self.out)
    }
}

/// A block's options, each padded to 4 bytes, ended by opt_endofopt.
#[derive(Default)]
struct Options {
    bytes: Vec<u8>,
    any: bool,
}

impl Options {
    fn raw(&mut self, code: u16, value: &[u8]) {
        let len = value.len().min(usize::from(u16::MAX));
        put_u16(&mut self.bytes, code);
        put_u16(&mut self.bytes, len as u16);
        self.bytes.extend_from_slice(&value[..len]);
        pad(&mut self.bytes);
        self.any = true;
    }

    /// A UTF-8 option, cut at a character boundary if it is too long.
    fn text(&mut self, code: u16, value: &str) {
        if value.is_empty() {
            return;
        }
        let mut end = value.len().min(usize::from(u16::MAX) - 3);
        while !value.is_char_boundary(end) {
            end -= 1;
        }
        self.raw(code, &value.as_bytes()[..end]);
    }

    fn finish(mut self) -> Vec<u8> {
        if self.any {
            put_u16(&mut self.bytes, OPT_END);
            put_u16(&mut self.bytes, 0);
        }
        self.bytes
    }
}

fn write_block(out: &mut impl Write, kind: u32, body: &[u8]) -> io::Result<()> {
    let total =
        u32::try_from(body.len() + 12).map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "block too large"))?;
    let mut block = Vec::with_capacity(body.len() + 12);
    put_u32(&mut block, kind);
    put_u32(&mut block, total);
    block.extend_from_slice(body);
    put_u32(&mut block, total);
    out.write_all(&block)
}

fn pad(v: &mut Vec<u8>) {
    while !v.len().is_multiple_of(4) {
        v.push(0);
    }
}

fn put_u16(v: &mut Vec<u8>, n: u16) {
    v.extend_from_slice(&n.to_le_bytes());
}

fn put_u32(v: &mut Vec<u8>, n: u32) {
    v.extend_from_slice(&n.to_le_bytes());
}
