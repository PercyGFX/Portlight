//! Installing and removing the service. Both need administrator rights; the
//! window runs them through one Windows prompt.
//!
//! The service runs with full system rights, so its program is copied into
//! Program Files, where only administrators can change it. Running it from
//! Portlight's own folder, which the signed-in user can write to, would let
//! any program on the PC swap it out and gain those rights.

use std::ffi::{OsStr, OsString};
use std::fs;
use std::io;
use std::path::PathBuf;
use std::process::ExitCode;
use std::thread;
use std::time::{Duration, Instant};

use portlight_platform::service::SERVICE_NAME;
use windows_service::service::{
    ServiceAccess, ServiceAction, ServiceActionType, ServiceErrorControl, ServiceFailureActions,
    ServiceFailureResetPeriod, ServiceInfo, ServiceStartType, ServiceState, ServiceType,
};
use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};

use crate::collect::log;

// Exit codes the window turns into messages. Keep in step with
// src-tauri/src/collector.rs.
pub const USAGE: u8 = 2;
const NOT_ADMIN: u8 = 3;
const SERVICE_FAILED: u8 = 4;
const COPY_FAILED: u8 = 5;

const DISPLAY_NAME: &str = "Portlight Collector";
const DESCRIPTION: &str = "Reads this PC's network events for Portlight, so it can show every connection, \
    the data each app uses and the sites apps look up. Remove it from Portlight's menu.";
const EXE_NAME: &str = "portlight-service.exe";

fn install_dir() -> PathBuf {
    let base = std::env::var_os("ProgramW6432")
        .or_else(|| std::env::var_os("ProgramFiles"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Program Files"));
    base.join("Portlight")
}

pub fn install() -> ExitCode {
    match try_install() {
        Ok(()) => ExitCode::SUCCESS,
        Err((code, error)) => {
            log(&format!("install failed: {error}"));
            ExitCode::from(code)
        }
    }
}

pub fn uninstall() -> ExitCode {
    match try_uninstall() {
        Ok(()) => ExitCode::SUCCESS,
        Err((code, error)) => {
            log(&format!("uninstall failed: {error}"));
            ExitCode::from(code)
        }
    }
}

type Failure = (u8, String);

fn manager() -> Result<ServiceManager, Failure> {
    ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT | ServiceManagerAccess::CREATE_SERVICE)
        .map_err(|e| (NOT_ADMIN, e.to_string()))
}

fn try_install() -> Result<(), Failure> {
    let manager = manager()?;
    let access = ServiceAccess::QUERY_STATUS
        | ServiceAccess::START
        | ServiceAccess::STOP
        | ServiceAccess::CHANGE_CONFIG;

    // An older copy has to stop before its program can be replaced.
    let existing = manager.open_service(SERVICE_NAME, access).ok();
    if let Some(service) = &existing {
        stop_and_wait(service);
    }

    let dir = install_dir();
    let target = dir.join(EXE_NAME);
    let source = std::env::current_exe().map_err(|e| (COPY_FAILED, e.to_string()))?;
    if !same_file(&source, &target) {
        fs::create_dir_all(&dir).map_err(|e| (COPY_FAILED, e.to_string()))?;
        copy_with_retry(&source, &target).map_err(|e| (COPY_FAILED, format!("{}: {e}", target.display())))?;
    }

    let info = ServiceInfo {
        name: OsString::from(SERVICE_NAME),
        display_name: OsString::from(DISPLAY_NAME),
        service_type: ServiceType::OWN_PROCESS,
        start_type: ServiceStartType::AutoStart,
        error_control: ServiceErrorControl::Normal,
        executable_path: target,
        launch_arguments: vec![OsString::from("service")],
        dependencies: vec![],
        account_name: None, // LocalSystem
        account_password: None,
    };
    let service = match existing {
        Some(service) => {
            service.change_config(&info).map_err(|e| (SERVICE_FAILED, e.to_string()))?;
            service
        }
        None => manager.create_service(&info, access).map_err(|e| (SERVICE_FAILED, e.to_string()))?,
    };
    let _ = service.set_description(DESCRIPTION);
    // Restart after a crash: after 5 seconds, then 30, then a minute.
    let _ = service.update_failure_actions(ServiceFailureActions {
        reset_period: ServiceFailureResetPeriod::After(Duration::from_secs(24 * 60 * 60)),
        reboot_msg: None,
        command: None,
        actions: Some(vec![
            ServiceAction { action_type: ServiceActionType::Restart, delay: Duration::from_secs(5) },
            ServiceAction { action_type: ServiceActionType::Restart, delay: Duration::from_secs(30) },
            ServiceAction { action_type: ServiceActionType::Restart, delay: Duration::from_secs(60) },
        ]),
    });
    let _ = service.set_failure_actions_on_non_crash_failures(true);

    service.start(&[] as &[&OsStr]).map_err(|e| (SERVICE_FAILED, e.to_string()))?;
    Ok(())
}

fn try_uninstall() -> Result<(), Failure> {
    let manager = manager()?;
    let access = ServiceAccess::QUERY_STATUS | ServiceAccess::STOP | ServiceAccess::DELETE;
    if let Ok(service) = manager.open_service(SERVICE_NAME, access) {
        stop_and_wait(&service);
        service.delete().map_err(|e| (SERVICE_FAILED, e.to_string()))?;
    }
    // Windows may hold the program file for a moment after the service stops.
    let dir = install_dir();
    for _ in 0..20 {
        match fs::remove_file(dir.join(EXE_NAME)) {
            Ok(()) => break,
            Err(e) if e.kind() == io::ErrorKind::NotFound => break,
            Err(_) => thread::sleep(Duration::from_millis(250)),
        }
    }
    let _ = fs::remove_dir(&dir);
    Ok(())
}

fn stop_and_wait(service: &windows_service::service::Service) {
    let _ = service.stop();
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(15) {
        match service.query_status() {
            Ok(status) if status.current_state == ServiceState::Stopped => return,
            Err(_) => return,
            _ => thread::sleep(Duration::from_millis(200)),
        }
    }
}

fn copy_with_retry(source: &PathBuf, target: &PathBuf) -> io::Result<()> {
    let mut last = None;
    for _ in 0..20 {
        match fs::copy(source, target) {
            Ok(_) => return Ok(()),
            Err(e) => {
                last = Some(e);
                thread::sleep(Duration::from_millis(250));
            }
        }
    }
    Err(last.unwrap_or_else(|| io::Error::other("copy failed")))
}

fn same_file(a: &PathBuf, b: &PathBuf) -> bool {
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}
