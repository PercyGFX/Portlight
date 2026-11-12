//! Packet capture: records packets with Npcap, keeps the ones the person
//! asked for (one app, one server, one connection or everything), and hands
//! the window rows, breakdowns and files.
//!
//! Packets don't say which app sent them. Each one is matched to an app by
//! its local port, using the connections the tracker knows and the UDP
//! table. A packet for a connection that opened a moment ago may arrive
//! before the tracker has seen it, so packets that can't be matched yet wait
//! briefly before an app capture gives up on them.

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufWriter, Write};
use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, RwLock, RwLockReadGuard};
use std::thread;
use std::time::{Duration, Instant};

use portlight_core::{Platform, Protocol, Tracker, describe_process};
use portlight_packets::filter::{Filter, PacketRef};
use portlight_packets::{
    Context, Endpoints, FrameInfo, Layer, LinkType, Summary, Transport, dissect, endpoints, pcapng, summarize,
};
use portlight_platform::npcap::{Npcap, Problem};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

use crate::state::now_ms;

/// Event the window listens to. Its payload is a `CaptureStatus`.
pub const STATUS_EVENT: &str = "capture:status";

const MAX_PACKETS: usize = 200_000;
const MAX_BYTES: u64 = 512 * 1024 * 1024;
/// How long a packet may wait for its app to become known.
const WAIT_FOR_APP: Duration = Duration::from_secs(2);
/// Ports stay matched to their app this long after the connection closes,
/// for the last packets of a goodbye.
const PORT_MEMORY_MS: u64 = 5_000;
const STATUS_EVERY: Duration = Duration::from_millis(250);
const LOCAL_ADDRESSES_EVERY: Duration = Duration::from_secs(10);

// ---------- Shapes the window sees (keep in step with src/lib/types.ts) ----------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase", rename_all_fields = "camelCase")]
pub enum CaptureTarget {
    All,
    App { app_id: String, name: String },
    Host { ip: String, name: Option<String> },
    Domain { name: String },
    Connection { protocol: Protocol, local_port: u16, remote_ip: String, remote_port: u16, app_name: String, host: Option<String> },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureStartOptions {
    pub target: CaptureTarget,
    #[serde(default)]
    pub interfaces: Vec<String>,
    #[serde(default)]
    pub capture_filter: String,
    #[serde(default)]
    pub stop_after: StopAfter,
}

/// Optional limits that end a capture by themselves, like Wireshark's
/// "Stop capture automatically after...". Empty means run until stopped.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StopAfter {
    pub packets: Option<u32>,
    pub seconds: Option<u32>,
    pub megabytes: Option<u32>,
}

/// Which packets an export covers.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Selection {
    All,
    /// The ones matching the display filter.
    Shown,
    /// These frame numbers.
    Packets { numbers: Vec<u32> },
    /// Frame numbers `from` to `to`, both included.
    Range { from: u32, to: u32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExportFormat {
    /// For Wireshark.
    Pcapng,
    /// The packet list.
    Csv,
    /// Every packet's breakdown, readable, for reports or pasting into a chat.
    Text,
    /// Every packet's breakdown, for programs.
    Json,
}

impl ExportFormat {
    pub fn extension(self) -> &'static str {
        match self {
            ExportFormat::Pcapng => "pcapng",
            ExportFormat::Csv => "csv",
            ExportFormat::Text => "txt",
            ExportFormat::Json => "json",
        }
    }

    pub fn kind(self) -> &'static str {
        match self {
            ExportFormat::Pcapng => "Wireshark capture",
            ExportFormat::Csv => "CSV (spreadsheet)",
            ExportFormat::Text => "Text",
            ExportFormat::Json => "JSON",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureInterface {
    pub id: String,
    pub name: String,
    pub description: String,
    pub addresses: Vec<String>,
    pub active: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureSupport {
    pub available: bool,
    pub npcap_version: Option<String>,
    pub interfaces: Vec<CaptureInterface>,
    pub problem: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CaptureState {
    Idle,
    Running,
    Stopped,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureStatus {
    pub state: CaptureState,
    pub target: Option<CaptureTarget>,
    pub started_at: Option<u64>,
    pub stopped_at: Option<u64>,
    pub packets: u32,
    pub shown: u32,
    pub bytes: u64,
    pub dropped: u64,
    pub filter: String,
    pub filter_error: Option<String>,
    pub limit_reached: bool,
    pub error: Option<String>,
    pub stop_after: StopAfter,
    /// Why the capture stopped by itself, in plain words.
    pub stop_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PacketRow {
    pub number: u32,
    pub time: f64,
    #[serde(flatten)]
    pub summary: Summary,
    pub app: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PacketDetail {
    pub number: u32,
    pub time: f64,
    pub timestamp: f64,
    pub length: u32,
    pub captured_length: u32,
    pub interface_name: String,
    pub app: Option<String>,
    pub layers: Vec<Layer>,
    pub bytes: Vec<u8>,
}

// ---------- What is known about this PC, for matching and naming ----------

#[derive(Clone)]
struct Owner {
    app_id: Arc<str>,
    name: Arc<str>,
}

#[derive(Default)]
struct Knowledge {
    local: HashSet<IpAddr>,
    local_read_at: Option<Instant>,
    names: HashMap<IpAddr, String>,
    names_version: u64,
    /// (transport, local port) → app, with when it was last confirmed.
    ports: HashMap<(Transport, u16), (Owner, u64)>,
    /// Programs behind UDP-only process IDs, so they are looked up once.
    pid_owners: HashMap<u32, Owner>,
    /// Addresses the captured domain resolves to.
    domain_ips: HashSet<IpAddr>,
}

impl Context for Knowledge {
    fn is_local(&self, ip: IpAddr) -> bool {
        self.local.contains(&ip.to_canonical())
    }

    fn name_of(&self, ip: IpAddr) -> Option<String> {
        self.names.get(&ip.to_canonical()).cloned()
    }
}

impl Knowledge {
    fn owner(&self, ep: &Endpoints) -> Option<Owner> {
        let ports = match (ep.src_port, ep.dst_port) {
            (Some(s), Some(d)) => (s, d),
            _ => return None,
        };
        let (first, second) =
            if self.is_local(ep.dst) && !self.is_local(ep.src) { (ports.1, ports.0) } else { (ports.0, ports.1) };
        let find = |port| self.ports.get(&(ep.transport, port)).map(|(owner, _)| owner.clone());
        find(first).or_else(|| if self.is_local(ep.src) || self.is_local(ep.dst) { None } else { find(second) })
    }
}

// ---------- The capture itself ----------

struct Stored {
    timestamp_us: u64,
    iface: u16,
    original_len: u32,
    data: Box<[u8]>,
    app: Option<Arc<str>>,
}

struct Interface {
    name: String,
    friendly: String,
    link: LinkType,
}

struct Waiting {
    packet: Stored,
    endpoints: Endpoints,
    since: Instant,
}

struct Session {
    generation: u64,
    state: CaptureState,
    target: Option<CaptureTarget>,
    started_at: Option<u64>,
    stopped_at: Option<u64>,
    first_us: Option<u64>,
    packets: Vec<Stored>,
    bytes: u64,
    dropped_full: u64,
    driver_dropped: HashMap<u16, u64>,
    interfaces: Vec<Interface>,
    waiting: Vec<Waiting>,
    filter_text: String,
    filter: Filter,
    filter_error: Option<String>,
    shown: Vec<u32>,
    limit_reached: bool,
    error: Option<String>,
    stop: Arc<AtomicBool>,
    /// For domain captures: name lookups for the domain count as its traffic.
    dns_filter: Option<Filter>,
    stop_after: StopAfter,
    stop_reason: Option<String>,
    started: Option<Instant>,
}

impl Session {
    fn empty(generation: u64) -> Self {
        Session {
            generation,
            state: CaptureState::Idle,
            target: None,
            started_at: None,
            stopped_at: None,
            first_us: None,
            packets: Vec::new(),
            bytes: 0,
            dropped_full: 0,
            driver_dropped: HashMap::new(),
            interfaces: Vec::new(),
            waiting: Vec::new(),
            filter_text: String::new(),
            filter: Filter::parse("").expect("an empty filter always parses"),
            filter_error: None,
            shown: Vec::new(),
            limit_reached: false,
            error: None,
            stop: Arc::new(AtomicBool::new(true)),
            dns_filter: None,
            stop_after: StopAfter::default(),
            stop_reason: None,
            started: None,
        }
    }

    fn status(&self) -> CaptureStatus {
        CaptureStatus {
            state: self.state,
            target: self.target.clone(),
            started_at: self.started_at,
            stopped_at: self.stopped_at,
            packets: self.packets.len() as u32,
            shown: self.shown.len() as u32,
            bytes: self.bytes,
            dropped: self.dropped_full + self.driver_dropped.values().sum::<u64>(),
            filter: self.filter_text.clone(),
            filter_error: self.filter_error.clone(),
            limit_reached: self.limit_reached,
            error: self.error.clone(),
            stop_after: self.stop_after,
            stop_reason: self.stop_reason.clone(),
        }
    }

    /// Stops the capture if one of the person's stop conditions is met.
    fn check_stop_after(&mut self) {
        if self.state != CaptureState::Running {
            return;
        }
        let limits = self.stop_after;
        let reason = if limits.packets.is_some_and(|n| self.packets.len() as u64 >= u64::from(n)) {
            limits.packets.map(|n| format!("Stopped after {n} packets, as you asked."))
        } else if limits.megabytes.is_some_and(|mb| self.bytes >= u64::from(mb) * 1024 * 1024) {
            limits.megabytes.map(|mb| format!("Stopped after {mb} MB, as you asked."))
        } else if let (Some(seconds), Some(started)) = (limits.seconds, self.started)
            && started.elapsed() >= Duration::from_secs(u64::from(seconds))
        {
            Some(format!("Stopped after {seconds} seconds, as you asked."))
        } else {
            None
        };
        if reason.is_some() {
            self.stop_reason = reason;
            self.stop();
        }
    }

    fn link_of(&self, packet: &Stored) -> LinkType {
        self.interfaces.get(packet.iface as usize).map_or(LinkType::Ethernet, |i| i.link)
    }

    fn time_of(&self, packet: &Stored) -> f64 {
        packet.timestamp_us.saturating_sub(self.first_us.unwrap_or(packet.timestamp_us)) as f64 / 1e6
    }

    fn matches(&self, index: usize, knowledge: &Knowledge) -> bool {
        if self.filter.is_empty() {
            return true;
        }
        let packet = &self.packets[index];
        let link = self.interfaces.get(packet.iface as usize).map_or(LinkType::Ethernet, |i| i.link);
        self.filter.matches(&PacketRef {
            link,
            frame: &packet.data,
            original_len: packet.original_len,
            number: index as u32 + 1,
            app: packet.app.as_deref(),
            ctx: knowledge,
        })
    }

    fn stop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if self.state == CaptureState::Running {
            self.state = CaptureState::Stopped;
            self.stopped_at = Some(now_ms());
        }
        self.waiting.clear();
    }
}

pub struct Capture {
    session: Mutex<Session>,
    knowledge: RwLock<Knowledge>,
    npcap: Mutex<Option<Npcap>>,
    generations: AtomicU64,
    running: AtomicBool,
}

impl Default for Capture {
    fn default() -> Self {
        Capture {
            session: Mutex::new(Session::empty(0)),
            knowledge: RwLock::new(Knowledge::default()),
            npcap: Mutex::new(None),
            generations: AtomicU64::new(0),
            running: AtomicBool::new(false),
        }
    }
}

impl Capture {
    fn session(&self) -> MutexGuard<'_, Session> {
        self.session.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn knowledge(&self) -> RwLockReadGuard<'_, Knowledge> {
        self.knowledge.read().unwrap_or_else(|p| p.into_inner())
    }

    /// True while a capture is recording, so the monitor loop keeps the
    /// port-to-app table fresh.
    pub fn is_active(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// Copies what the tracker knows: which local port belongs to which app,
    /// and every address's name. Called by the monitor loop while recording.
    pub fn refresh(&self, tracker: &Tracker) {
        let now = now_ms();
        let platform = portlight_platform::system();
        let domain = match &self.session().target {
            Some(CaptureTarget::Domain { name }) => Some(name.to_lowercase()),
            _ => None,
        };
        let udp = portlight_platform::udp_ports().unwrap_or_default();

        let mut k = self.knowledge.write().unwrap_or_else(|p| p.into_inner());
        if k.local_read_at.is_none_or(|at| at.elapsed() >= LOCAL_ADDRESSES_EVERY) {
            k.local = portlight_platform::adapters::local_addresses();
            k.local_read_at = Some(Instant::now());
        }
        if tracker.names_version() != k.names_version || k.names.is_empty() {
            k.names = tracker.names();
            k.names_version = tracker.names_version();
            if let Some(domain) = &domain {
                k.domain_ips = name_matches(&k.names, domain);
            }
        }

        for (conn, app_id) in tracker.open_connections() {
            if conn.protocol != Protocol::Tcp {
                continue;
            }
            let name = tracker.app_name(app_id).unwrap_or(app_id);
            let owner = Owner { app_id: app_id.into(), name: name.into() };
            k.ports.insert((Transport::Tcp, conn.local.port()), (owner, now));
        }

        let live_pids: HashSet<u32> = udp.iter().map(|(_, pid)| *pid).collect();
        k.pid_owners.retain(|pid, _| live_pids.contains(pid));
        for (local, pid) in udp {
            let owner = match tracker.app_of_pid(pid) {
                Some(app_id) => {
                    Owner { app_id: app_id.into(), name: tracker.app_name(app_id).unwrap_or(app_id).into() }
                }
                None => k
                    .pid_owners
                    .entry(pid)
                    .or_insert_with(|| {
                        let (id, name) = describe_process(platform.process_info(pid));
                        Owner { app_id: id.into(), name: name.into() }
                    })
                    .clone(),
            };
            k.ports.insert((Transport::Udp, local.port()), (owner, now));
        }
        k.ports.retain(|_, (_, seen)| now.saturating_sub(*seen) <= PORT_MEMORY_MS);
    }

    /// Whether Npcap is installed and which network cards it can use.
    pub fn support(&self) -> CaptureSupport {
        let npcap = match self.load_npcap() {
            Ok(npcap) => npcap,
            Err(problem) => {
                return CaptureSupport {
                    available: false,
                    npcap_version: None,
                    interfaces: vec![],
                    problem: Some(problem.to_string()),
                };
            }
        };
        match interfaces(&npcap) {
            Ok(list) => CaptureSupport {
                available: true,
                npcap_version: Some(npcap.version()),
                interfaces: list,
                problem: None,
            },
            Err(problem) => CaptureSupport {
                available: false,
                npcap_version: Some(npcap.version()),
                interfaces: vec![],
                problem: Some(problem.to_string()),
            },
        }
    }

    fn load_npcap(&self) -> Result<Npcap, Problem> {
        let mut slot = self.npcap.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(npcap) = slot.as_ref() {
            return Ok(npcap.clone());
        }
        let npcap = Npcap::load()?;
        *slot = Some(npcap.clone());
        Ok(npcap)
    }

    pub fn status(&self) -> CaptureStatus {
        self.session().status()
    }

    /// Starts a new capture, replacing any earlier one.
    pub fn start(self: &Arc<Self>, app: &AppHandle, options: CaptureStartOptions) -> Result<CaptureStatus, String> {
        let npcap = self.load_npcap().map_err(|p| p.to_string())?;
        let available = interfaces(&npcap).map_err(|p| p.to_string())?;
        let chosen: Vec<CaptureInterface> = if options.interfaces.is_empty() {
            available.into_iter().filter(|i| i.active).collect()
        } else {
            available.into_iter().filter(|i| options.interfaces.contains(&i.id)).collect()
        };
        if chosen.is_empty() {
            return Err("No connected network card to capture on.".into());
        }

        let filter = capture_filter(&options.target, &options.capture_filter);
        let mut handles = Vec::new();
        let mut problems = Vec::new();
        for interface in &chosen {
            match npcap.open(&interface.id, &filter) {
                Ok(handle) => handles.push((interface.clone(), handle)),
                Err(error) => problems.push(format!("{}: {error}", interface.name)),
            }
        }
        if handles.is_empty() {
            return Err(problems.join(" "));
        }

        let generation = self.generations.fetch_add(1, Ordering::SeqCst) + 1;
        let stop = Arc::new(AtomicBool::new(false));
        {
            let mut session = self.session();
            session.stop();
            let filter_text = std::mem::take(&mut session.filter_text);
            let display = std::mem::replace(&mut session.filter, Filter::parse("").expect("empty filter"));
            *session = Session::empty(generation);
            session.state = CaptureState::Running;
            session.target = Some(options.target.clone());
            session.started_at = Some(now_ms());
            session.started = Some(Instant::now());
            session.stop_after = options.stop_after;
            session.stop = Arc::clone(&stop);
            session.filter_text = filter_text;
            session.filter = display;
            session.error = (!problems.is_empty()).then(|| format!("Some network cards could not be used. {}", problems.join(" ")));
            if let CaptureTarget::Domain { name } = &options.target {
                let quoted = name.replace('"', "");
                session.dns_filter = Filter::parse(&format!("dns.qry.name contains \"{quoted}\"")).ok();
            }
            session.interfaces = handles
                .iter()
                .map(|(i, h)| Interface { name: i.id.clone(), friendly: i.name.clone(), link: LinkType::from_dlt(h.datalink()) })
                .collect();
        }
        {
            // Fresh names and ports before the first packet arrives.
            let mut k = self.knowledge.write().unwrap_or_else(|p| p.into_inner());
            k.names_version = u64::MAX;
            if let CaptureTarget::Domain { name } = &options.target {
                k.domain_ips = name_matches(&k.names, &name.to_lowercase());
            }
        }
        self.running.store(true, Ordering::SeqCst);

        for (index, (_, handle)) in handles.into_iter().enumerate() {
            let (capture, stop) = (Arc::clone(self), Arc::clone(&stop));
            let _ = thread::Builder::new()
                .name(format!("portlight-capture-{index}"))
                .spawn(move || capture.record(generation, index as u16, handle, &stop));
        }
        let (capture, app, stop_status) = (Arc::clone(self), app.clone(), Arc::clone(&stop));
        let _ = thread::Builder::new().name("portlight-capture-status".into()).spawn(move || {
            while !stop_status.load(Ordering::SeqCst) {
                thread::sleep(STATUS_EVERY);
                capture.retry_waiting(generation);
                capture.check_time_limit(generation);
                let _ = app.emit(STATUS_EVENT, capture.status());
            }
            let _ = app.emit(STATUS_EVENT, capture.status());
        });

        Ok(self.status())
    }

    /// One network card's capture loop.
    fn record(&self, generation: u64, iface: u16, mut handle: portlight_platform::npcap::Handle, stop: &AtomicBool) {
        let mut batch: Vec<Stored> = Vec::with_capacity(256);
        let mut flushed = Instant::now();
        let mut stats_at = Instant::now();
        while !stop.load(Ordering::SeqCst) {
            match handle.next_packet() {
                Ok(Some(packet)) => batch.push(Stored {
                    timestamp_us: packet.timestamp_us,
                    iface,
                    original_len: packet.original_len,
                    data: packet.data.into(),
                    app: None,
                }),
                Ok(None) => {}
                Err(error) => {
                    let mut session = self.session();
                    if session.generation == generation {
                        session.error = Some(format!("Capture stopped on one network card: {error}"));
                    }
                    break;
                }
            }
            if batch.len() >= 256 || flushed.elapsed() >= Duration::from_millis(50) {
                self.ingest(generation, std::mem::take(&mut batch));
                flushed = Instant::now();
            }
            if stats_at.elapsed() >= Duration::from_secs(1) {
                stats_at = Instant::now();
                let (_, dropped) = handle.stats();
                let mut session = self.session();
                if session.generation == generation {
                    session.driver_dropped.insert(iface, dropped as u64);
                }
            }
        }
        self.ingest(generation, batch);
    }

    /// Sorts new packets into kept, waiting for their app, or not wanted.
    fn ingest(&self, generation: u64, batch: Vec<Stored>) {
        if batch.is_empty() {
            return;
        }
        let knowledge = self.knowledge();
        let mut session = self.session();
        if session.generation != generation || session.state != CaptureState::Running {
            return;
        }
        let Some(target) = session.target.clone() else { return };
        for mut packet in batch {
            let link = session.interfaces.get(packet.iface as usize).map_or(LinkType::Ethernet, |i| i.link);
            let ep = endpoints(link, &packet.data);
            let owner = ep.as_ref().and_then(|ep| knowledge.owner(ep));
            packet.app = owner.as_ref().map(|o| Arc::clone(&o.name));
            match wanted(&target, &session, &knowledge, link, &packet, ep.as_ref(), owner.as_ref()) {
                Want::Keep => keep(&mut session, &knowledge, packet),
                Want::Wait => {
                    if let Some(endpoints) = ep {
                        session.waiting.push(Waiting { packet, endpoints, since: Instant::now() });
                    }
                }
                Want::Drop => {}
            }
            if session.state != CaptureState::Running {
                break;
            }
        }
        if session.state != CaptureState::Running {
            self.running.store(false, Ordering::SeqCst);
        }
    }

    /// Tries again to match waiting packets to their app.
    fn retry_waiting(&self, generation: u64) {
        let knowledge = self.knowledge();
        let mut session = self.session();
        if session.generation != generation || session.waiting.is_empty() {
            return;
        }
        let Some(CaptureTarget::App { app_id, .. }) = session.target.clone() else { return };
        let waiting = std::mem::take(&mut session.waiting);
        let mut kept = Vec::new();
        for mut entry in waiting {
            match knowledge.owner(&entry.endpoints) {
                Some(owner) if *owner.app_id == *app_id => {
                    entry.packet.app = Some(owner.name);
                    kept.push(entry.packet);
                }
                Some(_) => {}
                None if entry.since.elapsed() < WAIT_FOR_APP => session.waiting.push(entry),
                None => {}
            }
        }
        // Waiting packets go in at their place in time.
        for packet in kept {
            keep(&mut session, &knowledge, packet);
        }
    }

    pub fn stop(&self) -> CaptureStatus {
        let mut session = self.session();
        session.stop();
        self.running.store(false, Ordering::SeqCst);
        session.status()
    }

    pub fn clear(&self) -> CaptureStatus {
        let generation = self.generations.fetch_add(1, Ordering::SeqCst) + 1;
        let mut session = self.session();
        session.stop();
        let (text, filter) = (std::mem::take(&mut session.filter_text), Filter::parse("").expect("empty filter"));
        let kept_filter = std::mem::replace(&mut session.filter, filter);
        *session = Session::empty(generation);
        session.filter_text = text;
        session.filter = kept_filter;
        self.running.store(false, Ordering::SeqCst);
        session.status()
    }

    /// Applies a display filter. A filter that doesn't parse is explained and
    /// the previous one stays.
    pub fn set_filter(&self, text: &str) -> CaptureStatus {
        let knowledge = self.knowledge();
        let mut session = self.session();
        match Filter::parse(text) {
            Ok(filter) => {
                session.filter = filter;
                session.filter_text = text.trim().to_string();
                session.filter_error = None;
                let shown: Vec<u32> =
                    (0..session.packets.len()).filter(|&i| session.matches(i, &knowledge)).map(|i| i as u32).collect();
                session.shown = shown;
            }
            Err(error) => session.filter_error = Some(error.to_string()),
        }
        session.status()
    }

    /// Rows matching a filter of the caller's own, without touching the
    /// display filter the person has set. Returns the rows and how many
    /// packets match in all. For AI assistants.
    pub fn rows_matching(&self, filter: &str, start: usize, count: usize) -> Result<(Vec<PacketRow>, usize), String> {
        let filter = Filter::parse(filter).map_err(|e| e.to_string())?;
        let knowledge = self.knowledge();
        let session = self.session();
        let mut rows = Vec::new();
        let mut matched = 0usize;
        for (index, packet) in session.packets.iter().enumerate() {
            let link = session.link_of(packet);
            let hit = filter.is_empty()
                || filter.matches(&PacketRef {
                    link,
                    frame: &packet.data,
                    original_len: packet.original_len,
                    number: index as u32 + 1,
                    app: packet.app.as_deref(),
                    ctx: &*knowledge,
                });
            if !hit {
                continue;
            }
            if matched >= start && rows.len() < count {
                rows.push(PacketRow {
                    number: index as u32 + 1,
                    time: session.time_of(packet),
                    summary: summarize(link, &packet.data, packet.original_len, &*knowledge),
                    app: packet.app.as_deref().map(str::to_string),
                });
            }
            matched += 1;
        }
        Ok((rows, matched))
    }

    /// What the current capture is of, in words.
    pub fn target_words(&self) -> String {
        target_words(self.session().target.as_ref())
    }

    pub fn rows(&self, start: u32, count: u32) -> Vec<PacketRow> {
        let knowledge = self.knowledge();
        let session = self.session();
        let start = (start as usize).min(session.shown.len());
        let end = start.saturating_add(count.min(2_000) as usize).min(session.shown.len());
        session.shown[start..end]
            .iter()
            .map(|&index| {
                let packet = &session.packets[index as usize];
                let link = session.interfaces.get(packet.iface as usize).map_or(LinkType::Ethernet, |i| i.link);
                PacketRow {
                    number: index + 1,
                    time: session.time_of(packet),
                    summary: summarize(link, &packet.data, packet.original_len, &*knowledge),
                    app: packet.app.as_deref().map(str::to_string),
                }
            })
            .collect()
    }

    pub fn packet(&self, number: u32) -> Option<PacketDetail> {
        let knowledge = self.knowledge();
        let session = self.session();
        detail(&session, &knowledge, (number as usize).checked_sub(1)?)
    }

    /// Stops a capture whose time is up.
    fn check_time_limit(&self, generation: u64) {
        let mut session = self.session();
        if session.generation == generation {
            session.check_stop_after();
            if session.state != CaptureState::Running {
                self.running.store(false, Ordering::SeqCst);
            }
        }
    }

    /// A file name that says what was captured.
    pub fn suggested_name(&self, extension: &str) -> String {
        let label = match &self.session().target {
            Some(CaptureTarget::App { name, .. }) => name.clone(),
            Some(CaptureTarget::Host { ip, name }) => name.clone().unwrap_or_else(|| ip.clone()),
            Some(CaptureTarget::Domain { name }) => name.clone(),
            Some(CaptureTarget::Connection { app_name, remote_port, .. }) => format!("{app_name}-{remote_port}"),
            _ => "everything".into(),
        };
        let safe: String = label
            .chars()
            .map(|c| if c.is_alphanumeric() || c == '.' || c == '-' { c.to_ascii_lowercase() } else { '-' })
            .collect();
        let date = crate::state::today();
        format!("portlight-{}-{date}.{extension}", safe.trim_matches('-'))
    }

    /// Writes the chosen packets in one of the export formats.
    /// `utc_offset_minutes` is the person's time zone, for readable times.
    pub fn export(&self, path: &PathBuf, format: ExportFormat, selection: &Selection, utc_offset_minutes: i32) -> std::io::Result<()> {
        let knowledge = self.knowledge();
        let session = self.session();
        let chosen = chosen(&session, selection);
        let mut out = BufWriter::new(File::create(path)?);
        match format {
            ExportFormat::Pcapng => {
                let mut writer = pcapng::Writer::new(out, &format!("Portlight {}", env!("CARGO_PKG_VERSION")))?;
                let mut ids = Vec::new();
                for interface in &session.interfaces {
                    ids.push(writer.add_interface(interface.link, 65_535, &interface.name, &interface.friendly)?);
                }
                for index in chosen {
                    let packet = &session.packets[index];
                    let id = ids.get(packet.iface as usize).copied().unwrap_or(0);
                    writer.write_packet(id, packet.timestamp_us, packet.original_len, &packet.data, packet.app.as_deref())?;
                }
                writer.finish()?.flush()
            }
            ExportFormat::Csv => {
                writeln!(out, "No.,Time,Source,Source name,Destination,Destination name,Protocol,Length,App,Info,Summary")?;
                for index in chosen {
                    let packet = &session.packets[index];
                    let s = summarize(session.link_of(packet), &packet.data, packet.original_len, &*knowledge);
                    let with_port = |ip: &str, port: Option<u16>| match port {
                        Some(port) => format!("{ip}:{port}"),
                        None => ip.to_string(),
                    };
                    let fields = [
                        (index + 1).to_string(),
                        format!("{:.6}", session.time_of(packet)),
                        with_port(&s.source, s.source_port),
                        s.source_name.clone().unwrap_or_default(),
                        with_port(&s.destination, s.destination_port),
                        s.destination_name.clone().unwrap_or_default(),
                        s.protocol.clone(),
                        s.length.to_string(),
                        packet.app.as_deref().unwrap_or("").to_string(),
                        s.info.clone(),
                        s.summary.clone(),
                    ];
                    let line: Vec<String> = fields.iter().map(|f| csv_field(f)).collect();
                    writeln!(out, "{}", line.join(","))?;
                }
                out.flush()
            }
            ExportFormat::Text => {
                writeln!(out, "Portlight packet breakdown")?;
                writeln!(out, "Capture: {}", target_words(session.target.as_ref()))?;
                writeln!(out, "Packets: {}", chosen.len())?;
                for index in chosen {
                    let Some(d) = detail(&session, &knowledge, index) else { continue };
                    let packet = &session.packets[index];
                    let s = summarize(session.link_of(packet), &packet.data, packet.original_len, &*knowledge);
                    writeln!(out, "\n{}", "=".repeat(72))?;
                    write!(out, "Packet {} · {:.6} s · {} · {} bytes", d.number, d.time, crate::state::format_time(d.timestamp as u64, utc_offset_minutes), d.length)?;
                    match &d.app {
                        Some(app) => writeln!(out, " · {app}")?,
                        None => writeln!(out)?,
                    }
                    writeln!(out, "{}", s.summary)?;
                    writeln!(out, "{}", s.info)?;
                    for layer in &d.layers {
                        writeln!(out, "\n{} — {}", layer.title, layer.summary)?;
                        for field in &layer.fields {
                            write_field(&mut out, field, 1)?;
                        }
                    }
                }
                out.flush()
            }
            ExportFormat::Json => {
                let packets: Vec<serde_json::Value> = chosen
                    .into_iter()
                    .filter_map(|index| {
                        let d = detail(&session, &knowledge, index)?;
                        let packet = &session.packets[index];
                        let s = summarize(session.link_of(packet), &packet.data, packet.original_len, &*knowledge);
                        let mut value = serde_json::to_value(&d).ok()?;
                        let hex: String = d.bytes.iter().map(|b| format!("{b:02x}")).collect();
                        value["bytes"] = serde_json::Value::String(hex);
                        value["summary"] = serde_json::to_value(&s).ok()?;
                        Some(value)
                    })
                    .collect();
                let document = serde_json::json!({
                    "exportedBy": format!("Portlight {}", env!("CARGO_PKG_VERSION")),
                    "capture": session.target,
                    "packets": packets,
                });
                serde_json::to_writer_pretty(&mut out, &document).map_err(std::io::Error::other)?;
                out.flush()
            }
        }
    }
}

fn write_field(out: &mut impl Write, field: &portlight_packets::Field, depth: usize) -> std::io::Result<()> {
    let indent = "  ".repeat(depth);
    writeln!(out, "{indent}{}: {}", field.label, field.value)?;
    for child in &field.children {
        write_field(out, child, depth + 1)?;
    }
    Ok(())
}

/// The full breakdown of one stored packet.
fn detail(session: &Session, knowledge: &Knowledge, index: usize) -> Option<PacketDetail> {
    let packet = session.packets.get(index)?;
    let number = index as u32 + 1;
    let interface = session.interfaces.get(packet.iface as usize);
    let interface_name = interface.map(|i| i.friendly.clone()).unwrap_or_default();
    let time = session.time_of(packet);
    let timestamp = packet.timestamp_us as f64 / 1000.0;
    let info = FrameInfo {
        number,
        time,
        timestamp_ms: timestamp,
        original_len: packet.original_len,
        interface_name: &interface_name,
        app: packet.app.as_deref(),
    };
    Some(PacketDetail {
        number,
        time,
        timestamp,
        length: packet.original_len,
        captured_length: packet.data.len() as u32,
        interface_name: interface_name.clone(),
        app: packet.app.as_deref().map(str::to_string),
        layers: dissect(session.link_of(packet), &packet.data, &info, knowledge),
        bytes: packet.data.to_vec(),
    })
}

/// What a capture is of, in words, for exported files.
fn target_words(target: Option<&CaptureTarget>) -> String {
    match target {
        Some(CaptureTarget::App { name, .. }) => format!("packets from {name}"),
        Some(CaptureTarget::Host { ip, name }) => format!("packets to and from {}", name.clone().unwrap_or_else(|| ip.clone())),
        Some(CaptureTarget::Domain { name }) => format!("packets to and from {name}"),
        Some(CaptureTarget::Connection { app_name, remote_ip, remote_port, host, .. }) => {
            format!("one connection: {app_name} with {}:{remote_port}", host.clone().unwrap_or_else(|| remote_ip.clone()))
        }
        _ => "everything on this PC's network".into(),
    }
}

enum Want {
    Keep,
    Wait,
    Drop,
}

fn wanted(
    target: &CaptureTarget,
    session: &Session,
    knowledge: &Knowledge,
    link: LinkType,
    packet: &Stored,
    ep: Option<&Endpoints>,
    owner: Option<&Owner>,
) -> Want {
    let keep_if = |yes: bool| if yes { Want::Keep } else { Want::Drop };
    match target {
        CaptureTarget::All => Want::Keep,
        CaptureTarget::App { app_id, .. } => match (ep, owner) {
            (None, _) => Want::Drop,
            (Some(_), Some(owner)) => keep_if(*owner.app_id == **app_id),
            (Some(_), None) => Want::Wait,
        },
        CaptureTarget::Host { ip, .. } => {
            let Ok(ip) = ip.parse::<IpAddr>() else { return Want::Drop };
            keep_if(ep.is_some_and(|ep| ep.src.to_canonical() == ip.to_canonical() || ep.dst.to_canonical() == ip.to_canonical()))
        }
        CaptureTarget::Domain { .. } => {
            let by_address = ep.is_some_and(|ep| {
                knowledge.domain_ips.contains(&ep.src.to_canonical()) || knowledge.domain_ips.contains(&ep.dst.to_canonical())
            });
            let by_lookup = session.dns_filter.as_ref().is_some_and(|filter| {
                filter.matches(&PacketRef {
                    link,
                    frame: &packet.data,
                    original_len: packet.original_len,
                    number: 0,
                    app: None,
                    ctx: knowledge,
                })
            });
            keep_if(by_address || by_lookup)
        }
        CaptureTarget::Connection { protocol, local_port, remote_ip, remote_port, .. } => {
            let Ok(remote) = remote_ip.parse::<IpAddr>() else { return Want::Drop };
            let transport = match protocol {
                Protocol::Tcp => Transport::Tcp,
                Protocol::Udp => Transport::Udp,
            };
            keep_if(ep.is_some_and(|ep| {
                let (src, dst) = (ep.src.to_canonical(), ep.dst.to_canonical());
                let remote = remote.to_canonical();
                let ports = (ep.src_port, ep.dst_port);
                // UDP conversations have no fixed local port (it is 0 in the monitor).
                let local_ok = |port: Option<u16>| *local_port == 0 || port == Some(*local_port);
                ep.transport == transport
                    && ((dst == remote && ports.1 == Some(*remote_port) && local_ok(ports.0))
                        || (src == remote && ports.0 == Some(*remote_port) && local_ok(ports.1)))
            }))
        }
    }
}

/// Adds a packet to the capture, applying the display filter and the limits.
fn keep(session: &mut Session, knowledge: &Knowledge, packet: Stored) {
    // Packets still in flight when the capture stopped (by hand, a stop
    // condition or the limit) are not added.
    if session.state != CaptureState::Running {
        return;
    }
    if session.packets.len() >= MAX_PACKETS || session.bytes + packet.data.len() as u64 > MAX_BYTES {
        session.dropped_full += 1;
        session.limit_reached = true;
        session.stop();
        return;
    }
    session.first_us = Some(session.first_us.map_or(packet.timestamp_us, |first| first.min(packet.timestamp_us)));
    session.bytes += packet.data.len() as u64;
    session.packets.push(packet);
    let index = session.packets.len() - 1;
    if session.matches(index, knowledge) {
        session.shown.push(index as u32);
    }
    session.check_stop_after();
}

fn chosen(session: &Session, selection: &Selection) -> Vec<usize> {
    let count = session.packets.len();
    match selection {
        Selection::All => (0..count).collect(),
        Selection::Shown => session.shown.iter().map(|&i| i as usize).collect(),
        Selection::Packets { numbers } => {
            let mut picked: Vec<usize> =
                numbers.iter().filter_map(|n| (*n as usize).checked_sub(1)).filter(|&i| i < count).collect();
            picked.sort_unstable();
            picked.dedup();
            picked
        }
        Selection::Range { from, to } => {
            let (low, high) = (*from.min(to), *from.max(to));
            let first = (low as usize).saturating_sub(1);
            let last = (high as usize).min(count);
            (first..last).collect()
        }
    }
}

/// The capture filter (BPF) sent to the driver: the part the target implies,
/// plus the person's own, so the driver drops unwanted packets early.
fn capture_filter(target: &CaptureTarget, own: &str) -> String {
    let implied = match target {
        CaptureTarget::Host { ip, .. } => format!("host {ip}"),
        CaptureTarget::Connection { protocol, remote_ip, remote_port, local_port, .. } => {
            let proto = match protocol {
                Protocol::Tcp => "tcp",
                Protocol::Udp => "udp",
            };
            if *local_port == 0 {
                format!("{proto} and host {remote_ip} and port {remote_port}")
            } else {
                format!("{proto} and host {remote_ip} and port {remote_port} and port {local_port}")
            }
        }
        CaptureTarget::App { .. } => "tcp or udp".into(),
        CaptureTarget::Domain { .. } | CaptureTarget::All => String::new(),
    };
    match (implied.is_empty(), own.trim().is_empty()) {
        (true, _) => own.trim().to_string(),
        (false, true) => implied,
        (false, false) => format!("({implied}) and ({})", own.trim()),
    }
}

/// Addresses whose name is the domain or one of its subdomains.
fn name_matches(names: &HashMap<IpAddr, String>, domain: &str) -> HashSet<IpAddr> {
    let suffix = format!(".{domain}");
    names
        .iter()
        .filter(|(_, name)| {
            let name = name.to_lowercase();
            name == domain || name.ends_with(&suffix)
        })
        .map(|(ip, _)| *ip)
        .collect()
}

fn interfaces(npcap: &Npcap) -> Result<Vec<CaptureInterface>, Problem> {
    let adapters = portlight_platform::adapters::adapters();
    let devices = npcap.devices()?;
    Ok(devices
        .into_iter()
        .map(|device| {
            let adapter = adapters.iter().find(|a| !a.guid.is_empty() && device.name.ends_with(&a.guid));
            let name = if device.loopback {
                "This PC only (loopback)".to_string()
            } else {
                adapter.map(|a| a.friendly_name.clone()).filter(|n| !n.is_empty()).unwrap_or_else(|| device.description.clone())
            };
            let up = adapter.map_or(device.up, |a| a.up);
            CaptureInterface {
                active: up && device.connected && !device.loopback && !device.addresses.is_empty(),
                id: device.name,
                name,
                description: adapter.map(|a| a.description.clone()).unwrap_or(device.description),
                addresses: device.addresses.iter().map(IpAddr::to_string).collect(),
            }
        })
        .collect())
}

fn csv_field(text: &str) -> String {
    if text.contains([',', '"', '\n', '\r']) { format!("\"{}\"", text.replace('"', "\"\"")) } else { text.to_string() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn targets_read_the_shapes_the_window_sends() {
        let app: CaptureTarget = serde_json::from_str(r#"{"kind":"app","appId":"c:\\a.exe","name":"A"}"#).unwrap();
        assert_eq!(app, CaptureTarget::App { app_id: "c:\\a.exe".into(), name: "A".into() });
        let conn: CaptureTarget = serde_json::from_str(
            r#"{"kind":"connection","protocol":"tcp","localPort":50000,"remoteIp":"1.2.3.4","remotePort":443,"appName":"A","host":null}"#,
        )
        .unwrap();
        assert!(matches!(conn, CaptureTarget::Connection { local_port: 50_000, remote_port: 443, .. }));
        assert_eq!(serde_json::to_string(&CaptureTarget::All).unwrap(), r#"{"kind":"all"}"#);
    }

    #[test]
    fn the_capture_filter_combines_the_target_and_your_own() {
        let host = CaptureTarget::Host { ip: "1.2.3.4".into(), name: None };
        assert_eq!(capture_filter(&host, ""), "host 1.2.3.4");
        assert_eq!(capture_filter(&host, " port 443 "), "(host 1.2.3.4) and (port 443)");
        assert_eq!(capture_filter(&CaptureTarget::All, "udp"), "udp");
        let udp = CaptureTarget::Connection {
            protocol: Protocol::Udp,
            local_port: 0,
            remote_ip: "8.8.8.8".into(),
            remote_port: 53,
            app_name: "x".into(),
            host: None,
        };
        assert_eq!(capture_filter(&udp, ""), "udp and host 8.8.8.8 and port 53");
    }

    #[test]
    fn a_domain_covers_its_subdomains_only() {
        let names: HashMap<IpAddr, String> = [
            ("1.1.1.1".parse().unwrap(), "github.com".to_string()),
            ("2.2.2.2".parse().unwrap(), "api.github.com".to_string()),
            ("3.3.3.3".parse().unwrap(), "notgithub.com".to_string()),
        ]
        .into();
        let ips = name_matches(&names, "github.com");
        assert_eq!(ips.len(), 2);
        assert!(!ips.contains(&"3.3.3.3".parse().unwrap()));
    }

    const ME: [u8; 4] = [192, 168, 1, 10];
    const SERVER: [u8; 4] = [140, 82, 112, 3];

    /// An Ethernet + IPv4 + TCP frame with a little payload.
    fn tcp_frame(src: [u8; 4], dst: [u8; 4], sport: u16, dport: u16) -> Vec<u8> {
        let payload = b"hello";
        let mut f = vec![0x02, 0, 0, 0, 0, 1, 0x02, 0, 0, 0, 0, 2, 0x08, 0x00];
        let total = (20 + 20 + payload.len()) as u16;
        f.extend([0x45, 0, (total >> 8) as u8, total as u8, 0, 1, 0x40, 0, 64, 6, 0, 0]);
        f.extend(src);
        f.extend(dst);
        f.extend(sport.to_be_bytes());
        f.extend(dport.to_be_bytes());
        f.extend([0, 0, 0, 1, 0, 0, 0, 0, 0x50, 0x18, 0xff, 0xff, 0, 0, 0, 0]);
        f.extend(payload);
        f
    }

    fn knowledge() -> Knowledge {
        let mut k = Knowledge::default();
        k.local.insert(IpAddr::from(ME));
        k.names.insert(IpAddr::from(SERVER), "github.com".into());
        let chrome = Owner { app_id: "c:\\chrome.exe".into(), name: "Google Chrome".into() };
        k.ports.insert((Transport::Tcp, 50_000), (chrome, 0));
        k
    }

    fn session_with(frames: &[Vec<u8>]) -> Session {
        let mut session = Session::empty(1);
        session.interfaces.push(Interface { name: "dev".into(), friendly: "Wi-Fi".into(), link: LinkType::Ethernet });
        for (i, frame) in frames.iter().enumerate() {
            session.packets.push(Stored {
                timestamp_us: 1_000_000 + i as u64 * 1000,
                iface: 0,
                original_len: frame.len() as u32,
                data: frame.clone().into(),
                app: Some("Google Chrome".into()),
            });
            session.shown.push(i as u32);
        }
        session.first_us = Some(1_000_000);
        session
    }

    #[test]
    fn packets_are_matched_to_their_app_by_local_port_either_way() {
        let k = knowledge();
        let out = endpoints(LinkType::Ethernet, &tcp_frame(ME, SERVER, 50_000, 443)).unwrap();
        let back = endpoints(LinkType::Ethernet, &tcp_frame(SERVER, ME, 443, 50_000)).unwrap();
        let other = endpoints(LinkType::Ethernet, &tcp_frame(ME, SERVER, 50_001, 443)).unwrap();
        assert_eq!(k.owner(&out).map(|o| o.name.to_string()).as_deref(), Some("Google Chrome"));
        assert_eq!(k.owner(&back).map(|o| o.name.to_string()).as_deref(), Some("Google Chrome"));
        // The server's port 443 is not mistaken for a local port.
        assert!(k.owner(&other).is_none());
    }

    #[test]
    fn targets_keep_only_what_was_asked_for() {
        let k = knowledge();
        let session = session_with(&[]);
        let frame = tcp_frame(ME, SERVER, 50_000, 443);
        let packet = Stored { timestamp_us: 0, iface: 0, original_len: 0, data: frame.clone().into(), app: None };
        let ep = endpoints(LinkType::Ethernet, &frame);
        let check = |target: CaptureTarget| {
            let owner = ep.as_ref().and_then(|ep| k.owner(ep));
            matches!(wanted(&target, &session, &k, LinkType::Ethernet, &packet, ep.as_ref(), owner.as_ref()), Want::Keep)
        };

        assert!(check(CaptureTarget::App { app_id: "c:\\chrome.exe".into(), name: "Chrome".into() }));
        assert!(!check(CaptureTarget::App { app_id: "c:\\other.exe".into(), name: "Other".into() }));
        assert!(check(CaptureTarget::Host { ip: "140.82.112.3".into(), name: None }));
        assert!(!check(CaptureTarget::Host { ip: "1.1.1.1".into(), name: None }));
        let conn = |local_port| CaptureTarget::Connection {
            protocol: Protocol::Tcp,
            local_port,
            remote_ip: "140.82.112.3".into(),
            remote_port: 443,
            app_name: "Chrome".into(),
            host: None,
        };
        assert!(check(conn(50_000)));
        assert!(!check(conn(50_001)));

        // An app capture waits for a packet whose port is not known yet.
        let unknown = tcp_frame(ME, SERVER, 50_009, 443);
        let ep = endpoints(LinkType::Ethernet, &unknown);
        let target = CaptureTarget::App { app_id: "c:\\chrome.exe".into(), name: "Chrome".into() };
        assert!(matches!(wanted(&target, &session, &k, LinkType::Ethernet, &packet, ep.as_ref(), None), Want::Wait));
    }

    #[test]
    fn exports_cover_the_chosen_packets() {
        let frames: Vec<Vec<u8>> = (0..5).map(|i| tcp_frame(ME, SERVER, 50_000, 443 + i)).collect();
        let mut session = session_with(&frames);
        session.shown = vec![1, 3];
        let picked = |selection: Selection| chosen(&session, &selection);
        assert_eq!(picked(Selection::All), vec![0, 1, 2, 3, 4]);
        assert_eq!(picked(Selection::Shown), vec![1, 3]);
        assert_eq!(picked(Selection::Packets { numbers: vec![5, 1, 1, 99] }), vec![0, 4]);
        assert_eq!(picked(Selection::Range { from: 4, to: 2 }), vec![1, 2, 3]);
        assert_eq!(picked(Selection::Range { from: 3, to: 50 }), vec![2, 3, 4]);
    }

    #[test]
    fn a_capture_stops_itself_when_asked_to() {
        let k = knowledge();
        let mut session = session_with(&[]);
        session.state = CaptureState::Running;
        session.stop = Arc::new(AtomicBool::new(false));
        session.stop_after = StopAfter { packets: Some(2), ..StopAfter::default() };
        for _ in 0..3 {
            let frame = tcp_frame(ME, SERVER, 50_000, 443);
            keep(&mut session, &k, Stored { timestamp_us: 5, iface: 0, original_len: 0, data: frame.into(), app: None });
        }
        assert_eq!(session.state, CaptureState::Stopped);
        assert_eq!(session.packets.len(), 2);
        assert_eq!(session.stop_reason.as_deref(), Some("Stopped after 2 packets, as you asked."));
        assert!(session.stop.load(Ordering::SeqCst));
    }

    #[test]
    fn breakdowns_export_as_text_and_json() {
        let capture = Capture::default();
        *capture.knowledge.write().unwrap() = knowledge();
        *capture.session.lock().unwrap() = session_with(&[tcp_frame(ME, SERVER, 50_000, 443)]);
        let dir = std::env::temp_dir().join(format!("portlight-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();

        let text_path = dir.join("out.txt");
        capture.export(&text_path, ExportFormat::Text, &Selection::All, 0).unwrap();
        let text = std::fs::read_to_string(&text_path).unwrap();
        assert!(text.contains("Packet 1"), "{text}");
        assert!(text.contains("Google Chrome"));
        assert!(text.contains("Transmission Control Protocol") || text.contains("TCP"));

        let json_path = dir.join("out.json");
        capture.export(&json_path, ExportFormat::Json, &Selection::All, 0).unwrap();
        let json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&json_path).unwrap()).unwrap();
        let packet = &json["packets"][0];
        assert_eq!(packet["number"], 1);
        assert!(packet["bytes"].as_str().unwrap().starts_with("020000000001"));
        assert!(packet["layers"].as_array().unwrap().len() >= 3);

        let pcap_path = dir.join("out.pcapng");
        capture.export(&pcap_path, ExportFormat::Pcapng, &Selection::All, 0).unwrap();
        let pcap = std::fs::read(&pcap_path).unwrap();
        assert_eq!(&pcap[..4], &[0x0a, 0x0d, 0x0d, 0x0a]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn csv_fields_are_quoted_when_needed() {
        assert_eq!(csv_field("plain"), "plain");
        assert_eq!(csv_field("a, \"b\""), "\"a, \"\"b\"\"\"");
    }
}
