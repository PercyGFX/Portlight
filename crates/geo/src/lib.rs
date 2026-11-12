//! Who owns an IP address, and which country its network is registered in.
//!
//! Everything is looked up on this PC: nothing is sent anywhere. The data comes
//! from iptoasn.com (public domain, PDDL v1.0), converted by [`build`] into a
//! small file that [`OwnerDb`] searches in place, without parsing it first.
//!
//! The country is where the network is registered, not necessarily where the
//! server is.
//!
//! # File format (version 1, little endian)
//!
//! | Part | Contents |
//! |---|---|
//! | Header, 64 bytes | `PLOWNERS`, version u32, header size u32, built_at u64, then offset u32 and count u32 for the IPv4, IPv6 and owner tables, then the names' offset u32 and length u32, then zeros |
//! | IPv4 ranges, 12 bytes each | start u32, end u32 (inclusive), owner index u32 |
//! | IPv6 ranges, 36 bytes each | start u128, end u128 (inclusive), owner index u32 |
//! | Owners, 12 bytes each | AS number u32, country `[u8; 2]` (zeros when unknown), name offset u32, name length u16 |
//! | Names | UTF-8 text the owners point into |
//!
//! Ranges are sorted and never overlap, so a lookup is one binary search.

mod build;
mod names;

use std::fs;
use std::io;
use std::net::IpAddr;
use std::ops::Range;
use std::path::Path;

use serde::Serialize;

pub use build::{BuildStats, build};
pub use names::friendly_name;

const MAGIC: &[u8; 8] = b"PLOWNERS";
const VERSION: u32 = 1;
const HEADER_LEN: usize = 64;
const V4_RECORD: usize = 12;
const V6_RECORD: usize = 36;
const OWNER_RECORD: usize = 12;

/// What is known about the network an address belongs to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkOwner {
    /// The network's autonomous system number, e.g. 13335.
    pub asn: u32,
    /// Friendly owner name, e.g. "Cloudflare, Inc.".
    pub name: String,
    /// ISO 3166-1 alpha-2, upper case, e.g. "US". None when unknown.
    pub country: Option<String>,
}

/// The owners file, loaded into memory and checked once.
pub struct OwnerDb {
    bytes: Vec<u8>,
    built_at: u64,
    v4: Range<usize>,
    v6: Range<usize>,
    owners: Range<usize>,
    names: Range<usize>,
}

impl OwnerDb {
    /// Reads the whole file once.
    pub fn open(path: &Path) -> io::Result<OwnerDb> {
        OwnerDb::from_bytes(fs::read(path)?)
    }

    /// Checks the bytes are a whole, sound owners file. Anything damaged is an
    /// error, so lookups never need to check again.
    pub fn from_bytes(bytes: Vec<u8>) -> io::Result<OwnerDb> {
        let header = bytes.get(..HEADER_LEN).ok_or_else(|| bad("the file is too short"))?;
        if &header[..8] != MAGIC {
            return Err(bad("this is not a Portlight owners file"));
        }
        let version = u32_at(header, 8);
        if version != VERSION {
            return Err(bad(&format!("owners file version {version} is not supported")));
        }
        if u32_at(header, 12) as usize != HEADER_LEN {
            return Err(bad("the header size is wrong"));
        }
        let built_at = u64_at(header, 16);
        let section = |at: usize, size: usize| -> io::Result<Range<usize>> {
            let start = u32_at(header, at) as usize;
            let len = (u32_at(header, at + 4) as usize)
                .checked_mul(size)
                .ok_or_else(|| bad("a table is too large"))?;
            let end = start.checked_add(len).ok_or_else(|| bad("a table is too large"))?;
            if start < HEADER_LEN || end > bytes.len() {
                return Err(bad("a table lies outside the file"));
            }
            Ok(start..end)
        };
        let db = OwnerDb {
            built_at,
            v4: section(24, V4_RECORD)?,
            v6: section(32, V6_RECORD)?,
            owners: section(40, OWNER_RECORD)?,
            names: section(48, 1)?,
            bytes,
        };
        db.check()?;
        Ok(db)
    }

    /// The owner of the network `ip` belongs to, or None for private,
    /// reserved and unrouted addresses. IPv4 written as IPv6
    /// (`::ffff:1.1.1.1`) is looked up as IPv4.
    pub fn lookup(&self, ip: IpAddr) -> Option<NetworkOwner> {
        let owner = match ip {
            IpAddr::V4(v4) => self.find_v4(u32::from(v4)),
            IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
                Some(v4) => self.find_v4(u32::from(v4)),
                None => self.find_v6(u128::from(v6)),
            },
        }?;
        self.owner(owner)
    }

    /// When the file was built, in Unix seconds.
    pub fn built_at(&self) -> u64 {
        self.built_at
    }

    /// How many IPv4 and IPv6 ranges the file holds.
    pub fn len(&self) -> (usize, usize) {
        (self.v4.len() / V4_RECORD, self.v6.len() / V6_RECORD)
    }

    /// True when the file holds no ranges at all.
    pub fn is_empty(&self) -> bool {
        self.v4.is_empty() && self.v6.is_empty()
    }

    fn owner_count(&self) -> usize {
        self.owners.len() / OWNER_RECORD
    }

    /// Checks every range and owner once, so lookups can trust them.
    fn check(&self) -> io::Result<()> {
        let owners = self.owner_count();
        let v4 = &self.bytes[self.v4.clone()];
        let mut previous_end: Option<u32> = None;
        for record in v4.as_chunks::<V4_RECORD>().0 {
            let (start, end, owner) = (u32_at(record, 0), u32_at(record, 4), u32_at(record, 8));
            if start > end || previous_end.is_some_and(|previous| start <= previous) {
                return Err(bad("the IPv4 ranges are out of order"));
            }
            if owner as usize >= owners {
                return Err(bad("an IPv4 range points to a missing owner"));
            }
            previous_end = Some(end);
        }
        let v6 = &self.bytes[self.v6.clone()];
        let mut previous_end: Option<u128> = None;
        for record in v6.as_chunks::<V6_RECORD>().0 {
            let (start, end, owner) = (u128_at(record, 0), u128_at(record, 16), u32_at(record, 32));
            if start > end || previous_end.is_some_and(|previous| start <= previous) {
                return Err(bad("the IPv6 ranges are out of order"));
            }
            if owner as usize >= owners {
                return Err(bad("an IPv6 range points to a missing owner"));
            }
            previous_end = Some(end);
        }
        let names = std::str::from_utf8(&self.bytes[self.names.clone()]).map_err(|_| bad("a name is not valid text"))?;
        for record in self.bytes[self.owners.clone()].as_chunks::<OWNER_RECORD>().0 {
            let country = &record[4..6];
            if country != [0, 0] && !country.iter().all(u8::is_ascii_uppercase) {
                return Err(bad("an owner has a damaged country"));
            }
            let start = u32_at(record, 6) as usize;
            let len = u16_at(record, 10) as usize;
            if start.checked_add(len).and_then(|end| names.get(start..end)).is_none() {
                return Err(bad("an owner's name lies outside the names"));
            }
        }
        Ok(())
    }

    /// Binary search: the last range starting at or before `ip`, if it reaches `ip`.
    fn find_v4(&self, ip: u32) -> Option<u32> {
        let table = &self.bytes[self.v4.clone()];
        let (mut low, mut high) = (0, table.len() / V4_RECORD);
        while low < high {
            let middle = low + (high - low) / 2;
            if u32_at(table, middle * V4_RECORD) <= ip {
                low = middle + 1;
            } else {
                high = middle;
            }
        }
        let at = low.checked_sub(1)? * V4_RECORD;
        (ip <= u32_at(table, at + 4)).then(|| u32_at(table, at + 8))
    }

    fn find_v6(&self, ip: u128) -> Option<u32> {
        let table = &self.bytes[self.v6.clone()];
        let (mut low, mut high) = (0, table.len() / V6_RECORD);
        while low < high {
            let middle = low + (high - low) / 2;
            if u128_at(table, middle * V6_RECORD) <= ip {
                low = middle + 1;
            } else {
                high = middle;
            }
        }
        let at = low.checked_sub(1)? * V6_RECORD;
        (ip <= u128_at(table, at + 16)).then(|| u32_at(table, at + 32))
    }

    fn owner(&self, index: u32) -> Option<NetworkOwner> {
        let table = &self.bytes[self.owners.clone()];
        let record = table.get(index as usize * OWNER_RECORD..)?.get(..OWNER_RECORD)?;
        let country = match &record[4..6] {
            [0, 0] => None,
            code => std::str::from_utf8(code).ok().map(str::to_owned),
        };
        let start = u32_at(record, 6) as usize;
        let len = u16_at(record, 10) as usize;
        let names = &self.bytes[self.names.clone()];
        let name = std::str::from_utf8(names.get(start..start.checked_add(len)?)?).ok()?;
        Some(NetworkOwner { asn: u32_at(record, 0), name: name.to_owned(), country })
    }
}

fn bad(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, format!("owners file: {message}"))
}

/// Reads a little-endian number. Callers stay inside checked tables; outside
/// one, the missing bytes read as zero rather than panicking.
fn read<const N: usize>(bytes: &[u8], at: usize) -> [u8; N] {
    let mut out = [0; N];
    if let Some(slice) = bytes.get(at..at.saturating_add(N)) {
        out.copy_from_slice(slice);
    }
    out
}

fn u16_at(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes(read(bytes, at))
}

fn u32_at(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(read(bytes, at))
}

fn u64_at(bytes: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(read(bytes, at))
}

fn u128_at(bytes: &[u8], at: usize) -> u128 {
    u128::from_le_bytes(read(bytes, at))
}
