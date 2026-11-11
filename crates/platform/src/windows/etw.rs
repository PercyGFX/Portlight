//! Reads Windows' live network events (Event Tracing for Windows, ETW).
//!
//! Two built-in Windows event sources are used:
//! - `Microsoft-Windows-Kernel-Network`: every TCP connect, accept and
//!   disconnect, and every send and receive over TCP and UDP, with the process
//!   ID and the number of bytes.
//! - `Microsoft-Windows-DNS-Client`: every finished name lookup, with the
//!   name and the addresses it resolved to.
//!
//! Starting an event session needs administrator rights, which is why this
//! runs inside the Portlight Collector service and not in the window.

use std::collections::HashSet;
use std::ffi::c_void;
use std::io;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Mutex;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use portlight_core::{NetEvent, NetEventKind, Protocol, parse_dns_results};
use windows::Win32::Foundation::{ERROR_ALREADY_EXISTS, ERROR_SUCCESS, WIN32_ERROR};
use windows::Win32::System::Diagnostics::Etw::{
    CONTROLTRACE_HANDLE, CloseTrace, ControlTraceW, EVENT_CONTROL_CODE_ENABLE_PROVIDER, EVENT_RECORD,
    EVENT_TRACE_CONTROL_STOP, EVENT_TRACE_LOGFILEW, EVENT_TRACE_PROPERTIES, EVENT_TRACE_REAL_TIME_MODE,
    EnableTraceEx2, OpenTraceW, PROCESS_TRACE_MODE_EVENT_RECORD, PROCESS_TRACE_MODE_REAL_TIME,
    PROPERTY_DATA_DESCRIPTOR, ProcessTrace, StartTraceW, TRACE_LEVEL_INFORMATION, TdhGetProperty, TdhGetPropertySize,
    WNODE_FLAG_TRACED_GUID,
};
use windows::core::{GUID, PCWSTR, PWSTR};

use super::adapters;

/// What the event session reports.
#[derive(Debug, Clone)]
pub enum Feed {
    Net(NetEvent),
    /// A finished name lookup: the name and the addresses it resolved to.
    Name { name: String, ips: Vec<IpAddr> },
}

const SESSION_NAME: &str = "Portlight Collector";

/// Microsoft-Windows-Kernel-Network
const KERNEL_NETWORK: GUID = GUID::from_u128(0x7dd42a49_5329_4832_8dfd_43d979153a88);
/// Its IPv4 and IPv6 keywords.
const KERNEL_NETWORK_KEYWORDS: u64 = 0x10 | 0x20;
/// Microsoft-Windows-DNS-Client
const DNS_CLIENT: GUID = GUID::from_u128(0x1c95126e_7eea_49a9_a3fe_a378b03ddb4d);
/// "DNS query is completed for the name %1 ... Results %5"
const DNS_QUERY_COMPLETED: u16 = 3008;

/// How often the list of this PC's own addresses is read again.
const LOCAL_ADDRESSES_EVERY: Duration = Duration::from_secs(10);

/// A running event session. Dropping it stops the session.
pub struct EventSession {
    control: CONTROLTRACE_HANDLE,
    properties: Box<Properties>,
    reader: Option<JoinHandle<()>>,
}

/// Everything the callback needs, kept alive until the session stops.
struct Sink {
    on_feed: Box<dyn Fn(Feed) + Send + Sync>,
    local: Mutex<(Instant, HashSet<IpAddr>)>,
}

/// EVENT_TRACE_PROPERTIES followed by room for the session name, which
/// Windows expects in the same block of memory.
#[repr(C)]
struct Properties {
    inner: EVENT_TRACE_PROPERTIES,
    name: [u16; 64],
}

impl Properties {
    fn new() -> Box<Self> {
        // SAFETY: both fields are plain data for which all-zero is valid.
        let mut props: Box<Self> = Box::new(unsafe { std::mem::zeroed() });
        props.inner.Wnode.BufferSize = std::mem::size_of::<Properties>() as u32;
        props.inner.Wnode.Flags = WNODE_FLAG_TRACED_GUID;
        // Timestamps from the performance counter: cheap and precise.
        props.inner.Wnode.ClientContext = 1;
        props.inner.LogFileMode = EVENT_TRACE_REAL_TIME_MODE;
        props.inner.BufferSize = 64; // KB per buffer
        props.inner.MinimumBuffers = 16;
        props.inner.MaximumBuffers = 64;
        props.inner.FlushTimer = 1; // seconds
        props.inner.LoggerNameOffset = std::mem::offset_of!(Properties, name) as u32;
        props
    }
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

fn check(code: WIN32_ERROR, what: &str) -> io::Result<()> {
    if code == ERROR_SUCCESS {
        Ok(())
    } else {
        let error = io::Error::from_raw_os_error(code.0 as i32);
        Err(io::Error::new(error.kind(), format!("{what}: {error}")))
    }
}

impl EventSession {
    /// Starts the session and a thread that calls `on_feed` for every event.
    /// Fails without administrator rights.
    pub fn start(on_feed: impl Fn(Feed) + Send + Sync + 'static) -> io::Result<EventSession> {
        let name = wide(SESSION_NAME);
        let mut properties = Properties::new();
        let mut control = CONTROLTRACE_HANDLE::default();

        // SAFETY: `properties` is a correctly sized, initialised block and `name` is NUL-terminated.
        let mut code = unsafe { StartTraceW(&mut control, PCWSTR(name.as_ptr()), &mut properties.inner) };
        if code == ERROR_ALREADY_EXISTS {
            // Left over from a run that did not shut down cleanly.
            let mut old = Properties::new();
            // SAFETY: as above; stopping by name needs no handle.
            unsafe {
                let _ = ControlTraceW(
                    CONTROLTRACE_HANDLE::default(),
                    PCWSTR(name.as_ptr()),
                    &mut old.inner,
                    EVENT_TRACE_CONTROL_STOP,
                );
            }
            properties = Properties::new();
            // SAFETY: as above.
            code = unsafe { StartTraceW(&mut control, PCWSTR(name.as_ptr()), &mut properties.inner) };
        }
        check(code, "could not start the network event session")?;

        let mut session = EventSession { control, properties, reader: None };

        for (provider, keywords) in [(KERNEL_NETWORK, KERNEL_NETWORK_KEYWORDS), (DNS_CLIENT, u64::MAX)] {
            // SAFETY: `control` is the live session handle and `provider` outlives the call.
            let code = unsafe {
                EnableTraceEx2(
                    session.control,
                    &provider,
                    EVENT_CONTROL_CODE_ENABLE_PROVIDER.0,
                    TRACE_LEVEL_INFORMATION as u8,
                    keywords,
                    0,
                    0,
                    None,
                )
            };
            check(code, "could not subscribe to network events")?;
        }

        let sink: &'static Sink = Box::leak(Box::new(Sink {
            on_feed: Box::new(on_feed),
            local: Mutex::new((Instant::now(), adapters::local_addresses())),
        }));

        let mut logfile_name = wide(SESSION_NAME);
        // SAFETY: zero is a valid starting value for every field.
        let mut logfile: EVENT_TRACE_LOGFILEW = unsafe { std::mem::zeroed() };
        logfile.LoggerName = PWSTR(logfile_name.as_mut_ptr());
        logfile.Anonymous1.ProcessTraceMode = PROCESS_TRACE_MODE_REAL_TIME | PROCESS_TRACE_MODE_EVENT_RECORD;
        logfile.Anonymous2.EventRecordCallback = Some(on_event);
        logfile.Context = sink as *const Sink as *mut c_void;

        // SAFETY: `logfile` is filled in and `logfile_name` lives until OpenTraceW returns.
        let reader = unsafe { OpenTraceW(&mut logfile) };
        if reader.Value == u64::MAX {
            return Err(io::Error::last_os_error());
        }

        session.reader = Some(
            thread::Builder::new().name("portlight-etw".into()).spawn(move || {
                // SAFETY: `reader` is an open trace; ProcessTrace blocks until the session stops.
                unsafe {
                    let _ = ProcessTrace(&[reader], None, None);
                    let _ = CloseTrace(reader);
                }
                // The callback can no longer run, so the sink can go.
                // SAFETY: `sink` came from Box::leak above and nothing else uses it now.
                drop(unsafe { Box::from_raw(sink as *const Sink as *mut Sink) });
            })?,
        );
        Ok(session)
    }

    /// True while events are still being read.
    pub fn is_running(&self) -> bool {
        self.reader.as_ref().is_some_and(|reader| !reader.is_finished())
    }
}

impl Drop for EventSession {
    fn drop(&mut self) {
        // SAFETY: `control` is our session handle; stopping makes ProcessTrace return.
        unsafe {
            let _ = ControlTraceW(self.control, PCWSTR::null(), &mut self.properties.inner, EVENT_TRACE_CONTROL_STOP);
        }
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

unsafe extern "system" fn on_event(record: *mut EVENT_RECORD) {
    // SAFETY: Windows passes a valid record whose UserContext is our Sink.
    let record = unsafe { &*record };
    let Some(sink) = (unsafe { (record.UserContext as *const Sink).as_ref() }) else { return };

    let provider = record.EventHeader.ProviderId;
    if provider == KERNEL_NETWORK {
        // SAFETY: UserData points at UserDataLength bytes for the life of the callback.
        let data = unsafe { std::slice::from_raw_parts(record.UserData as *const u8, record.UserDataLength as usize) };
        if let Some(event) = parse_network(record.EventHeader.EventDescriptor.Id, data, |ip| sink.is_local(ip)) {
            (sink.on_feed)(Feed::Net(event));
        }
    } else if provider == DNS_CLIENT
        && record.EventHeader.EventDescriptor.Id == DNS_QUERY_COMPLETED
        && let Some((name, ips)) = parse_dns(record)
    {
        (sink.on_feed)(Feed::Name { name, ips });
    }
}

impl Sink {
    fn is_local(&self, ip: IpAddr) -> bool {
        let mut local = self.local.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if local.0.elapsed() > LOCAL_ADDRESSES_EVERY {
            *local = (Instant::now(), adapters::local_addresses());
        }
        local.1.contains(&ip.to_canonical())
    }
}

/// Reads a Kernel-Network event. Every event used here starts with the same
/// fields: process ID, size, then the two addresses and the two ports (in
/// network byte order). The event message calls them "from %4:%6 to %3:%5".
fn parse_network(id: u16, data: &[u8], is_local: impl Fn(IpAddr) -> bool) -> Option<NetEvent> {
    use NetEventKind::*;
    let (kind, protocol, v6) = match id {
        10 => (Send, Protocol::Tcp, false),
        11 => (Receive, Protocol::Tcp, false),
        12 | 16 => (Connect, Protocol::Tcp, false),
        13 => (Disconnect, Protocol::Tcp, false),
        15 => (Accept, Protocol::Tcp, false),
        26 => (Send, Protocol::Tcp, true),
        27 => (Receive, Protocol::Tcp, true),
        28 | 32 => (Connect, Protocol::Tcp, true),
        29 => (Disconnect, Protocol::Tcp, true),
        31 => (Accept, Protocol::Tcp, true),
        42 => (Send, Protocol::Udp, false),
        43 => (Receive, Protocol::Udp, false),
        58 => (Send, Protocol::Udp, true),
        59 => (Receive, Protocol::Udp, true),
        _ => return None,
    };

    let u32_at = |at: usize| data.get(at..at + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    let port_at = |at: usize| data.get(at..at + 2).map(|b| u16::from_be_bytes([b[0], b[1]]));
    let pid = u32_at(0)?;
    let size = u32_at(4)?;

    let (daddr, saddr, ports_at) = if v6 {
        let a: [u8; 16] = data.get(8..24)?.try_into().ok()?;
        let b: [u8; 16] = data.get(24..40)?.try_into().ok()?;
        (IpAddr::V6(Ipv6Addr::from(a)), IpAddr::V6(Ipv6Addr::from(b)), 40)
    } else {
        let a: [u8; 4] = data.get(8..12)?.try_into().ok()?;
        let b: [u8; 4] = data.get(12..16)?.try_into().ok()?;
        (IpAddr::V4(Ipv4Addr::from(a)), IpAddr::V4(Ipv4Addr::from(b)), 16)
    };
    let dport = port_at(ports_at)?;
    let sport = port_at(ports_at + 2)?;

    // "saddr" is this PC's side for data sent. Rather than trust the naming
    // for every event, pick whichever side is one of this PC's addresses.
    let d = SocketAddr::new(daddr.to_canonical(), dport);
    let s = SocketAddr::new(saddr.to_canonical(), sport);
    let (local, remote) = if is_local(d.ip()) && !is_local(s.ip()) { (d, s) } else { (s, d) };

    Some(NetEvent { kind, protocol, pid, local, remote, size })
}

/// Reads the name and addresses from a finished DNS lookup. These events are
/// rare, so the slower by-name property lookup is fine here.
fn parse_dns(record: &EVENT_RECORD) -> Option<(String, Vec<IpAddr>)> {
    let status = property(record, "QueryStatus")?;
    if status.len() >= 4 && u32::from_le_bytes([status[0], status[1], status[2], status[3]]) != 0 {
        return None;
    }
    let name = utf16(&property(record, "QueryName")?);
    let ips = parse_dns_results(&utf16(&property(record, "QueryResults")?));
    (!name.is_empty() && !ips.is_empty()).then_some((name, ips))
}

fn property(record: &EVENT_RECORD, name: &str) -> Option<Vec<u8>> {
    let name = wide(name);
    let descriptor =
        [PROPERTY_DATA_DESCRIPTOR { PropertyName: name.as_ptr() as u64, ArrayIndex: u32::MAX, Reserved: 0 }];
    let mut size = 0u32;
    // SAFETY: `record` is the live event and `descriptor` points at a NUL-terminated name.
    unsafe {
        if TdhGetPropertySize(record, None, &descriptor, &mut size) != 0 || size == 0 || size > 64 * 1024 {
            return None;
        }
        let mut buffer = vec![0u8; size as usize];
        (TdhGetProperty(record, None, &descriptor, &mut buffer) == 0).then_some(buffer)
    }
}

fn utf16(bytes: &[u8]) -> String {
    let units: Vec<u16> =
        bytes.as_chunks::<2>().0.iter().map(|pair| u16::from_le_bytes(*pair)).take_while(|u| *u != 0).collect();
    String::from_utf16_lossy(&units)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v4_event(pid: u32, size: u32, daddr: [u8; 4], saddr: [u8; 4], dport: u16, sport: u16) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend(pid.to_le_bytes());
        data.extend(size.to_le_bytes());
        data.extend(daddr);
        data.extend(saddr);
        data.extend(dport.to_be_bytes());
        data.extend(sport.to_be_bytes());
        data.extend([0u8; 16]); // the fields after the ports are not used
        data
    }

    const ME: [u8; 4] = [192, 168, 1, 10];
    const SERVER: [u8; 4] = [140, 82, 112, 3];
    fn is_me(ip: IpAddr) -> bool {
        ip == IpAddr::from(ME)
    }

    #[test]
    fn reads_a_tcp_send() {
        let data = v4_event(1234, 517, SERVER, ME, 443, 50_123);
        let event = parse_network(10, &data, is_me).unwrap();
        assert_eq!((event.kind, event.protocol, event.pid, event.size), (NetEventKind::Send, Protocol::Tcp, 1234, 517));
        assert_eq!(event.local, SocketAddr::from((ME, 50_123)));
        assert_eq!(event.remote, SocketAddr::from((SERVER, 443)));
    }

    #[test]
    fn this_pc_is_always_the_local_side_whichever_way_the_fields_point() {
        let flipped = v4_event(1, 60, ME, SERVER, 50_123, 443);
        let event = parse_network(43, &flipped, is_me).unwrap();
        assert_eq!((event.kind, event.protocol), (NetEventKind::Receive, Protocol::Udp));
        assert_eq!(event.local, SocketAddr::from((ME, 50_123)));
        assert_eq!(event.remote, SocketAddr::from((SERVER, 443)));
    }

    #[test]
    fn reads_ipv6_and_ignores_short_or_unknown_events() {
        let mut data = Vec::new();
        data.extend(9u32.to_le_bytes());
        data.extend(0u32.to_le_bytes());
        data.extend("2606:50c0::1".parse::<Ipv6Addr>().unwrap().octets());
        data.extend("2001:db8::10".parse::<Ipv6Addr>().unwrap().octets());
        data.extend(443u16.to_be_bytes());
        data.extend(50_000u16.to_be_bytes());
        let event = parse_network(28, &data, |_| false).unwrap();
        assert_eq!(event.kind, NetEventKind::Connect);
        assert_eq!(event.remote, "[2606:50c0::1]:443".parse().unwrap());

        assert!(parse_network(10, &data[..10], |_| false).is_none());
        assert!(parse_network(17, &data, |_| false).is_none());
    }
}
