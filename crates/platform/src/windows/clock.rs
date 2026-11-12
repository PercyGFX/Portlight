//! The local time zone, so days in the history start at local midnight.

use windows::Win32::System::Time::{GetTimeZoneInformation, TIME_ZONE_INFORMATION};

/// Minutes to add to UTC to get local time right now (east of UTC is
/// positive), including daylight saving.
pub fn local_utc_offset_minutes() -> i32 {
    let mut info = TIME_ZONE_INFORMATION::default();
    // SAFETY: `info` is a valid structure to fill in.
    let mode = unsafe { GetTimeZoneInformation(&mut info) };
    // 1 = standard time, 2 = daylight time, 0 = no daylight saving rules.
    let bias = match mode {
        1 => info.Bias + info.StandardBias,
        2 => info.Bias + info.DaylightBias,
        _ => info.Bias,
    };
    -bias
}
