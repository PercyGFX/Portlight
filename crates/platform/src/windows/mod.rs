//! Windows data sources.
//!
//! Without administrator rights, connections come from snapshots of the TCP
//! table (`tcp_table`). The Portlight Collector service, which runs with
//! them, reads Windows' live network events instead (`etw`) and passes them
//! to the window over a named pipe (`pipe`). Packet capture goes through
//! Npcap when it is installed (`npcap`).

pub mod adapters;
pub mod etw;
pub mod npcap;
pub mod pipe;
pub mod process;
pub mod service;
mod tcp_table;

use std::io;
use std::net::SocketAddr;

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

/// Every open UDP port and the process that owns it.
pub fn udp_ports() -> io::Result<Vec<(SocketAddr, u32)>> {
    tcp_table::udp_ports()
}
