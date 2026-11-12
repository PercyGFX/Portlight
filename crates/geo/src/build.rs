//! Converts the iptoasn.com table into Portlight's owners file.
//!
//! The input is `ip2asn-combined.tsv`, plain or gzipped: one range per line,
//! `start  end  AS-number  country  description`, IPv4 and IPv6 mixed.
//! Unrouted ranges (AS 0) are left out. Neighbouring ranges with the same
//! owner are joined, and each owner is stored once.

use std::collections::HashMap;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::IpAddr;

use flate2::bufread::MultiGzDecoder;

use crate::names::friendly_name;
use crate::{HEADER_LEN, MAGIC, OWNER_RECORD, V4_RECORD, V6_RECORD, VERSION};

/// What [`build`] wrote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildStats {
    pub v4_ranges: usize,
    pub v6_ranges: usize,
    pub owners: usize,
    /// Size of the file written.
    pub bytes: usize,
}

/// Converts the iptoasn TSV (plain or .gz, told apart by its first bytes)
/// into Portlight's compact file. `built_at` is stored in the header, in Unix
/// seconds.
pub fn build(input: impl Read, mut out: impl Write, built_at: u64) -> io::Result<BuildStats> {
    let mut input = BufReader::new(input);
    let gzipped = input.fill_buf()?.starts_with(&[0x1f, 0x8b]);
    let mut lines: Box<dyn BufRead> =
        if gzipped { Box::new(BufReader::new(MultiGzDecoder::new(input))) } else { Box::new(input) };

    let mut owners = Owners::default();
    let mut v4: Vec<(u32, u32, u32)> = Vec::new();
    let mut v6: Vec<(u128, u128, u32)> = Vec::new();
    let mut line = Vec::new();
    loop {
        line.clear();
        if lines.read_until(b'\n', &mut line)? == 0 {
            break;
        }
        let text = String::from_utf8_lossy(&line);
        let Some(row) = parse_row(text.trim_end_matches(['\r', '\n'])) else { continue };
        let owner = owners.index(row.asn, row.country, row.description);
        match (row.start, row.end) {
            (IpAddr::V4(start), IpAddr::V4(end)) => v4.push((start.into(), end.into(), owner)),
            (IpAddr::V6(start), IpAddr::V6(end)) => v6.push((start.into(), end.into(), owner)),
            _ => {}
        }
    }
    if v4.is_empty() && v6.is_empty() {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "no address ranges found in the input"));
    }
    let v4 = join_ranges(v4);
    let v6 = join_ranges(v6);
    let bytes = encode(&v4, &v6, &owners, built_at)?;
    out.write_all(&bytes)?;
    out.flush()?;
    Ok(BuildStats { v4_ranges: v4.len(), v6_ranges: v6.len(), owners: owners.records.len(), bytes: bytes.len() })
}

struct Row<'a> {
    start: IpAddr,
    end: IpAddr,
    asn: u32,
    country: Option<[u8; 2]>,
    description: &'a str,
}

/// One routed range, or None for unrouted ranges and lines that don't parse.
fn parse_row(line: &str) -> Option<Row<'_>> {
    let mut fields = line.splitn(5, '\t');
    let start = fields.next()?.trim().parse().ok()?;
    let end = fields.next()?.trim().parse().ok()?;
    let asn: u32 = fields.next()?.trim().parse().ok()?;
    let country = fields.next()?.trim();
    let description = fields.next().unwrap_or("").trim();
    if asn == 0 {
        return None;
    }
    // "None" and "Unknown" mean no country.
    let country = match country.as_bytes() {
        [a, b] if a.is_ascii_alphabetic() && b.is_ascii_alphabetic() => {
            Some([a.to_ascii_uppercase(), b.to_ascii_uppercase()])
        }
        _ => None,
    };
    Some(Row { start, end, asn, country, description })
}

/// The owner table and the names it points into, each stored once.
#[derive(Default)]
struct Owners {
    records: Vec<(u32, [u8; 2], u32, u16)>,
    by_key: HashMap<(u32, [u8; 2], String), u32>,
    /// Descriptions already converted, since most repeat on many lines.
    by_description: HashMap<(u32, [u8; 2], String), u32>,
    names: Vec<u8>,
    name_at: HashMap<String, (u32, u16)>,
}

impl Owners {
    fn index(&mut self, asn: u32, country: Option<[u8; 2]>, description: &str) -> u32 {
        let code = country.unwrap_or([0, 0]);
        let raw_key = (asn, code, description.to_owned());
        if let Some(&index) = self.by_description.get(&raw_key) {
            return index;
        }
        let country_text = country.and_then(|c| String::from_utf8(c.to_vec()).ok());
        let mut name = friendly_name(description, country_text.as_deref());
        if name.is_empty() {
            name = format!("AS{asn}");
        }
        let name = truncate(name, u16::MAX as usize);
        let key = (asn, code, name);
        let index = match self.by_key.get(&key) {
            Some(&index) => index,
            None => {
                let (offset, len) = match self.name_at.get(&key.2) {
                    Some(&at) => at,
                    None => {
                        let at = (self.names.len() as u32, key.2.len() as u16);
                        self.names.extend_from_slice(key.2.as_bytes());
                        self.name_at.insert(key.2.clone(), at);
                        at
                    }
                };
                let index = self.records.len() as u32;
                self.records.push((asn, code, offset, len));
                self.by_key.insert(key, index);
                index
            }
        };
        self.by_description.insert(raw_key, index);
        index
    }
}

/// Cuts a string to at most `max` bytes without splitting a character.
fn truncate(mut text: String, max: usize) -> String {
    if text.len() > max {
        let mut cut = max;
        while !text.is_char_boundary(cut) {
            cut -= 1;
        }
        text.truncate(cut);
    }
    text
}

/// An address that has a next one, for telling whether ranges touch.
trait Address: Copy + Ord {
    fn next(self) -> Option<Self>;
}

impl Address for u32 {
    fn next(self) -> Option<Self> {
        self.checked_add(1)
    }
}

impl Address for u128 {
    fn next(self) -> Option<Self> {
        self.checked_add(1)
    }
}

/// Sorts the ranges, joins touching or overlapping ones with the same owner,
/// and trims any overlap between different owners so none remain: the range
/// that starts first keeps the shared addresses.
fn join_ranges<A: Address>(mut ranges: Vec<(A, A, u32)>) -> Vec<(A, A, u32)> {
    ranges.retain(|&(start, end, _)| start <= end);
    ranges.sort_by_key(|&(start, _, _)| start);
    let mut out: Vec<(A, A, u32)> = Vec::with_capacity(ranges.len());
    for (mut start, end, owner) in ranges {
        if let Some(last) = out.last_mut() {
            let touches = start <= last.1 || last.1.next() == Some(start);
            if touches && last.2 == owner {
                last.1 = last.1.max(end);
                continue;
            }
            if start <= last.1 {
                match last.1.next() {
                    Some(after) if end >= after => start = after,
                    _ => continue,
                }
            }
        }
        out.push((start, end, owner));
    }
    out
}

fn encode(v4: &[(u32, u32, u32)], v6: &[(u128, u128, u32)], owners: &Owners, built_at: u64) -> io::Result<Vec<u8>> {
    let v4_at = HEADER_LEN;
    let v6_at = v4_at + v4.len() * V4_RECORD;
    let owners_at = v6_at + v6.len() * V6_RECORD;
    let names_at = owners_at + owners.records.len() * OWNER_RECORD;
    let total = names_at + owners.names.len();
    let too_big = || io::Error::new(io::ErrorKind::InvalidData, "the owners file would be over 4 GB");
    let small = |n: usize| u32::try_from(n).map_err(|_| too_big());
    small(total)?;

    let mut bytes = Vec::with_capacity(total);
    bytes.extend_from_slice(MAGIC);
    bytes.extend_from_slice(&VERSION.to_le_bytes());
    bytes.extend_from_slice(&(HEADER_LEN as u32).to_le_bytes());
    bytes.extend_from_slice(&built_at.to_le_bytes());
    for (at, count) in [
        (v4_at, v4.len()),
        (v6_at, v6.len()),
        (owners_at, owners.records.len()),
        (names_at, owners.names.len()),
    ] {
        bytes.extend_from_slice(&small(at)?.to_le_bytes());
        bytes.extend_from_slice(&small(count)?.to_le_bytes());
    }
    bytes.resize(HEADER_LEN, 0);
    for &(start, end, owner) in v4 {
        bytes.extend_from_slice(&start.to_le_bytes());
        bytes.extend_from_slice(&end.to_le_bytes());
        bytes.extend_from_slice(&owner.to_le_bytes());
    }
    for &(start, end, owner) in v6 {
        bytes.extend_from_slice(&start.to_le_bytes());
        bytes.extend_from_slice(&end.to_le_bytes());
        bytes.extend_from_slice(&owner.to_le_bytes());
    }
    for &(asn, country, offset, len) in &owners.records {
        bytes.extend_from_slice(&asn.to_le_bytes());
        bytes.extend_from_slice(&country);
        bytes.extend_from_slice(&offset.to_le_bytes());
        bytes.extend_from_slice(&len.to_le_bytes());
    }
    bytes.extend_from_slice(&owners.names);
    debug_assert_eq!(bytes.len(), total);
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::join_ranges;

    #[test]
    fn touching_ranges_with_one_owner_are_joined() {
        let joined = join_ranges(vec![(10u32, 19, 1), (0, 9, 1), (20, 29, 2), (30, 39, 2), (41, 50, 2)]);
        assert_eq!(joined, vec![(0, 19, 1), (20, 39, 2), (41, 50, 2)]);
    }

    #[test]
    fn overlaps_between_owners_are_trimmed() {
        let joined = join_ranges(vec![(0u32, 20, 1), (10, 30, 2), (12, 15, 3), (5, 8, 1)]);
        assert_eq!(joined, vec![(0, 20, 1), (21, 30, 2)]);
    }

    #[test]
    fn the_top_address_does_not_overflow() {
        let joined = join_ranges(vec![(0u32, u32::MAX, 1), (5, u32::MAX, 2)]);
        assert_eq!(joined, vec![(0, u32::MAX, 1)]);
    }
}
