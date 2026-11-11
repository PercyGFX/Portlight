//! The local channel between the Portlight Collector service and the window:
//! a Windows named pipe. Only programs on this PC can open it, and only
//! people signed in to this PC can read from it.

use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::os::windows::io::AsRawHandle;

use windows::Win32::Foundation::{CloseHandle, ERROR_PIPE_CONNECTED, HANDLE, HLOCAL, INVALID_HANDLE_VALUE, LocalFree};
use windows::Win32::Security::Authorization::ConvertStringSecurityDescriptorToSecurityDescriptorW;
use windows::Win32::Security::{PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES};
use windows::Win32::Storage::FileSystem::{FILE_FLAG_FIRST_PIPE_INSTANCE, FlushFileBuffers, PIPE_ACCESS_OUTBOUND, WriteFile};
use windows::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, GetNamedPipeServerSessionId, PIPE_REJECT_REMOTE_CLIENTS,
    PIPE_TYPE_BYTE, PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
};
use windows::core::PCWSTR;

pub const PIPE_NAME: &str = r"\\.\pipe\portlight-collector";

/// Who may use the pipe: full access for the system and administrators,
/// read-only for people signed in at this PC (including Remote Desktop).
const PIPE_ACCESS: &str = "D:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;GR;;;IU)";

/// Hands out one connected pipe per client, for the collector.
pub struct PipeServer {
    first: bool,
}

impl PipeServer {
    pub fn new() -> Self {
        Self { first: true }
    }

    /// Waits for the next window to connect and returns a writer to it.
    pub fn accept(&mut self) -> io::Result<PipeWriter> {
        let name: Vec<u16> = PIPE_NAME.encode_utf16().chain(std::iter::once(0)).collect();
        let access: Vec<u16> = PIPE_ACCESS.encode_utf16().chain(std::iter::once(0)).collect();

        let mut descriptor = PSECURITY_DESCRIPTOR::default();
        // SAFETY: `access` is a NUL-terminated SDDL string; the result is freed below.
        unsafe { ConvertStringSecurityDescriptorToSecurityDescriptorW(PCWSTR(access.as_ptr()), 1, &mut descriptor, None) }
            .map_err(io::Error::other)?;
        let attributes = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor.0,
            bInheritHandle: false.into(),
        };

        // The first instance claims the name, so no other program can have
        // created a look-alike pipe before us.
        let mut mode = PIPE_ACCESS_OUTBOUND;
        if self.first {
            mode |= FILE_FLAG_FIRST_PIPE_INSTANCE;
        }
        // SAFETY: all pointers are valid for the call.
        let handle = unsafe {
            CreateNamedPipeW(
                PCWSTR(name.as_ptr()),
                mode,
                PIPE_TYPE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                PIPE_UNLIMITED_INSTANCES,
                256 * 1024,
                0,
                0,
                Some(&attributes),
            )
        };
        // SAFETY: the descriptor came from ConvertStringSecurityDescriptor... and is no longer needed.
        unsafe {
            LocalFree(Some(HLOCAL(descriptor.0)));
        }
        if handle == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }
        self.first = false;

        // SAFETY: `handle` is a pipe we just created.
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
        Ok(PipeWriter { handle })
    }
}

impl Default for PipeServer {
    fn default() -> Self {
        Self::new()
    }
}

/// The collector's end of one client's pipe.
pub struct PipeWriter {
    handle: HANDLE,
}

// SAFETY: a pipe handle can be used from any thread.
unsafe impl Send for PipeWriter {}

impl Write for PipeWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut written = 0u32;
        // SAFETY: `handle` is open and `buf` is valid for the call.
        unsafe { WriteFile(self.handle, Some(buf), Some(&mut written), None) }.map_err(io::Error::other)?;
        Ok(written as usize)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Drop for PipeWriter {
    fn drop(&mut self) {
        // SAFETY: the handle is ours and is not used after this.
        unsafe {
            let _ = FlushFileBuffers(self.handle);
            let _ = DisconnectNamedPipe(self.handle);
            let _ = CloseHandle(self.handle);
        }
    }
}

/// Opens the collector's pipe for reading, for the window.
///
/// Checks that the other end is a Windows service: services run in session
/// 0, where ordinary programs cannot, so a program pretending to be the
/// collector is turned away.
pub fn connect() -> io::Result<File> {
    let file = OpenOptions::new().read(true).open(PIPE_NAME)?;
    let mut session = u32::MAX;
    // SAFETY: `file` is an open pipe handle.
    unsafe { GetNamedPipeServerSessionId(HANDLE(file.as_raw_handle()), &mut session) }.map_err(io::Error::other)?;
    if session != 0 {
        return Err(io::Error::new(io::ErrorKind::PermissionDenied, "the collector pipe is not run by a service"));
    }
    Ok(file)
}
