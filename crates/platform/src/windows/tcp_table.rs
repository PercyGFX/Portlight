//! Reads the Windows TCP connection table, which lists the owning process ID
//! for every connection.

use std::ffi::c_void;
use std::io;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

use portlight_core::Connection;
use windows::Win32::Foundation::{ERROR_INSUFFICIENT_BUFFER, NO_ERROR};
use windows::Win32::NetworkManagement::IpHelper::{
    GetExtendedTcpTable, MIB_TCP6ROW_OWNER_PID, MIB_TCPROW_OWNER_PID, TCP_TABLE_OWNER_PID_ALL,
};
use windows::Win32::Networking::WinSock::{AF_INET, AF_INET6};

/// TCP state value Windows uses for a socket that is only listening.
const TCP_STATE_LISTEN: u32 = 2;

/// Every open TCP connection (IPv4 and IPv6), without listening sockets.
pub fn connections() -> io::Result<Vec<Connection>> {
    let mut out = Vec::new();

    for_each_row::<MIB_TCPROW_OWNER_PID>(AF_INET.0 as u32, |row| {
        if row.dwState != TCP_STATE_LISTEN {
            out.push(Connection {
                pid: row.dwOwningPid,
                local: SocketAddr::new(v4(row.dwLocalAddr), port(row.dwLocalPort)),
                remote: SocketAddr::new(v4(row.dwRemoteAddr), port(row.dwRemotePort)),
            });
        }
    })?;

    for_each_row::<MIB_TCP6ROW_OWNER_PID>(AF_INET6.0 as u32, |row| {
        if row.dwState != TCP_STATE_LISTEN {
            out.push(Connection {
                pid: row.dwOwningPid,
                local: SocketAddr::new(v6(row.ucLocalAddr), port(row.dwLocalPort)),
                remote: SocketAddr::new(v6(row.ucRemoteAddr), port(row.dwRemotePort)),
            });
        }
    })?;

    Ok(out)
}

/// Reads one address family's table and calls `f` for every row.
///
/// Windows fills a buffer with a `u32` row count followed by the rows. The
/// table can grow between the size query and the read, so retry a few times.
fn for_each_row<Row: Copy>(family: u32, mut f: impl FnMut(Row)) -> io::Result<()> {
    let mut size = 0u32;
    let mut buf: Vec<u8> = Vec::new();

    for _ in 0..5 {
        // SAFETY: `buf` is either empty (size query) or at least `size` bytes long.
        let code = unsafe {
            GetExtendedTcpTable(
                if buf.is_empty() { None } else { Some(buf.as_mut_ptr() as *mut c_void) },
                &mut size,
                false,
                family,
                TCP_TABLE_OWNER_PID_ALL,
                0,
            )
        };

        if code == NO_ERROR.0 {
            if buf.len() < 4 {
                return Ok(());
            }
            let count = u32::from_ne_bytes([buf[0], buf[1], buf[2], buf[3]]) as usize;
            // Rows start after the count, at the row type's alignment (4 bytes here).
            let first = std::mem::align_of::<Row>().max(4);
            let row_size = std::mem::size_of::<Row>();
            for i in 0..count {
                let offset = first + i * row_size;
                if offset + row_size > buf.len() {
                    break;
                }
                // SAFETY: the range `offset..offset + row_size` was checked to be inside `buf`.
                let row = unsafe { std::ptr::read_unaligned(buf.as_ptr().add(offset) as *const Row) };
                f(row);
            }
            return Ok(());
        }

        if code == ERROR_INSUFFICIENT_BUFFER.0 {
            // Leave headroom in case more connections open before the retry.
            size += 4096;
            buf = vec![0u8; size as usize];
            continue;
        }

        return Err(io::Error::from_raw_os_error(code as i32));
    }

    Err(io::Error::other("connection table kept changing size"))
}

// Windows stores addresses and ports in network byte order.
fn v4(addr: u32) -> IpAddr {
    IpAddr::V4(Ipv4Addr::from(addr.to_ne_bytes()))
}

fn v6(addr: [u8; 16]) -> IpAddr {
    IpAddr::V6(Ipv6Addr::from(addr))
}

fn port(raw: u32) -> u16 {
    u16::from_be(raw as u16)
}
