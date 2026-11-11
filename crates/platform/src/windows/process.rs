//! Looks up the program behind a process ID, and its friendly name.

use std::ffi::c_void;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use windows::Win32::Foundation::CloseHandle;
use windows::Win32::Storage::FileSystem::{
    GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW,
};
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::core::{PCWSTR, PWSTR};

/// Full path of the program behind a process ID, if Windows lets us read it.
/// Protected system processes return `None` without administrator rights.
pub fn path(pid: u32) -> Option<PathBuf> {
    // SAFETY: the handle is closed before returning, and `buf` outlives the call.
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let result =
            QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len);
        let _ = CloseHandle(handle);
        result.ok()?;
        Some(PathBuf::from(String::from_utf16_lossy(&buf[..len as usize])))
    }
}

/// The "File description" from a program's version details, e.g.
/// "Google Chrome" for chrome.exe. Many small tools have none.
pub fn display_name(path: &Path) -> Option<String> {
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
    let file = PCWSTR(wide.as_ptr());

    // SAFETY: `data` is sized by GetFileVersionInfoSizeW, and every pointer
    // VerQueryValueW returns points inside `data`, which lives until the end.
    unsafe {
        let size = GetFileVersionInfoSizeW(file, None);
        if size == 0 {
            return None;
        }
        let mut data = vec![0u8; size as usize];
        GetFileVersionInfoW(file, None, size, data.as_mut_ptr() as *mut c_void).ok()?;

        // The version block is stored per language; take the first one listed.
        let (langs, langs_len) = query(&data, "\\VarFileInfo\\Translation")?;
        if langs_len < 4 {
            return None;
        }
        let lang = std::ptr::read_unaligned(langs as *const u16);
        let code_page = std::ptr::read_unaligned((langs as *const u16).add(1));

        let key = format!("\\StringFileInfo\\{lang:04x}{code_page:04x}\\FileDescription");
        let (text, chars) = query(&data, &key)?;
        // Some files report a length that runs past the text, so stop at the first NUL.
        let units: Vec<u16> = (0..chars as usize)
            .map(|i| std::ptr::read_unaligned((text as *const u16).add(i)))
            .take_while(|unit| *unit != 0)
            .collect();

        let name = String::from_utf16_lossy(&units);
        let name = name.trim();
        (!name.is_empty()).then(|| name.to_string())
    }
}

/// Finds one value inside a version block. Returns its address and length.
unsafe fn query(data: &[u8], key: &str) -> Option<(*const c_void, u32)> {
    let key: Vec<u16> = key.encode_utf16().chain(std::iter::once(0)).collect();
    let mut value: *mut c_void = std::ptr::null_mut();
    let mut len = 0u32;
    // SAFETY: `data` is a complete version block and `key` is NUL-terminated.
    let found = unsafe {
        VerQueryValueW(data.as_ptr() as *const c_void, PCWSTR(key.as_ptr()), &mut value, &mut len)
    };
    (found.as_bool() && !value.is_null() && len > 0).then_some((value as *const c_void, len))
}
