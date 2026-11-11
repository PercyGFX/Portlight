//! Small text helpers shared by the protocol modules.

use std::fmt::Write;

/// 1240 → "1,240".
pub(crate) fn thousands(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// "1 byte", "1,240 bytes".
pub(crate) fn byte_count(n: usize) -> String {
    if n == 1 { "1 byte".to_string() } else { format!("{} bytes", thousands(n as u64)) }
}

pub(crate) fn plural(n: usize, one: &str, many: &str) -> String {
    if n == 1 { format!("1 {one}") } else { format!("{} {many}", thousands(n as u64)) }
}

pub(crate) fn mac(m: &[u8; 6]) -> String {
    format!("{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}", m[0], m[1], m[2], m[3], m[4], m[5])
}

/// Bytes as hex, cut after `max` bytes with "…".
pub(crate) fn hex(b: &[u8], max: usize) -> String {
    let mut out = String::with_capacity(b.len().min(max) * 2 + 3);
    for byte in b.iter().take(max) {
        let _ = write!(out, "{byte:02x}");
    }
    if b.len() > max {
        out.push('…');
    }
    out
}

/// Bytes as text: invalid UTF-8 and control characters are replaced, and the
/// result is cut after `max` characters with "…".
pub(crate) fn lossy(b: &[u8], max: usize) -> String {
    let text = String::from_utf8_lossy(b);
    let mut out = String::new();
    for (count, c) in text.chars().enumerate() {
        if count == max {
            out.push('…');
            break;
        }
        out.push(if c.is_control() { '.' } else { c });
    }
    out
}

/// Cuts a string after `max` characters with "…".
pub(crate) fn shorten(s: &str, max: usize) -> String {
    match s.char_indices().nth(max) {
        Some((i, _)) => format!("{}…", &s[..i]),
        None => s.to_string(),
    }
}

/// A rough share of printable text, to decide whether a preview is worth showing.
pub(crate) fn mostly_text(b: &[u8]) -> bool {
    if b.is_empty() {
        return false;
    }
    let printable = b.iter().filter(|&&c| (0x20..0x7f).contains(&c) || c == b'\r' || c == b'\n' || c == b'\t').count();
    printable * 10 >= b.len() * 9
}

/// 3700 → "1 hour 1 minute".
pub(crate) fn duration(secs: u64) -> String {
    const UNITS: [(u64, &str, &str); 4] =
        [(86_400, "day", "days"), (3_600, "hour", "hours"), (60, "minute", "minutes"), (1, "second", "seconds")];
    if secs == 0 {
        return "0 seconds".to_string();
    }
    let mut parts = Vec::new();
    let mut rest = secs;
    for (size, one, many) in UNITS {
        if rest >= size {
            let n = rest / size;
            rest %= size;
            parts.push(plural(n as usize, one, many));
            if parts.len() == 2 {
                break;
            }
        } else if !parts.is_empty() {
            break;
        }
    }
    parts.join(" ")
}

/// Unix milliseconds as "2026-11-12 10:31:05.123456 UTC".
pub(crate) fn utc_time(ms: f64) -> String {
    if !ms.is_finite() || ms < 0.0 || ms > 253_402_300_799_000.0 {
        return "unknown".to_string();
    }
    let micros = (ms * 1000.0).round() as u64;
    let secs = micros / 1_000_000;
    let frac = micros % 1_000_000;
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02}.{frac:06} UTC", rem / 3600, rem / 60 % 60, rem % 60)
}

/// Days since 1970-01-01 to a calendar date (Howard Hinnant's algorithm).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// Case-insensitive substring test; `needle` must already be lower case.
pub(crate) fn contains_ci(hay: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return true;
    }
    if hay.is_ascii() && needle.is_ascii() {
        let (h, n) = (hay.as_bytes(), needle.as_bytes());
        return h.len() >= n.len() && h.windows(n.len()).any(|w| w.eq_ignore_ascii_case(n));
    }
    hay.to_lowercase().contains(needle)
}

/// Case-insensitive equality; `needle` must already be lower case.
pub(crate) fn eq_ci(value: &str, needle: &str) -> bool {
    if value.is_ascii() && needle.is_ascii() {
        return value.eq_ignore_ascii_case(needle);
    }
    value.to_lowercase() == needle
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_numbers_and_times() {
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(999), "999");
        assert_eq!(thousands(1240), "1,240");
        assert_eq!(thousands(1_234_567), "1,234,567");
        assert_eq!(byte_count(1), "1 byte");
        assert_eq!(duration(60), "1 minute");
        assert_eq!(duration(3_700), "1 hour 1 minute");
        assert_eq!(duration(86_400), "1 day");
        assert_eq!(utc_time(0.0), "1970-01-01 00:00:00.000000 UTC");
        assert_eq!(utc_time(1_762_943_465_123.5), "2025-11-12 10:31:05.123500 UTC");
        assert_eq!(shorten("abcdef", 3), "abc…");
        assert!(contains_ci("GitHub.com", "github"));
        assert!(eq_ci("GitHub.com", "github.com"));
    }
}
