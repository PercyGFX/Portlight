//! Data types shared by the core and its front-ends.

use std::io;
use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Protocol {
    Tcp,
    Udp,
}

/// One connection and the process that owns it, as the OS reports it.
///
/// UDP has no connections, so a UDP "connection" is one app talking to one
/// remote address and port. Its local port is always 0, because apps often
/// use a new local port for every message (name lookups do).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Connection {
    pub pid: u32,
    pub protocol: Protocol,
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

    /// Who signed a program file. Can be slow; the tracker only asks for
    /// programs it has never seen, when the answer decides whether to alert.
    fn signature(&self, _path: &Path) -> Signature {
        Signature::Unknown
    }
}

/// Whether a program carries a valid signature from its maker.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "lowercase")]
pub enum Signature {
    Signed { publisher: String },
    Unsigned,
    /// Not checked yet, or there is no program file to check.
    #[default]
    Unknown,
}

/// Who owns the network an address belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnerInfo {
    /// e.g. "Cloudflare, Inc.", or "Your home network".
    pub name: String,
    pub asn: Option<u32>,
    /// ISO 3166-1 alpha-2 code of the network's registration.
    pub country: Option<String>,
}

/// Something one app did with one server since the last reading, for the
/// history: connections opened and bytes moved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Activity {
    pub app_id: String,
    pub app_name: String,
    pub protocol: Protocol,
    pub remote_ip: IpAddr,
    pub remote_port: u16,
    pub host: Option<String>,
    pub opened: u32,
    pub sent: u64,
    pub received: u64,
    pub time: u64,
}

/// Where the tracker's picture comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    /// Snapshots of the TCP table: TCP only, no byte counts, may miss short connections.
    #[default]
    Basic,
    /// Live network events from the Portlight Collector service.
    Detailed,
}

/// A connection as a data source reports it, with the data it has carried so far.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Observed {
    #[serde(flatten)]
    pub conn: Connection,
    /// Bytes sent since the source first saw this connection.
    pub sent: u64,
    /// Bytes received since the source first saw this connection.
    pub received: u64,
}

impl Observed {
    /// A connection with no byte counts, as the TCP table reports it.
    pub fn bare(conn: Connection) -> Self {
        Self { conn, sent: 0, received: 0 }
    }
}

/// A program the collector looked up, sent along so the window needs no
/// administrator rights to name it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcessRecord {
    pub pid: u32,
    pub path: String,
    pub name: Option<String>,
}

impl ProcessRecord {
    pub fn info(&self) -> ProcessInfo {
        ProcessInfo { path: PathBuf::from(&self.path), display_name: self.name.clone() }
    }
}

/// One reading from a data source: what is open now and what ended since
/// the last reading.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Observation {
    pub source: Source,
    /// Every connection open right now.
    pub open: Vec<Observed>,
    /// Connections that ended since the last reading, with their final byte
    /// counts. Includes connections that opened and closed in between.
    pub closed: Vec<Observed>,
    /// Site names apps looked up, and the addresses they got back.
    pub names: Vec<(IpAddr, String)>,
    /// Programs behind the process IDs above, when the source knows them.
    pub processes: Vec<ProcessRecord>,
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
    /// Bytes sent this session, including connections already closed.
    pub bytes_sent: u64,
    /// Bytes received this session, including connections already closed.
    pub bytes_received: u64,
    pub signature: Signature,
}

/// A connection that is open right now.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ConnView {
    pub id: u64,
    pub app_id: String,
    pub pid: u32,
    pub protocol: Protocol,
    pub remote_ip: String,
    pub remote_port: u16,
    pub local_port: u16,
    /// The site name the app looked up, or else the reverse-DNS name, once known.
    pub host: Option<String>,
    pub opened_at: u64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    /// Who owns the server's network, e.g. "Cloudflare, Inc.".
    pub owner: Option<String>,
    pub asn: Option<u32>,
    /// Where that network is registered (ISO 3166-1 alpha-2).
    pub country: Option<String>,
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
    pub protocol: Protocol,
    pub remote_ip: String,
    pub remote_port: u16,
    pub host: Option<String>,
    /// For "closed" events, the connection's totals. Zero for "opened".
    pub bytes_sent: u64,
    pub bytes_received: u64,
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
    pub source: Source,
    /// Seconds left in the first-run learning period; 0 once alerts are live.
    pub learning_seconds_left: u32,
    pub apps: Vec<AppView>,
    pub connections: Vec<ConnView>,
    pub events: Vec<FlowEvent>,
}
