//! Packet capture through Npcap, the capture driver Wireshark uses.
//!
//! Npcap's licence does not allow bundling it, so Portlight loads its library
//! (`wpcap.dll`) at run time if it is installed, and works without it
//! otherwise. Only the few libpcap functions Portlight needs are loaded.

use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::Arc;

use libloading::os::windows::{LOAD_WITH_ALTERED_SEARCH_PATH, Library};
use windows::Win32::Networking::WinSock::SOCKADDR;
use windows::Win32::System::Registry::{HKEY_LOCAL_MACHINE, RRF_RT_REG_DWORD, RegGetValueW};
use windows::core::PCWSTR;

use super::adapters::sockaddr_ip;

const ERRBUF_SIZE: usize = 256;
const PCAP_IF_LOOPBACK: u32 = 0x1;
const PCAP_IF_UP: u32 = 0x2;
const PCAP_IF_CONNECTION_STATUS: u32 = 0x30;
const PCAP_IF_CONNECTION_STATUS_DISCONNECTED: u32 = 0x20;

#[repr(C)]
struct PcapIf {
    next: *mut PcapIf,
    name: *mut c_char,
    description: *mut c_char,
    addresses: *mut PcapAddr,
    flags: u32,
}

#[repr(C)]
struct PcapAddr {
    next: *mut PcapAddr,
    addr: *mut SOCKADDR,
    netmask: *mut SOCKADDR,
    broadaddr: *mut SOCKADDR,
    dstaddr: *mut SOCKADDR,
}

/// `struct pcap_pkthdr` on Windows, where `long` is 32 bits.
#[repr(C)]
struct PacketHeader {
    tv_sec: i32,
    tv_usec: i32,
    caplen: u32,
    len: u32,
}

#[repr(C)]
struct BpfProgram {
    bf_len: u32,
    bf_insns: *mut c_void,
}

type Pcap = c_void;

/// Why capture is not available, in words a person can act on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Problem {
    NotInstalled,
    AdminOnly,
    Broken(String),
}

impl std::fmt::Display for Problem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Problem::NotInstalled => write!(f, "Npcap is not installed."),
            Problem::AdminOnly => write!(
                f,
                "Npcap is set to let only administrators capture. Reinstall Npcap without \"Restrict Npcap driver's access to Administrators only\", or run Portlight as administrator."
            ),
            Problem::Broken(detail) => write!(f, "Npcap could not be used: {detail}"),
        }
    }
}

struct Functions {
    findalldevs: unsafe extern "C" fn(*mut *mut PcapIf, *mut c_char) -> c_int,
    freealldevs: unsafe extern "C" fn(*mut PcapIf),
    create: unsafe extern "C" fn(*const c_char, *mut c_char) -> *mut Pcap,
    set_snaplen: unsafe extern "C" fn(*mut Pcap, c_int) -> c_int,
    set_promisc: unsafe extern "C" fn(*mut Pcap, c_int) -> c_int,
    set_timeout: unsafe extern "C" fn(*mut Pcap, c_int) -> c_int,
    set_immediate_mode: unsafe extern "C" fn(*mut Pcap, c_int) -> c_int,
    set_buffer_size: unsafe extern "C" fn(*mut Pcap, c_int) -> c_int,
    activate: unsafe extern "C" fn(*mut Pcap) -> c_int,
    datalink: unsafe extern "C" fn(*mut Pcap) -> c_int,
    compile: unsafe extern "C" fn(*mut Pcap, *mut BpfProgram, *const c_char, c_int, u32) -> c_int,
    setfilter: unsafe extern "C" fn(*mut Pcap, *mut BpfProgram) -> c_int,
    freecode: unsafe extern "C" fn(*mut BpfProgram),
    next_ex: unsafe extern "C" fn(*mut Pcap, *mut *mut PacketHeader, *mut *const u8) -> c_int,
    geterr: unsafe extern "C" fn(*mut Pcap) -> *mut c_char,
    stats: unsafe extern "C" fn(*mut Pcap, *mut [u32; 8]) -> c_int,
    close: unsafe extern "C" fn(*mut Pcap),
    lib_version: unsafe extern "C" fn() -> *const c_char,
}

/// The loaded Npcap library. Cheap to clone and share between threads.
#[derive(Clone)]
pub struct Npcap {
    inner: Arc<(Library, Functions)>,
}

/// A network card Npcap can capture on.
#[derive(Debug, Clone)]
pub struct Device {
    /// e.g. `\Device\NPF_{4D36E972-...}`
    pub name: String,
    pub description: String,
    pub addresses: Vec<IpAddr>,
    pub loopback: bool,
    pub up: bool,
    pub connected: bool,
}

impl Npcap {
    /// Loads Npcap if it is installed.
    pub fn load() -> Result<Npcap, Problem> {
        let dll = npcap_dir().join("wpcap.dll");
        if !dll.exists() {
            return Err(Problem::NotInstalled);
        }
        // SAFETY: loading Npcap's own library; LOAD_WITH_ALTERED_SEARCH_PATH
        // makes it find its helper Packet.dll in the same folder.
        let library = unsafe { Library::load_with_flags(&dll, LOAD_WITH_ALTERED_SEARCH_PATH) }
            .map_err(|e| Problem::Broken(e.to_string()))?;

        fn symbol<T: Copy>(library: &Library, name: &str) -> Result<T, Problem> {
            // SAFETY: each field's type matches libpcap's documented prototype.
            unsafe { library.get::<T>(name.as_bytes()) }
                .map(|found| *found)
                .map_err(|e| Problem::Broken(format!("{name} is missing: {e}")))
        }
        macro_rules! load {
            ($name:literal) => {
                symbol(&library, $name)?
            };
        }
        let functions = Functions {
            findalldevs: load!("pcap_findalldevs"),
            freealldevs: load!("pcap_freealldevs"),
            create: load!("pcap_create"),
            set_snaplen: load!("pcap_set_snaplen"),
            set_promisc: load!("pcap_set_promisc"),
            set_timeout: load!("pcap_set_timeout"),
            set_immediate_mode: load!("pcap_set_immediate_mode"),
            set_buffer_size: load!("pcap_set_buffer_size"),
            activate: load!("pcap_activate"),
            datalink: load!("pcap_datalink"),
            compile: load!("pcap_compile"),
            setfilter: load!("pcap_setfilter"),
            freecode: load!("pcap_freecode"),
            next_ex: load!("pcap_next_ex"),
            geterr: load!("pcap_geterr"),
            stats: load!("pcap_stats"),
            close: load!("pcap_close"),
            lib_version: load!("pcap_lib_version"),
        };
        Ok(Npcap { inner: Arc::new((library, functions)) })
    }

    fn f(&self) -> &Functions {
        &self.inner.1
    }

    /// e.g. "Npcap version 1.80, based on libpcap version 1.10.5"
    pub fn version(&self) -> String {
        // SAFETY: returns a static NUL-terminated string.
        unsafe { text((self.f().lib_version)()) }
    }

    /// Every network card Npcap can see.
    pub fn devices(&self) -> Result<Vec<Device>, Problem> {
        let mut errbuf = [0 as c_char; ERRBUF_SIZE];
        let mut first: *mut PcapIf = std::ptr::null_mut();
        // SAFETY: `errbuf` is PCAP_ERRBUF_SIZE long; the list is freed below.
        let code = unsafe { (self.f().findalldevs)(&mut first, errbuf.as_mut_ptr()) };
        if code != 0 {
            // SAFETY: libpcap wrote a NUL-terminated message into `errbuf`.
            let message = unsafe { text(errbuf.as_ptr()) };
            return Err(if admin_only() { Problem::AdminOnly } else { Problem::Broken(message) });
        }

        let mut devices = Vec::new();
        let mut next = first;
        // SAFETY: walking the list libpcap returned, then freeing it once.
        unsafe {
            while let Some(device) = next.as_ref() {
                let mut addresses = Vec::new();
                let mut address = device.addresses;
                while let Some(entry) = address.as_ref() {
                    if let Some(ip) = sockaddr_ip(entry.addr) {
                        addresses.push(ip);
                    }
                    address = entry.next;
                }
                devices.push(Device {
                    name: text(device.name),
                    description: text(device.description),
                    addresses,
                    loopback: device.flags & PCAP_IF_LOOPBACK != 0,
                    up: device.flags & PCAP_IF_UP != 0,
                    connected: device.flags & PCAP_IF_CONNECTION_STATUS != PCAP_IF_CONNECTION_STATUS_DISCONNECTED,
                });
                next = device.next;
            }
            if !first.is_null() {
                (self.f().freealldevs)(first);
            }
        }
        if devices.is_empty() && admin_only() {
            return Err(Problem::AdminOnly);
        }
        Ok(devices)
    }

    /// Starts capturing on one card. `filter` is an optional BPF capture
    /// filter ("tcp port 443"). Never promiscuous: only this PC's traffic.
    pub fn open(&self, device: &str, filter: &str) -> Result<Handle, String> {
        let f = self.f();
        let name = CString::new(device).map_err(|_| "bad device name".to_string())?;
        let mut errbuf = [0 as c_char; ERRBUF_SIZE];
        // SAFETY: `name` and `errbuf` are valid for the call.
        let pcap = unsafe { (f.create)(name.as_ptr(), errbuf.as_mut_ptr()) };
        if pcap.is_null() {
            // SAFETY: libpcap wrote a NUL-terminated message.
            return Err(unsafe { text(errbuf.as_ptr()) });
        }
        let handle = Handle { npcap: self.clone(), pcap };

        // SAFETY: `pcap` is a fresh, not yet activated handle.
        unsafe {
            (f.set_snaplen)(pcap, 65_535);
            (f.set_promisc)(pcap, 0);
            (f.set_timeout)(pcap, 100);
            (f.set_immediate_mode)(pcap, 1);
            (f.set_buffer_size)(pcap, 16 * 1024 * 1024);
            let code = (f.activate)(pcap);
            if code < 0 {
                let message = handle.error();
                return Err(if admin_only() { Problem::AdminOnly.to_string() } else { message });
            }
        }

        let filter = filter.trim();
        if !filter.is_empty() {
            let text = CString::new(filter).map_err(|_| "The capture filter has a stray character.".to_string())?;
            let mut program = BpfProgram { bf_len: 0, bf_insns: std::ptr::null_mut() };
            // SAFETY: `program` is freed after being set; netmask 0 is "unknown".
            unsafe {
                if (f.compile)(pcap, &mut program, text.as_ptr(), 1, 0) != 0 {
                    return Err(format!("The capture filter is not valid: {}", handle.error()));
                }
                let code = (f.setfilter)(pcap, &mut program);
                (f.freecode)(&mut program);
                if code != 0 {
                    return Err(format!("The capture filter could not be used: {}", handle.error()));
                }
            }
        }
        Ok(handle)
    }
}

/// One open capture. Use it from a single thread.
pub struct Handle {
    npcap: Npcap,
    pcap: *mut Pcap,
}

// SAFETY: a pcap handle may move between threads as long as only one uses it at a time.
unsafe impl Send for Handle {}

/// One captured packet, borrowed until the next call to [`Handle::next`].
pub struct Packet<'a> {
    /// Microseconds since 1970.
    pub timestamp_us: u64,
    /// Length on the wire; `data` may be shorter.
    pub original_len: u32,
    pub data: &'a [u8],
}

impl Handle {
    /// pcap link type (DLT), e.g. 1 for Ethernet.
    pub fn datalink(&self) -> u32 {
        // SAFETY: `pcap` is an active handle.
        unsafe { (self.npcap.f().datalink)(self.pcap) as u32 }
    }

    /// The next packet, or `None` after a short wait with nothing to read.
    pub fn next_packet(&mut self) -> Result<Option<Packet<'_>>, String> {
        let mut header: *mut PacketHeader = std::ptr::null_mut();
        let mut data: *const u8 = std::ptr::null();
        // SAFETY: `pcap` is active; on success both pointers are valid until the next call.
        unsafe {
            match (self.npcap.f().next_ex)(self.pcap, &mut header, &mut data) {
                1 => {
                    let header = &*header;
                    let caplen = header.caplen as usize;
                    Ok(Some(Packet {
                        timestamp_us: header.tv_sec as u32 as u64 * 1_000_000 + header.tv_usec as u32 as u64,
                        original_len: header.len,
                        data: if data.is_null() { &[] } else { std::slice::from_raw_parts(data, caplen) },
                    }))
                }
                0 => Ok(None),
                _ => Err(self.error()),
            }
        }
    }

    /// Packets seen and packets the driver had to drop, so far.
    pub fn stats(&self) -> (u32, u32) {
        let mut stats = [0u32; 8];
        // SAFETY: `stats` is larger than struct pcap_stat.
        if unsafe { (self.npcap.f().stats)(self.pcap, &mut stats) } == 0 { (stats[0], stats[1]) } else { (0, 0) }
    }

    fn error(&self) -> String {
        // SAFETY: returns a NUL-terminated message owned by the handle.
        unsafe { text((self.npcap.f().geterr)(self.pcap)) }
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: the handle is closed once and not used after.
        unsafe { (self.npcap.f().close)(self.pcap) }
    }
}

/// Reads a C string, tolerating null.
unsafe fn text(ptr: *const c_char) -> String {
    if ptr.is_null() {
        String::new()
    } else {
        // SAFETY: the caller passes a NUL-terminated string.
        unsafe { CStr::from_ptr(ptr) }.to_string_lossy().into_owned()
    }
}

fn npcap_dir() -> PathBuf {
    let windows = std::env::var_os("SystemRoot").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
    windows.join("System32").join("Npcap")
}

/// True when Npcap was installed with "Restrict Npcap driver's access to
/// Administrators only".
fn admin_only() -> bool {
    let key: Vec<u16> = r"SYSTEM\CurrentControlSet\Services\npcap\Parameters".encode_utf16().chain([0]).collect();
    let value: Vec<u16> = "AdminOnly".encode_utf16().chain([0]).collect();
    let mut data = 0u32;
    let mut size = 4u32;
    // SAFETY: `data` is a 4-byte buffer and `size` says so.
    let code = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            PCWSTR(key.as_ptr()),
            PCWSTR(value.as_ptr()),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut data as *mut u32 as *mut c_void),
            Some(&mut size),
        )
    };
    code.is_ok() && data == 1
}
