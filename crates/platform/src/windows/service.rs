//! Checks on the Portlight Collector service, and running a program with
//! administrator rights (one Windows prompt) to install or remove it.

use std::io;
use std::path::Path;

use windows::Win32::Foundation::{CloseHandle, ERROR_CANCELLED, ERROR_SERVICE_DOES_NOT_EXIST};
use windows::Win32::System::Services::{
    CloseServiceHandle, OpenSCManagerW, OpenServiceW, QueryServiceStatus, SC_MANAGER_CONNECT, SERVICE_QUERY_STATUS,
    SERVICE_RUNNING, SERVICE_START_PENDING, SERVICE_STATUS,
};
use windows::Win32::System::Threading::{GetExitCodeProcess, INFINITE, WaitForSingleObject};
use windows::Win32::UI::Shell::{SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, ShellExecuteExW};
use windows::Win32::UI::WindowsAndMessaging::{SW_HIDE, SW_SHOWNORMAL};
use windows::core::PCWSTR;

/// The collector's name in the Windows service list.
pub const SERVICE_NAME: &str = "PortlightCollector";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceState {
    NotInstalled,
    Stopped,
    Starting,
    Running,
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Whether the collector service is installed and running. Needs no
/// administrator rights.
pub fn collector_state() -> io::Result<ServiceState> {
    let name = wide(SERVICE_NAME);
    // SAFETY: handles are closed before returning; `name` is NUL-terminated.
    unsafe {
        let manager = OpenSCManagerW(PCWSTR::null(), PCWSTR::null(), SC_MANAGER_CONNECT).map_err(io::Error::other)?;
        let service = match OpenServiceW(manager, PCWSTR(name.as_ptr()), SERVICE_QUERY_STATUS) {
            Ok(service) => service,
            Err(error) => {
                let _ = CloseServiceHandle(manager);
                if error.code() == ERROR_SERVICE_DOES_NOT_EXIST.to_hresult() {
                    return Ok(ServiceState::NotInstalled);
                }
                return Err(io::Error::other(error));
            }
        };
        let mut status = SERVICE_STATUS::default();
        let result = QueryServiceStatus(service, &mut status);
        let _ = CloseServiceHandle(service);
        let _ = CloseServiceHandle(manager);
        result.map_err(io::Error::other)?;
        Ok(match status.dwCurrentState {
            SERVICE_RUNNING => ServiceState::Running,
            SERVICE_START_PENDING => ServiceState::Starting,
            _ => ServiceState::Stopped,
        })
    }
}

/// Runs a program with administrator rights, which shows the Windows
/// prompt, and waits for it to finish. Returns its exit code.
///
/// If the person says no, the error has kind `PermissionDenied`.
pub fn run_elevated(program: &Path, arguments: &str) -> io::Result<u32> {
    let verb = wide("runas");
    let file = wide(&program.to_string_lossy());
    let params = wide(arguments);

    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC,
        lpVerb: PCWSTR(verb.as_ptr()),
        lpFile: PCWSTR(file.as_ptr()),
        lpParameters: PCWSTR(params.as_ptr()),
        nShow: SW_HIDE.0,
        ..Default::default()
    };
    // SAFETY: every string outlives the call; the process handle is closed below.
    unsafe {
        if let Err(error) = ShellExecuteExW(&mut info) {
            if error.code() == ERROR_CANCELLED.to_hresult() {
                return Err(io::Error::new(io::ErrorKind::PermissionDenied, "the administrator prompt was declined"));
            }
            return Err(io::Error::other(error));
        }
        if info.hProcess.is_invalid() {
            return Err(io::Error::other("Windows did not start the program"));
        }
        WaitForSingleObject(info.hProcess, INFINITE);
        let mut code = 0u32;
        let result = GetExitCodeProcess(info.hProcess, &mut code);
        let _ = CloseHandle(info.hProcess);
        result.map_err(io::Error::other)?;
        Ok(code)
    }
}

/// Opens a web address in the default browser.
pub fn open_url(url: &str) -> io::Result<()> {
    let verb = wide("open");
    let file = wide(url);
    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOASYNC,
        lpVerb: PCWSTR(verb.as_ptr()),
        lpFile: PCWSTR(file.as_ptr()),
        nShow: SW_SHOWNORMAL.0,
        ..Default::default()
    };
    // SAFETY: the strings outlive the call.
    unsafe { ShellExecuteExW(&mut info) }.map_err(io::Error::other)
}
