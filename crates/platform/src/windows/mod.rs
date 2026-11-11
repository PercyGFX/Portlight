//! Windows data sources.
//!
//! Connections come from snapshots of the TCP connection table, which needs
//! no administrator rights and no driver. Very short connections can fall
//! between two snapshots, and UDP is not covered; the planned event-based
//! source (ETW) fixes both. See docs/PLAN.md.

mod process;
mod tcp_table;

use std::io;

use portlight_core::{Connection, Platform, ProcessInfo};

pub struct WindowsPlatform;

impl Platform for WindowsPlatform {
    fn tcp_connections(&self) -> io::Result<Vec<Connection>> {
        tcp_table::connections()
    }

    fn process_info(&self, pid: u32) -> Option<ProcessInfo> {
        let path = process::path(pid)?;
        let display_name = process::display_name(&path);
        Some(ProcessInfo { path, display_name })
    }
}
