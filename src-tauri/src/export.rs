//! Saving the live view as CSV: the apps, the open connections, or the
//! connection flow history.

use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::Path;

use portlight_core::{EventKind, Protocol, Snapshot};
use serde::Deserialize;

use crate::state::format_time;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LiveExport {
    Apps,
    Connections,
    Flow,
}

impl LiveExport {
    pub fn slug(self) -> &'static str {
        match self {
            LiveExport::Apps => "apps",
            LiveExport::Connections => "connections",
            LiveExport::Flow => "connection-flow",
        }
    }
}

pub fn live_csv(path: &Path, snapshot: &Snapshot, what: LiveExport, utc_offset_minutes: i32) -> io::Result<()> {
    let mut out = BufWriter::new(File::create(path)?);
    let time = |ms: u64| format_time(ms, utc_offset_minutes);
    let app_name = |id: &str| snapshot.apps.iter().find(|a| a.id == id).map(|a| a.name.clone()).unwrap_or_default();

    match what {
        LiveExport::Apps => {
            row(&mut out, &["App", "Program", "Path", "First seen", "Trusted", "Open connections", "Servers", "Connections opened", "Bytes sent", "Bytes received"])?;
            for app in &snapshot.apps {
                row(&mut out, &[
                    app.name.clone(),
                    app.exe.clone(),
                    app.path.clone().unwrap_or_default(),
                    time(app.first_seen),
                    if app.trusted { "yes" } else { "no" }.into(),
                    app.connections.to_string(),
                    app.servers.to_string(),
                    app.total_opened.to_string(),
                    app.bytes_sent.to_string(),
                    app.bytes_received.to_string(),
                ])?;
            }
        }
        LiveExport::Connections => {
            row(&mut out, &["App", "Protocol", "Server name", "Server address", "Server port", "Local port", "Opened", "Bytes sent", "Bytes received"])?;
            for conn in &snapshot.connections {
                row(&mut out, &[
                    app_name(&conn.app_id),
                    protocol(conn.protocol).into(),
                    conn.host.clone().unwrap_or_default(),
                    conn.remote_ip.clone(),
                    conn.remote_port.to_string(),
                    conn.local_port.to_string(),
                    time(conn.opened_at),
                    conn.bytes_sent.to_string(),
                    conn.bytes_received.to_string(),
                ])?;
            }
        }
        LiveExport::Flow => {
            row(&mut out, &["Time", "Event", "App", "Protocol", "Server name", "Server address", "Server port", "Bytes sent", "Bytes received"])?;
            for event in &snapshot.events {
                row(&mut out, &[
                    time(event.time),
                    match event.kind {
                        EventKind::Opened => "opened",
                        EventKind::Closed => "closed",
                    }
                    .into(),
                    event.app_name.clone(),
                    protocol(event.protocol).into(),
                    event.host.clone().unwrap_or_default(),
                    event.remote_ip.clone(),
                    event.remote_port.to_string(),
                    event.bytes_sent.to_string(),
                    event.bytes_received.to_string(),
                ])?;
            }
        }
    }
    out.flush()
}

fn protocol(protocol: Protocol) -> &'static str {
    match protocol {
        Protocol::Tcp => "TCP",
        Protocol::Udp => "UDP",
    }
}

fn row<S: AsRef<str>>(out: &mut impl Write, fields: &[S]) -> io::Result<()> {
    let line: Vec<String> = fields.iter().map(|f| csv_field(f.as_ref())).collect();
    writeln!(out, "{}", line.join(","))
}

pub fn csv_field(text: &str) -> String {
    if text.contains([',', '"', '\n', '\r']) { format!("\"{}\"", text.replace('"', "\"\"")) } else { text.to_string() }
}
