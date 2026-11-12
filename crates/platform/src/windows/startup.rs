//! "Start Portlight when I sign in": the same per-user Run entry the
//! installer creates, so either place can turn it on or off.

use std::ffi::c_void;
use std::io;

use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
use windows::Win32::System::Registry::{
    HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ, RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW,
};
use windows::core::PCWSTR;

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE: &str = "Portlight";

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// True when Windows will start Portlight at sign-in.
pub fn is_enabled() -> bool {
    let (key, value) = (wide(RUN_KEY), wide(VALUE));
    let mut size = 0u32;
    // SAFETY: a size query with no buffer.
    let code = unsafe {
        RegGetValueW(HKEY_CURRENT_USER, PCWSTR(key.as_ptr()), PCWSTR(value.as_ptr()), RRF_RT_REG_SZ, None, None, Some(&mut size))
    };
    code == ERROR_SUCCESS && size > 2
}

/// Turns starting at sign-in on (running `command_line`) or off.
pub fn set(enabled: bool, command_line: &str) -> io::Result<()> {
    let (key, value) = (wide(RUN_KEY), wide(VALUE));
    // SAFETY: all strings are NUL-terminated and outlive the calls.
    let code = unsafe {
        if enabled {
            let data = wide(command_line);
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                PCWSTR(key.as_ptr()),
                PCWSTR(value.as_ptr()),
                REG_SZ.0,
                Some(data.as_ptr() as *const c_void),
                (data.len() * 2) as u32,
            )
        } else {
            RegDeleteKeyValueW(HKEY_CURRENT_USER, PCWSTR(key.as_ptr()), PCWSTR(value.as_ptr()))
        }
    };
    if code == ERROR_SUCCESS || (!enabled && code == ERROR_FILE_NOT_FOUND) {
        Ok(())
    } else {
        Err(io::Error::from_raw_os_error(code.0 as i32))
    }
}
