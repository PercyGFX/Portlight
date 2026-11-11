//! Checked reads from untrusted bytes: every read past the end is `None`.

pub(crate) fn slice(b: &[u8], at: usize, len: usize) -> Option<&[u8]> {
    b.get(at..at.checked_add(len)?)
}

pub(crate) fn u8_at(b: &[u8], at: usize) -> Option<u8> {
    b.get(at).copied()
}

pub(crate) fn u16_at(b: &[u8], at: usize) -> Option<u16> {
    let s = slice(b, at, 2)?;
    Some(u16::from_be_bytes([s[0], s[1]]))
}

pub(crate) fn u24_at(b: &[u8], at: usize) -> Option<u32> {
    let s = slice(b, at, 3)?;
    Some(u32::from_be_bytes([0, s[0], s[1], s[2]]))
}

pub(crate) fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    let s = slice(b, at, 4)?;
    Some(u32::from_be_bytes([s[0], s[1], s[2], s[3]]))
}

pub(crate) fn u32_le_at(b: &[u8], at: usize) -> Option<u32> {
    let s = slice(b, at, 4)?;
    Some(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

pub(crate) fn array<const N: usize>(b: &[u8], at: usize) -> Option<[u8; N]> {
    slice(b, at, N)?.try_into().ok()
}

pub(crate) fn ipv4_at(b: &[u8], at: usize) -> Option<std::net::Ipv4Addr> {
    array::<4>(b, at).map(std::net::Ipv4Addr::from)
}

pub(crate) fn ipv6_at(b: &[u8], at: usize) -> Option<std::net::Ipv6Addr> {
    array::<16>(b, at).map(std::net::Ipv6Addr::from)
}

/// The bytes from `at` to `end`, or nothing when the range is out of bounds.
pub(crate) fn range(b: &[u8], at: usize, end: usize) -> &[u8] {
    b.get(at..end).unwrap_or(&[])
}
