//! Data types shared by the core and its front-ends.

use std::io;
use std::net::SocketAddr;
use std::path::PathBuf;

use serde::Serialize;

/// One TCP connection and the process that owns it, as the OS reports it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Connection {
    pub pid: u32,
    pub local: SocketAddr,
    pub remote: SocketAddr,
}

/// What the OS knows about a running program.
#[derive(Debug, Clone)]
pub struct ProcessInfo {
    /// Full path of the executable.
    pub path: PathBuf,
    /// Friendly name from the file's version details, e.g. "Google Chrome".
    pub display_name: Option<String>,
}

/// Everything the core asks of the operating system.
pub trait Platform {
    /// Every open TCP connection right now, without listening sockets.
    fn tcp_connections(&self) -> io::Result<Vec<Connection>>;

    /// The program behind a process ID. `None` when the OS will not say,
    /// which happens for protected system processes without administrator rights.
    fn process_info(&self, pid: u32) -> Option<ProcessInfo>;
}

/// An app that has been online during this session.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AppView {
    /// Stable identity: the lower-cased executable path.
    pub id: String,
    pub name: String,
    /// File name of the executable, e.g. `chrome.exe`.
    pub exe: String,
    pub path: Option<String>,
    /// When Portlight first saw this app online (Unix milliseconds).
    pub first_seen: u64,
    /// False until the user acknowledges a new app.
    pub trusted: bool,
    /// Connections open right now.
    pub connections: u32,
    /// Distinct remote addresses it is connected to right now.
    pub servers: u32,
    /// Connections opened since Portlight started.
    pub total_opened: u64,
}

/// A connection that is open right now.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ConnView {
    pub id: u64,
    pub app_id: String,
    pub pid: u32,
    pub remote_ip: String,
    pub remote_port: u16,
    pub local_port: u16,
    /// Reverse-DNS name of the remote address, once known.
    pub host: Option<String>,
    pub opened_at: u64,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum EventKind {
    Opened,
    Closed,
}

/// A connection opening or closing.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FlowEvent {
    /// Increases by one per event; front-ends use it to spot what is new.
    pub seq: u64,
    pub time: u64,
    pub kind: EventKind,
    pub app_id: String,
    pub app_name: String,
    pub remote_ip: String,
    pub remote_port: u16,
    pub host: Option<String>,
}

/// An app that just went online for the first time.
#[derive(Debug, Clone, PartialEq)]
pub struct NewApp {
    pub id: String,
    pub name: String,
}

/// The full picture a front-end needs to draw itself.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub now: u64,
    /// Seconds left in the first-run learning period; 0 once alerts are live.
    pub learning_seconds_left: u32,
    pub apps: Vec<AppView>,
    pub connections: Vec<ConnView>,
    pub events: Vec<FlowEvent>,
}
