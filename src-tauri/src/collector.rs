//! The window's side of the Portlight Collector service: connecting to its
//! feed, and installing or removing it with one administrator prompt.

use std::io::{BufRead, BufReader};
use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

use portlight_core::Observation;
use portlight_core::wire::{self, Message};
use portlight_platform::service::{self, ServiceState};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::state::AppState;

/// Event the window listens to. Its payload is a `CollectorStatus`.
pub const STATUS_EVENT: &str = "collector:status";

const SERVICE_EXE: &str = "portlight-service.exe";

// Exit codes from `portlight-service install|uninstall`. Keep in step with
// crates/collector/src/setup.rs.
const NOT_ADMIN: u32 = 3;
const SERVICE_FAILED: u32 = 4;
const COPY_FAILED: u32 = 5;

/// Keep in step with CollectorStatus in src/lib/types.ts.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CollectorStatus {
    pub installed: bool,
    pub connected: bool,
    pub version: Option<String>,
    pub outdated: bool,
    pub can_install: bool,
    pub error: Option<String>,
}

/// What the monitor loop knows about the connection.
#[derive(Debug, Default, Clone)]
pub struct Link {
    pub connected: bool,
    pub version: Option<String>,
    pub outdated: bool,
    pub error: Option<String>,
}

/// An open feed from the collector.
pub struct Feed {
    readings: Receiver<Option<Observation>>,
}

pub struct Hello {
    pub version: String,
    pub names: Vec<(IpAddr, String)>,
}

/// Connects to the collector, if it is running. The first message names
/// its version and every site name it knows.
pub fn connect() -> Result<(Feed, Hello), (Option<String>, String)> {
    let pipe = portlight_platform::pipe::connect().map_err(|e| (None, e.to_string()))?;
    let mut reader = BufReader::new(pipe);
    let mut line = String::new();
    reader.read_line(&mut line).map_err(|e| (None, e.to_string()))?;
    let Ok(Message::Hello { version, protocol, names }) = Message::decode(&line) else {
        return Err((None, "the collector sent something unexpected".into()));
    };
    if protocol != wire::PROTOCOL {
        return Err((Some(version), "the collector is a different version".into()));
    }

    let (send, readings) = mpsc::channel();
    let started = thread::Builder::new().name("portlight-feed".into()).spawn(move || {
        let mut line = String::new();
        loop {
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    if let Ok(Message::Reading { observation }) = Message::decode(&line)
                        && send.send(Some(observation)).is_err()
                    {
                        return;
                    }
                }
            }
        }
        // `None` says the collector went away.
        let _ = send.send(None);
    });
    started.map_err(|e| (Some(version.clone()), e.to_string()))?;
    Ok((Feed { readings }, Hello { version, names }))
}

pub enum Drained {
    /// One or more readings, merged.
    Reading(Observation),
    /// Nothing new since last time.
    Nothing,
    /// The collector stopped or the pipe broke.
    Ended,
}

impl Feed {
    /// Everything that arrived since the last call, merged into one reading.
    pub fn drain(&self) -> Drained {
        let mut merged: Option<Observation> = None;
        for reading in self.readings.try_iter() {
            let Some(reading) = reading else { return Drained::Ended };
            match &mut merged {
                Some(merged) => merged.merge(reading),
                None => merged = Some(reading),
            }
        }
        match merged {
            Some(reading) => Drained::Reading(reading),
            None => Drained::Nothing,
        }
    }
}

/// True when the service is older than this window.
pub fn is_outdated(service_version: &str) -> bool {
    let parse = |v: &str| -> Vec<u64> { v.split('.').map(|part| part.parse().unwrap_or(0)).collect() };
    parse(service_version) < parse(env!("CARGO_PKG_VERSION"))
}

pub fn status(app: &AppHandle) -> CollectorStatus {
    let link = app.state::<AppState>().link();
    let installed = matches!(service::collector_state(), Ok(state) if state != ServiceState::NotInstalled);
    CollectorStatus {
        installed,
        connected: link.connected,
        version: link.version,
        outdated: link.outdated,
        can_install: bundled_service().is_some(),
        error: link.error,
    }
}

pub fn emit_status(app: &AppHandle) {
    let _ = app.emit(STATUS_EVENT, status(app));
}

/// The service program shipped next to Portlight.
fn bundled_service() -> Option<PathBuf> {
    let path = std::env::current_exe().ok()?.parent()?.join(SERVICE_EXE);
    path.exists().then_some(path)
}

/// Installs (or updates) and starts the service, then waits briefly for its
/// feed so the window can switch straight to detailed monitoring.
pub fn install(app: &AppHandle) -> Result<CollectorStatus, String> {
    let program = bundled_service().ok_or("The background service is not part of this copy of Portlight.")?;
    run(app, &program, "install")?;

    let state = app.state::<AppState>();
    state.reconnect_now();
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(6) && !state.link().connected {
        thread::sleep(Duration::from_millis(200));
    }
    Ok(status(app))
}

/// Stops and removes the service. The window goes back to basic monitoring.
pub fn uninstall(app: &AppHandle) -> Result<CollectorStatus, String> {
    let program = bundled_service().ok_or("The background service is not part of this copy of Portlight.")?;
    run(app, &program, "uninstall")?;
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(4) && app.state::<AppState>().link().connected {
        thread::sleep(Duration::from_millis(200));
    }
    Ok(status(app))
}

fn run(app: &AppHandle, program: &std::path::Path, command: &str) -> Result<(), String> {
    let result = service::run_elevated(program, command);
    let error = match result {
        Ok(0) => None,
        Ok(NOT_ADMIN) => Some("The background service needs administrator rights.".to_string()),
        Ok(SERVICE_FAILED) => Some(
            "Windows could not set up the background service. Details are in C:\\ProgramData\\Portlight\\collector.log."
                .to_string(),
        ),
        Ok(COPY_FAILED) => Some("The background service could not be copied into Program Files.".to_string()),
        Ok(code) => Some(format!("The background service setup stopped with code {code}.")),
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
            Some("Windows did not allow the change, so nothing was changed.".to_string())
        }
        Err(e) => Some(format!("Could not start the background service setup: {e}")),
    };
    app.state::<AppState>().update_link(|link| link.error = error.clone());
    emit_status(app);
    match error {
        Some(message) => Err(message),
        None => Ok(()),
    }
}
