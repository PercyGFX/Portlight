//! pcapng output, read back block by block.

use super::samples;
use crate::LinkType;
use crate::pcapng::Writer;

fn u16le(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}

fn u32le(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

struct Block {
    kind: u32,
    body: Vec<u8>,
}

/// Splits a file into blocks, checking lengths and trailers.
fn blocks(file: &[u8]) -> Vec<Block> {
    let mut out = Vec::new();
    let mut pos = 0;
    while pos < file.len() {
        let kind = u32le(file, pos);
        let len = u32le(file, pos + 4) as usize;
        assert_eq!(len % 4, 0, "block length is a multiple of 4");
        assert!(len >= 12);
        assert_eq!(u32le(file, pos + len - 4) as usize, len, "trailer repeats the length");
        out.push(Block { kind, body: file[pos + 8..pos + len - 4].to_vec() });
        pos += len;
    }
    assert_eq!(pos, file.len());
    out
}

/// (code, value) pairs, checking each is padded and the list ends with opt_endofopt.
fn options(b: &[u8]) -> Vec<(u16, Vec<u8>)> {
    let mut out = Vec::new();
    let mut pos = 0;
    loop {
        let code = u16le(b, pos);
        let len = usize::from(u16le(b, pos + 2));
        if code == 0 {
            assert_eq!(len, 0);
            assert_eq!(pos + 4, b.len(), "opt_endofopt ends the block");
            return out;
        }
        out.push((code, b[pos + 4..pos + 4 + len].to_vec()));
        pos += 4 + len.div_ceil(4) * 4;
    }
}

#[test]
fn writes_a_file_wireshark_can_read() {
    let syn = samples::syn();
    let ping = samples::ping();
    let mut w = Writer::new(Vec::new(), "Portlight 0.3.0").unwrap();
    assert_eq!(w.add_interface(LinkType::Ethernet, 65535, "Wi-Fi", "Intel(R) Wi-Fi 6 AX201").unwrap(), 0);
    assert_eq!(
        w.add_interface(LinkType::Null, 262_144, "Loopback", "Adapter for loopback traffic capture").unwrap(),
        1
    );
    w.write_packet(0, 1_762_943_465_123_456, 66, &syn, Some("Google Chrome")).unwrap();
    w.write_packet(0, 1_762_943_465_200_000, 1500, &ping[..61], None).unwrap();
    assert!(w.write_packet(5, 0, 0, &ping, None).is_err());
    let file = w.finish().unwrap();

    let blocks = blocks(&file);
    assert_eq!(blocks.iter().map(|b| b.kind).collect::<Vec<_>>(), [0x0a0d0d0a, 1, 1, 6, 6]);

    let shb = &blocks[0].body;
    assert_eq!(u32le(shb, 0), 0x1a2b3c4d);
    assert_eq!((u16le(shb, 4), u16le(shb, 6)), (1, 0));
    assert_eq!(&shb[8..16], &[0xff; 8]);
    assert_eq!(options(&shb[16..]), [(4, b"Portlight 0.3.0".to_vec())]);

    let idb = &blocks[1].body;
    assert_eq!((u16le(idb, 0), u32le(idb, 4)), (1, 65535));
    assert_eq!(options(&idb[8..]), [(2, b"Wi-Fi".to_vec()), (3, b"Intel(R) Wi-Fi 6 AX201".to_vec()), (9, vec![6])]);
    assert_eq!(u16le(&blocks[2].body, 0), 0, "loopback is link type 0");

    let epb = &blocks[3].body;
    assert_eq!(u32le(epb, 0), 0);
    let ts = (u64::from(u32le(epb, 4)) << 32) | u64::from(u32le(epb, 8));
    assert_eq!(ts, 1_762_943_465_123_456);
    assert_eq!((u32le(epb, 12), u32le(epb, 16)), (66, 66));
    assert_eq!(&epb[20..86], &syn[..]);
    assert_eq!(options(&epb[88..]), [(1, b"Google Chrome".to_vec())]);

    let cut = &blocks[4].body;
    assert_eq!((u32le(cut, 12), u32le(cut, 16)), (61, 1500));
    assert_eq!(&cut[20..81], &ping[..61]);
    assert_eq!(cut.len(), 20 + 64, "data padded to 4 bytes, no options");
}
