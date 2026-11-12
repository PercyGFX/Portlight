//! A two-way named pipe that only the signed-in user can open, for talking
//! between Portlight's own programs on this PC (the window and the AI
//! assistant bridge, `portlight-mcp`).
//!
//! The pipe's name includes the user's security ID, so people signed in to
//! the same PC each get their own, and its access list names only that user.

use std::fs::{File, OpenOptions};
use std::io;
use std::os::windows::io::FromRawHandle;

use windows::Win32::Foundation::{CloseHandle, ERROR_PIPE_CONNECTED, HANDLE, HLOCAL, INVALID_HANDLE_VALUE, LocalFree};
use windows::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
};
use windows::Win32::Security::{
    GetTokenInformation, PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_USER, TokenUser,
};
use windows::Win32::Storage::FileSystem::{FILE_FLAG_FIRST_PIPE_INSTANCE, PIPE_ACCESS_DUPLEX};
use windows::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
};
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
use windows::core::{PCWSTR, PWSTR};

/// The signed-in user's security ID as text, e.g. "S-1-5-21-...".
pub fn current_user_sid() -> io::Result<String> {
    // SAFETY: the token handle and the SID string are released before returning.
    unsafe {
        let mut token = HANDLE::default();
        OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).map_err(io::Error::other)?;
        let mut size = 0u32;
        let _ = GetTokenInformation(token, TokenUser, None, 0, &mut size);
        // u64 elements keep the buffer aligned for TOKEN_USER.
        let mut buf = vec![0u64; (size as usize).div_ceil(8).max(1)];
        let read = GetTokenInformation(token, TokenUser, Some(buf.as_mut_ptr().cast()), size, &mut size);
        let _ = CloseHandle(token);
        read.map_err(io::Error::other)?;
        let user = &*(buf.as_ptr() as *const TOKEN_USER);
        let mut text = PWSTR::null();
        ConvertSidToStringSidW(user.User.Sid, &mut text).map_err(io::Error::other)?;
        let sid = text.to_string().map_err(io::Error::other);
        LocalFree(Some(HLOCAL(text.0.cast())));
        sid
    }
}

/// `\\.\pipe\<prefix>-<user SID>`
pub fn pipe_name(prefix: &str) -> io::Result<String> {
    Ok(format!(r"\\.\pipe\{prefix}-{}", current_user_sid()?))
}

/// Hands out one connected pipe per client.
pub struct UserPipeServer {
    name: String,
    access: String,
    first: bool,
}

impl UserPipeServer {
    pub fn new(prefix: &str) -> io::Result<Self> {
        let sid = current_user_sid()?;
        Ok(Self {
            name: format!(r"\\.\pipe\{prefix}-{sid}"),
            // Full access for this user only. "P" stops inherited entries.
            access: format!("D:P(A;;GA;;;{sid})"),
            first: true,
        })
    }

    /// Waits for the next client and returns the connected pipe, which can
    /// be read from and written to like a file.
    pub fn accept(&mut self) -> io::Result<File> {
        let name: Vec<u16> = self.name.encode_utf16().chain(std::iter::once(0)).collect();
        let access: Vec<u16> = self.access.encode_utf16().chain(std::iter::once(0)).collect();
        let mut descriptor = PSECURITY_DESCRIPTOR::default();
        // SAFETY: `access` is NUL-terminated SDDL; the descriptor is freed below.
        unsafe { ConvertStringSecurityDescriptorToSecurityDescriptorW(PCWSTR(access.as_ptr()), 1, &mut descriptor, None) }
            .map_err(io::Error::other)?;
        let attributes = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor.0,
            bInheritHandle: false.into(),
        };
        let mut mode = PIPE_ACCESS_DUPLEX;
        if self.first {
            mode |= FILE_FLAG_FIRST_PIPE_INSTANCE;
        }
        // SAFETY: every pointer is valid for the call.
        let handle = unsafe {
            CreateNamedPipeW(
                PCWSTR(name.as_ptr()),
                mode,
                PIPE_TYPE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                PIPE_UNLIMITED_INSTANCES,
                64 * 1024,
                64 * 1024,
                0,
                Some(&attributes),
            )
        };
        // SAFETY: no longer needed once the pipe exists.
        unsafe {
            LocalFree(Some(HLOCAL(descriptor.0)));
        }
        if handle == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }
        self.first = false;
        // SAFETY: `handle` is the pipe just created.
        match unsafe { ConnectNamedPipe(handle, None) } {
            Ok(()) => {}
            Err(error) if error.code() == ERROR_PIPE_CONNECTED.to_hresult() => {}
            Err(error) => {
                // SAFETY: closing the handle we own.
                unsafe {
                    let _ = CloseHandle(handle);
                }
                return Err(io::Error::other(error));
            }
        }
        // SAFETY: we own `handle`; File closes it when dropped.
        Ok(unsafe { File::from_raw_handle(handle.0) })
    }
}

/// Opens the user's pipe with the given prefix, for reading and writing.
pub fn connect(prefix: &str) -> io::Result<File> {
    OpenOptions::new().read(true).write(true).open(pipe_name(prefix)?)
}

#[cfg(test)]
mod tests {
    use std::io::{BufRead, BufReader, Write};

    use super::*;

    #[test]
    fn a_message_goes_there_and_back() {
        assert!(current_user_sid().unwrap().starts_with("S-1-"));
        let prefix = format!("portlight-test-{}", std::process::id());
        let mut server = UserPipeServer::new(&prefix).unwrap();
        let echo = std::thread::spawn(move || {
            let pipe = server.accept().unwrap();
            let mut line = String::new();
            BufReader::new(&pipe).read_line(&mut line).unwrap();
            (&pipe).write_all(format!("echo {line}").as_bytes()).unwrap();
        });
        // The server may not be listening yet.
        let mut client = None;
        for _ in 0..50 {
            match connect(&prefix) {
                Ok(pipe) => {
                    client = Some(pipe);
                    break;
                }
                Err(_) => std::thread::sleep(std::time::Duration::from_millis(20)),
            }
        }
        let client = client.expect("the pipe should open");
        (&client).write_all(b"hello\n").unwrap();
        let mut reply = String::new();
        BufReader::new(&client).read_line(&mut reply).unwrap();
        assert_eq!(reply, "echo hello\n");
        echo.join().unwrap();
    }
}
