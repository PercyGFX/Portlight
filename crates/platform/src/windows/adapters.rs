//! This PC's network cards: their friendly names ("Wi-Fi") and addresses.

use std::collections::HashSet;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use windows::Win32::Foundation::{ERROR_BUFFER_OVERFLOW, NO_ERROR};
use windows::Win32::NetworkManagement::IpHelper::{
    GAA_FLAG_SKIP_ANYCAST, GAA_FLAG_SKIP_DNS_SERVER, GAA_FLAG_SKIP_MULTICAST, GetAdaptersAddresses,
    IP_ADAPTER_ADDRESSES_LH,
};
use windows::Win32::NetworkManagement::Ndis::IfOperStatusUp;
use windows::Win32::Networking::WinSock::{AF_UNSPEC, SOCKADDR};

#[derive(Debug, Clone)]
pub struct Adapter {
    /// The adapter's GUID with braces, e.g. "{4D36E972-...}". Npcap's device
    /// names end with it.
    pub guid: String,
    /// What Windows shows in Settings, e.g. "Wi-Fi".
    pub friendly_name: String,
    pub description: String,
    pub up: bool,
    pub addresses: Vec<IpAddr>,
}

/// Every network card Windows knows about.
pub fn adapters() -> Vec<Adapter> {
    let flags = GAA_FLAG_SKIP_ANYCAST | GAA_FLAG_SKIP_MULTICAST | GAA_FLAG_SKIP_DNS_SERVER;
    let mut size: u32 = 16 * 1024;
    // u64 elements keep the buffer aligned for the structures Windows writes.
    let mut buf: Vec<u64> = Vec::new();

    for _ in 0..4 {
        buf = vec![0u64; (size as usize).div_ceil(8)];
        // SAFETY: `buf` holds at least `size` bytes.
        let code = unsafe {
            GetAdaptersAddresses(
                AF_UNSPEC.0 as u32,
                flags,
                None,
                Some(buf.as_mut_ptr() as *mut IP_ADAPTER_ADDRESSES_LH),
                &mut size,
            )
        };
        if code == NO_ERROR.0 {
            break;
        }
        buf.clear();
        if code != ERROR_BUFFER_OVERFLOW.0 {
            break;
        }
    }
    if buf.is_empty() {
        return Vec::new();
    }

    let mut out = Vec::new();
    let mut next = buf.as_ptr() as *const IP_ADAPTER_ADDRESSES_LH;
    // SAFETY: Windows filled `buf` with a linked list whose pointers all point inside it.
    unsafe {
        while let Some(adapter) = next.as_ref() {
            let mut addresses = Vec::new();
            let mut unicast = adapter.FirstUnicastAddress;
            while let Some(address) = unicast.as_ref() {
                if let Some(ip) = sockaddr_ip(address.Address.lpSockaddr) {
                    addresses.push(ip);
                }
                unicast = address.Next;
            }
            out.push(Adapter {
                guid: adapter.AdapterName.to_string().unwrap_or_default(),
                friendly_name: adapter.FriendlyName.to_string().unwrap_or_default(),
                description: adapter.Description.to_string().unwrap_or_default(),
                up: adapter.OperStatus == IfOperStatusUp,
                addresses,
            });
            next = adapter.Next;
        }
    }
    out
}

/// This PC's own addresses, on every card.
pub fn local_addresses() -> HashSet<IpAddr> {
    adapters().into_iter().flat_map(|a| a.addresses).map(|ip| ip.to_canonical()).collect()
}

/// Reads the address out of a Windows SOCKADDR (IPv4 or IPv6).
///
/// # Safety
/// `addr` must be null or point at a complete sockaddr_in / sockaddr_in6.
pub unsafe fn sockaddr_ip(addr: *const SOCKADDR) -> Option<IpAddr> {
    let bytes = addr as *const u8;
    if bytes.is_null() {
        return None;
    }
    // SAFETY: the caller promises a complete structure; the family says which.
    unsafe {
        let family = u16::from_ne_bytes([*bytes, *bytes.add(1)]);
        match family {
            2 => {
                let mut octets = [0u8; 4];
                std::ptr::copy_nonoverlapping(bytes.add(4), octets.as_mut_ptr(), 4);
                Some(IpAddr::V4(Ipv4Addr::from(octets)))
            }
            23 => {
                let mut octets = [0u8; 16];
                std::ptr::copy_nonoverlapping(bytes.add(8), octets.as_mut_ptr(), 16);
                Some(IpAddr::V6(Ipv6Addr::from(octets)))
            }
            _ => None,
        }
    }
}
