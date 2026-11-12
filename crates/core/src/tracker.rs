//! Turns readings from a data source into apps, live connections and events.
//!
//! Call [`Tracker::update`] with each [`Observation`], or [`Tracker::poll`] to
//! read the TCP table directly. Each call compares what is open now with what
//! was open last time: anything new is an "opened" event, anything gone is a
//! "closed" event. Connections that opened and closed between two readings
//! (the event source reports these) get both events at once.

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::io;
use std::net::IpAddr;
use std::path::{Path, PathBuf};

use crate::known_apps::{KnownApp, KnownApps};
use crate::model::{
    Activity, AppView, ConnView, Connection, EventKind, FlowEvent, NewApp, Observation, Observed, OwnerInfo, Platform,
    ProcessInfo, Signature, Snapshot, Source,
};

/// All processes the OS will not name are grouped under this one app.
pub const SYSTEM_APP_ID: &str = "system";
const SYSTEM_APP_NAME: &str = "Windows system services";

/// How many past events are kept, for front-ends that start late and for
/// exporting the connection flow.
const MAX_EVENTS: usize = 2_000;

struct AppRecord {
    name: String,
    exe: String,
    path: Option<String>,
    first_seen: u64,
    trusted: bool,
    total_opened: u64,
    bytes_sent: u64,
    bytes_received: u64,
    signature: Signature,
}

struct ConnRecord {
    id: u64,
    app_id: String,
    opened_at: u64,
    sent: u64,
    received: u64,
}

/// What one [`Tracker::update`] call found.
#[derive(Debug, Default)]
pub struct Update {
    /// Apps that went online for the first time ever. Empty while learning.
    pub new_apps: Vec<NewApp>,
    /// Remote addresses seen for the first time with no site name yet, for
    /// reverse lookups.
    pub new_ips: Vec<IpAddr>,
    /// Every remote address seen for the first time, for owner lookups.
    pub new_addresses: Vec<IpAddr>,
    /// Programs whose signature should be checked in the background.
    pub check_signatures: Vec<(String, PathBuf)>,
    /// What each app did with each server, for the history.
    pub activity: Vec<Activity>,
    /// The known-apps list changed and should be saved.
    pub known_changed: bool,
}

pub struct Tracker {
    known: KnownApps,
    apps: BTreeMap<String, AppRecord>,
    open: HashMap<Connection, ConnRecord>,
    /// Which app each process ID belongs to, while it has open connections.
    pid_apps: HashMap<u32, String>,
    /// Site names apps looked up. These win over reverse lookups.
    asked_names: HashMap<IpAddr, String>,
    /// Names from reverse lookups.
    reverse_names: HashMap<IpAddr, String>,
    /// Goes up whenever a name is learned, so readers can tell when to copy.
    names_version: u64,
    owners: HashMap<IpAddr, OwnerInfo>,
    seen_ips: HashSet<IpAddr>,
    events: VecDeque<FlowEvent>,
    next_event: u64,
    next_conn: u64,
    source: Source,
    /// False until the first update, whose connections are the starting
    /// picture and not news.
    primed: bool,
}

impl Tracker {
    pub fn new(known: KnownApps) -> Self {
        Self {
            known,
            apps: BTreeMap::new(),
            open: HashMap::new(),
            pid_apps: HashMap::new(),
            asked_names: HashMap::new(),
            reverse_names: HashMap::new(),
            names_version: 0,
            owners: HashMap::new(),
            seen_ips: HashSet::new(),
            events: VecDeque::new(),
            next_event: 1,
            next_conn: 1,
            source: Source::Basic,
            primed: false,
        }
    }

    /// Reads the TCP table and records what changed since last time.
    pub fn poll(&mut self, platform: &dyn Platform, now: u64) -> io::Result<Update> {
        let open = platform.tcp_connections()?.into_iter().map(Observed::bare).collect();
        let observation = Observation { source: Source::Basic, open, ..Observation::default() };
        Ok(self.update(observation, platform, now))
    }

    /// Records one reading from a data source. `platform` names programs the
    /// reading does not.
    pub fn update(&mut self, observation: Observation, platform: &dyn Platform, now: u64) -> Update {
        let Observation { source, mut open, mut closed, names, processes } = observation;
        self.source = source;
        let processes: HashMap<u32, ProcessInfo> = processes.iter().map(|p| (p.pid, p.info())).collect();
        let lookup = |pid: u32| processes.get(&pid).cloned().or_else(|| platform.process_info(pid));
        let sign = |path: &Path| platform.signature(path);
        let programs = Programs { lookup: &lookup, sign: &sign };

        for (ip, name) in names {
            if self.asked_names.get(&ip) != Some(&name) {
                self.asked_names.insert(ip, name);
                self.names_version += 1;
            }
        }

        open.retain(|o| leaves_this_machine(&o.conn));
        closed.retain(|o| leaves_this_machine(&o.conn));
        // A fixed order keeps events and tests repeatable.
        let order = |o: &Observed| (o.conn.pid, o.conn.protocol, o.conn.remote, o.conn.local);
        open.sort_by_key(order);
        closed.sort_by_key(order);

        let mut update = Update::default();

        // Ended connections, with their final byte counts.
        for ended in &closed {
            if let Some(record) = self.open.remove(&ended.conn) {
                self.add_traffic(&record.app_id, record.sent, record.received, ended, false, now, &mut update);
                self.push_event(EventKind::Closed, &record.app_id, &ended.conn, ended.sent, ended.received, now);
            } else {
                // Opened and closed between two readings.
                let app_id = self.app_for(ended.conn.pid, &programs, now, &mut update);
                self.note_ip(ended.conn.remote.ip(), &mut update);
                self.add_traffic(&app_id, 0, 0, ended, true, now, &mut update);
                if let Some(app) = self.apps.get_mut(&app_id) {
                    app.total_opened += 1;
                }
                if self.primed {
                    self.push_event(EventKind::Opened, &app_id, &ended.conn, 0, 0, now);
                    self.push_event(EventKind::Closed, &app_id, &ended.conn, ended.sent, ended.received, now);
                }
            }
        }

        // Connections that are simply no longer listed.
        let listed: HashSet<&Connection> = open.iter().map(|o| &o.conn).collect();
        let mut gone: Vec<Connection> = self.open.keys().filter(|c| !listed.contains(c)).cloned().collect();
        gone.sort_by_key(|c| (c.pid, c.protocol, c.remote, c.local));
        for conn in gone {
            if let Some(record) = self.open.remove(&conn) {
                self.push_event(EventKind::Closed, &record.app_id, &conn, record.sent, record.received, now);
            }
        }

        for seen in &open {
            if let Some(record) = self.open.get(&seen.conn) {
                let (app_id, before_sent, before_received) = (record.app_id.clone(), record.sent, record.received);
                self.add_traffic(&app_id, before_sent, before_received, seen, false, now, &mut update);
                if let Some(record) = self.open.get_mut(&seen.conn) {
                    record.sent = record.sent.max(seen.sent);
                    record.received = record.received.max(seen.received);
                }
                continue;
            }
            let app_id = self.app_for(seen.conn.pid, &programs, now, &mut update);
            if let Some(app) = self.apps.get_mut(&app_id) {
                app.total_opened += 1;
            }
            self.note_ip(seen.conn.remote.ip(), &mut update);
            self.add_traffic(&app_id, 0, 0, seen, true, now, &mut update);
            if self.primed {
                self.push_event(EventKind::Opened, &app_id, &seen.conn, 0, 0, now);
            }
            let record =
                ConnRecord { id: self.next_conn, app_id, opened_at: now, sent: seen.sent, received: seen.received };
            self.next_conn += 1;
            self.open.insert(seen.conn.clone(), record);
        }

        // A process ID can be reused by a different program later, so only
        // remember it while it still has connections.
        let live_pids: HashSet<u32> = self.open.keys().map(|c| c.pid).collect();
        self.pid_apps.retain(|pid, _| live_pids.contains(pid));

        self.primed = true;
        update
    }

    /// The current picture, with every event newer than `events_after`.
    pub fn snapshot(&self, now: u64, events_after: u64) -> Snapshot {
        let mut live: HashMap<&str, (u32, HashSet<IpAddr>)> = HashMap::new();
        for (conn, record) in &self.open {
            let entry = live.entry(record.app_id.as_str()).or_default();
            entry.0 += 1;
            entry.1.insert(conn.remote.ip());
        }

        let mut apps: Vec<AppView> = self
            .apps
            .iter()
            .map(|(id, app)| {
                let (connections, servers) = live
                    .get(id.as_str())
                    .map(|(count, ips)| (*count, ips.len() as u32))
                    .unwrap_or((0, 0));
                AppView {
                    id: id.clone(),
                    name: app.name.clone(),
                    exe: app.exe.clone(),
                    path: app.path.clone(),
                    first_seen: app.first_seen,
                    trusted: app.trusted,
                    connections,
                    servers,
                    total_opened: app.total_opened,
                    bytes_sent: app.bytes_sent,
                    bytes_received: app.bytes_received,
                    signature: app.signature.clone(),
                }
            })
            .collect();
        apps.sort_by(|a, b| {
            b.connections.cmp(&a.connections).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });

        let mut connections: Vec<ConnView> = self
            .open
            .iter()
            .map(|(conn, record)| {
                let owner = self.owners.get(&conn.remote.ip());
                ConnView {
                id: record.id,
                app_id: record.app_id.clone(),
                pid: conn.pid,
                protocol: conn.protocol,
                remote_ip: conn.remote.ip().to_string(),
                remote_port: conn.remote.port(),
                local_port: conn.local.port(),
                host: self.name_of(conn.remote.ip()).map(str::to_string),
                opened_at: record.opened_at,
                bytes_sent: record.sent,
                bytes_received: record.received,
                owner: owner.map(|o| o.name.clone()),
                asn: owner.and_then(|o| o.asn),
                country: owner.and_then(|o| o.country.clone()),
            }})
            .collect();
        connections.sort_by_key(|c| c.id);

        Snapshot {
            now,
            source: self.source,
            learning_seconds_left: self.known.learning_seconds_left(now),
            apps,
            connections,
            events: self.events.iter().filter(|e| e.seq > events_after).cloned().collect(),
        }
    }

    /// The sequence number of the newest event, or 0 if there are none yet.
    pub fn last_event_seq(&self) -> u64 {
        self.next_event - 1
    }

    /// Records the name a reverse lookup found for a remote address. A site
    /// name an app looked up is kept in preference.
    pub fn set_host(&mut self, ip: IpAddr, host: String) {
        if self.reverse_names.get(&ip) != Some(&host) {
            self.reverse_names.insert(ip, host);
            self.names_version += 1;
        }
    }

    /// Records a site name an app looked up, e.g. from the collector's
    /// list when the window connects to it.
    pub fn set_asked_name(&mut self, ip: IpAddr, name: String) {
        if self.asked_names.get(&ip) != Some(&name) {
            self.asked_names.insert(ip, name);
            self.names_version += 1;
        }
    }

    /// Records who owns an address's network.
    pub fn set_owner(&mut self, ip: IpAddr, owner: OwnerInfo) {
        self.owners.insert(ip, owner);
    }

    /// Who owns an address's network, if looked up.
    pub fn owner_of(&self, ip: IpAddr) -> Option<&OwnerInfo> {
        self.owners.get(&ip)
    }

    /// Records the result of a background signature check. Returns true if
    /// the known-apps list changed and should be saved.
    pub fn set_signature(&mut self, app_id: &str, signature: Signature) -> bool {
        let publisher = match &signature {
            Signature::Signed { publisher } => Some(publisher.clone()),
            _ => None,
        };
        if let Some(app) = self.apps.get_mut(app_id) {
            app.signature = signature;
        }
        self.known.set_publisher(app_id, publisher)
    }

    /// The program path behind an app, if it has one.
    pub fn app_path(&self, app_id: &str) -> Option<&str> {
        self.apps.get(app_id).and_then(|a| a.path.as_deref())
    }

    /// The best name known for an address.
    pub fn name_of(&self, ip: IpAddr) -> Option<&str> {
        self.asked_names.get(&ip).or_else(|| self.reverse_names.get(&ip)).map(String::as_str)
    }

    /// Every address with a name, using the best name for each.
    pub fn names(&self) -> HashMap<IpAddr, String> {
        let mut all = self.reverse_names.clone();
        all.extend(self.asked_names.iter().map(|(ip, name)| (*ip, name.clone())));
        all
    }

    /// Goes up whenever a name is learned.
    pub fn names_version(&self) -> u64 {
        self.names_version
    }

    /// Every open connection and the app it belongs to.
    pub fn open_connections(&self) -> impl Iterator<Item = (&Connection, &str)> + '_ {
        self.open.iter().map(|(conn, record)| (conn, record.app_id.as_str()))
    }

    /// The app a process ID belongs to, while it has connections.
    pub fn app_of_pid(&self, pid: u32) -> Option<&str> {
        self.pid_apps.get(&pid).map(String::as_str)
    }

    /// An app's display name.
    pub fn app_name(&self, app_id: &str) -> Option<&str> {
        self.apps.get(app_id).map(|a| a.name.as_str())
    }

    /// Marks a new app as acknowledged. Returns true if that changed anything.
    pub fn trust(&mut self, app_id: &str) -> bool {
        if let Some(app) = self.apps.get_mut(app_id) {
            app.trusted = true;
        }
        self.known.trust(app_id)
    }

    /// Acknowledges every app. Returns true if that changed anything.
    pub fn trust_all(&mut self) -> bool {
        for app in self.apps.values_mut() {
            app.trusted = true;
        }
        self.known.trust_all()
    }

    /// Trusts every app seen so far and starts a new learning period, for
    /// when many apps alerted at once and the person knows them all.
    pub fn learn_again(&mut self, now: u64) {
        self.trust_all();
        self.known.learn_again(now);
    }

    /// Forgets every app Portlight has seen and starts learning again. Apps
    /// online right now are kept, as trusted.
    pub fn forget_known_apps(&mut self, now: u64) {
        self.known.forget_all(now);
        for (id, app) in self.apps.iter_mut() {
            app.trusted = true;
            let publisher = match &app.signature {
                Signature::Signed { publisher } => Some(publisher.clone()),
                _ => None,
            };
            self.known.insert(id.clone(), KnownApp { name: app.name.clone(), first_seen: now, trusted: true, publisher });
        }
    }

    pub fn known(&self) -> &KnownApps {
        &self.known
    }

    /// Finds, or creates, the app a process belongs to.
    fn app_for(&mut self, pid: u32, programs: &Programs, now: u64, update: &mut Update) -> String {
        if let Some(id) = self.pid_apps.get(&pid) {
            return id.clone();
        }

        let (id, record) = describe((programs.lookup)(pid));
        self.pid_apps.insert(pid, id.clone());
        if self.apps.contains_key(&id) {
            return id;
        }

        let record = match self.known.get(&id) {
            Some(known) => {
                if let Some(path) = &record.path {
                    update.check_signatures.push((id.clone(), PathBuf::from(path)));
                }
                AppRecord { first_seen: known.first_seen, trusted: known.trusted, ..record }
            }
            None => {
                // First time this program has ever been online.
                let learning = self.known.is_learning(now);
                // Outside the learning period the signature decides whether
                // this is news, so check it now rather than in the background.
                let signature = match (&record.path, learning) {
                    (Some(path), false) => (programs.sign)(Path::new(path)),
                    (Some(path), true) => {
                        update.check_signatures.push((id.clone(), PathBuf::from(path)));
                        Signature::Unknown
                    }
                    (None, _) => Signature::Unknown,
                };
                let publisher = match &signature {
                    Signature::Signed { publisher } => Some(publisher.clone()),
                    _ => None,
                };
                // An update that moved to a new folder: same publisher, same
                // program name. It keeps the earlier copy's trust, quietly.
                let earlier = publisher.as_deref().and_then(|p| self.known.same_program(p, &record.exe)).cloned();
                let trusted = learning || earlier.as_ref().is_some_and(|e| e.trusted);
                self.known.insert(
                    id.clone(),
                    KnownApp { name: record.name.clone(), first_seen: now, trusted, publisher },
                );
                update.known_changed = true;
                if !learning && earlier.is_none() {
                    update.new_apps.push(NewApp { id: id.clone(), name: record.name.clone() });
                }
                AppRecord { first_seen: earlier.map_or(now, |e| e.first_seen), trusted, signature, ..record }
            }
        };
        self.apps.insert(id.clone(), record);
        id
    }

    /// Notes an address the first time it is seen: for an owner lookup, and
    /// for a reverse lookup unless an app already looked its name up.
    fn note_ip(&mut self, ip: IpAddr, update: &mut Update) {
        if self.seen_ips.insert(ip) {
            update.new_addresses.push(ip);
            if !self.asked_names.contains_key(&ip) {
                update.new_ips.push(ip);
            }
        }
    }

    /// Adds the bytes a connection carried since it was last counted, and
    /// notes the activity for the history.
    #[allow(clippy::too_many_arguments)]
    fn add_traffic(
        &mut self,
        app_id: &str,
        counted_sent: u64,
        counted_received: u64,
        seen: &Observed,
        opened: bool,
        now: u64,
        update: &mut Update,
    ) {
        let sent = seen.sent.saturating_sub(counted_sent);
        let received = seen.received.saturating_sub(counted_received);
        let Some(app) = self.apps.get_mut(app_id) else { return };
        app.bytes_sent += sent;
        app.bytes_received += received;
        if opened || sent > 0 || received > 0 {
            update.activity.push(Activity {
                app_id: app_id.to_string(),
                app_name: app.name.clone(),
                protocol: seen.conn.protocol,
                remote_ip: seen.conn.remote.ip(),
                remote_port: seen.conn.remote.port(),
                host: self.name_of(seen.conn.remote.ip()).map(str::to_string),
                opened: u32::from(opened),
                sent,
                received,
                time: now,
            });
        }
    }

    fn push_event(&mut self, kind: EventKind, app_id: &str, conn: &Connection, sent: u64, received: u64, now: u64) {
        let app_name = self.apps.get(app_id).map(|a| a.name.clone()).unwrap_or_default();
        self.events.push_back(FlowEvent {
            seq: self.next_event,
            time: now,
            kind,
            app_id: app_id.to_string(),
            app_name,
            protocol: conn.protocol,
            remote_ip: conn.remote.ip().to_string(),
            remote_port: conn.remote.port(),
            host: self.name_of(conn.remote.ip()).map(str::to_string),
            bytes_sent: sent,
            bytes_received: received,
        });
        self.next_event += 1;
        if self.events.len() > MAX_EVENTS {
            self.events.pop_front();
        }
    }
}

/// Connections to this same machine are not internet access, and messages
/// to everyone on the local network (multicast, broadcast) are not one app
/// talking to one server.
pub fn leaves_this_machine(conn: &Connection) -> bool {
    let ip = conn.remote.ip();
    let to_everyone = match ip {
        IpAddr::V4(v4) => v4.is_multicast() || v4.is_broadcast(),
        IpAddr::V6(v6) => v6.is_multicast(),
    };
    !ip.is_loopback() && !ip.is_unspecified() && !to_everyone
}

/// How the tracker learns about programs: who runs a process, and who
/// signed a program file.
struct Programs<'a> {
    lookup: &'a dyn Fn(u32) -> Option<ProcessInfo>,
    sign: &'a dyn Fn(&Path) -> Signature,
}

/// Works out an app's identity and name from what the OS reported.
pub fn describe_process(info: Option<ProcessInfo>) -> (String, String) {
    let (id, record) = describe(info);
    (id, record.name)
}

fn describe(info: Option<ProcessInfo>) -> (String, AppRecord) {
    let blank = |name: String, exe: String, path: Option<String>| AppRecord {
        name,
        exe,
        path,
        first_seen: 0,
        trusted: true,
        total_opened: 0,
        bytes_sent: 0,
        bytes_received: 0,
        signature: Signature::Unknown,
    };

    let Some(info) = info else {
        return (
            SYSTEM_APP_ID.to_string(),
            blank(SYSTEM_APP_NAME.to_string(), String::new(), None),
        );
    };

    let path = info.path.to_string_lossy().into_owned();
    let exe = info.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let stem = info.path.file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let name = info
        .display_name
        .map(|n| n.trim().to_string())
        .filter(|n| !n.is_empty())
        .unwrap_or(if stem.is_empty() { exe.clone() } else { stem });

    (path.to_lowercase(), blank(name, exe, Some(path)))
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::net::SocketAddr;
    use std::path::PathBuf;

    use super::*;
    use crate::known_apps::LEARNING_MS;
    use crate::model::{Protocol, ProcessRecord};

    /// A pretend operating system the tests can script.
    #[derive(Default)]
    struct FakeOs {
        connections: RefCell<Vec<Connection>>,
        programs: HashMap<u32, (&'static str, Option<&'static str>)>,
        /// Publisher by lower-cased path; anything else is unsigned.
        signers: HashMap<String, &'static str>,
        signature_checks: RefCell<u32>,
    }

    impl FakeOs {
        fn with_programs(programs: &[(u32, &'static str, Option<&'static str>)]) -> Self {
            Self {
                connections: RefCell::default(),
                programs: programs.iter().map(|(pid, path, name)| (*pid, (*path, *name))).collect(),
                signers: HashMap::new(),
                signature_checks: RefCell::new(0),
            }
        }

        fn set(&self, connections: &[(u32, &str)]) {
            *self.connections.borrow_mut() = connections
                .iter()
                .enumerate()
                .map(|(i, (pid, remote))| Connection {
                    pid: *pid,
                    protocol: Protocol::Tcp,
                    local: SocketAddr::from(([192, 168, 1, 10], 50_000 + i as u16)),
                    remote: remote.parse().unwrap(),
                })
                .collect();
        }
    }

    impl Platform for FakeOs {
        fn tcp_connections(&self) -> io::Result<Vec<Connection>> {
            Ok(self.connections.borrow().clone())
        }

        fn process_info(&self, pid: u32) -> Option<ProcessInfo> {
            self.programs.get(&pid).map(|(path, name)| ProcessInfo {
                path: PathBuf::from(path),
                display_name: name.map(str::to_string),
            })
        }

        fn signature(&self, path: &Path) -> Signature {
            *self.signature_checks.borrow_mut() += 1;
            match self.signers.get(&path.to_string_lossy().to_lowercase()) {
                Some(publisher) => Signature::Signed { publisher: publisher.to_string() },
                None => Signature::Unsigned,
            }
        }
    }

    const AFTER_LEARNING: u64 = LEARNING_MS + 1;

    fn os() -> FakeOs {
        FakeOs::with_programs(&[
            (10, "C:\\Apps\\Chrome\\chrome.exe", Some("Google Chrome")),
            (20, "C:\\Apps\\tool.exe", None),
        ])
    }

    #[test]
    fn connections_open_at_start_are_not_events() {
        let os = os();
        os.set(&[(10, "203.0.113.5:443")]);
        let mut tracker = Tracker::new(KnownApps::first_run(0));

        tracker.poll(&os, 1).unwrap();
        let snapshot = tracker.snapshot(1, 0);

        assert!(snapshot.events.is_empty());
        assert_eq!(snapshot.connections.len(), 1);
        assert_eq!(snapshot.apps[0].name, "Google Chrome");
        assert_eq!(snapshot.apps[0].connections, 1);
    }

    #[test]
    fn opening_and_closing_become_events() {
        let os = os();
        let mut tracker = Tracker::new(KnownApps::first_run(0));
        tracker.poll(&os, 1).unwrap();

        os.set(&[(10, "203.0.113.5:443")]);
        tracker.poll(&os, 2).unwrap();
        os.set(&[]);
        tracker.poll(&os, 3).unwrap();

        let events = tracker.snapshot(3, 0).events;
        assert_eq!(events.len(), 2);
        assert_eq!((events[0].kind, events[0].seq, events[0].remote_port), (EventKind::Opened, 1, 443));
        assert_eq!((events[1].kind, events[1].seq), (EventKind::Closed, 2));
        assert_eq!(events[1].app_name, "Google Chrome");

        // Asking for events after the first one returns only the second.
        assert_eq!(tracker.snapshot(3, 1).events.len(), 1);
        assert_eq!(tracker.last_event_seq(), 2);
    }

    #[test]
    fn apps_seen_while_learning_are_trusted_silently() {
        let os = os();
        os.set(&[(10, "203.0.113.5:443")]);
        let mut tracker = Tracker::new(KnownApps::first_run(0));

        let update = tracker.poll(&os, 1).unwrap();

        assert!(update.new_apps.is_empty());
        assert!(update.known_changed);
        assert!(tracker.snapshot(1, 0).apps[0].trusted);
    }

    #[test]
    fn a_new_app_after_learning_raises_an_alert_once() {
        let os = os();
        let mut tracker = Tracker::new(KnownApps::first_run(0));
        tracker.poll(&os, AFTER_LEARNING).unwrap();

        os.set(&[(20, "198.51.100.7:80")]);
        let update = tracker.poll(&os, AFTER_LEARNING + 1).unwrap();
        assert_eq!(update.new_apps, vec![NewApp { id: "c:\\apps\\tool.exe".into(), name: "tool".into() }]);
        assert!(!tracker.snapshot(AFTER_LEARNING + 1, 0).apps[0].trusted);

        // A second connection from the same app is not news.
        os.set(&[(20, "198.51.100.7:80"), (20, "198.51.100.8:80")]);
        assert!(tracker.poll(&os, AFTER_LEARNING + 2).unwrap().new_apps.is_empty());

        assert!(tracker.trust("c:\\apps\\tool.exe"));
        assert!(tracker.snapshot(AFTER_LEARNING + 3, 0).apps[0].trusted);
        assert!(tracker.known().get("c:\\apps\\tool.exe").unwrap().trusted);
    }

    #[test]
    fn a_known_app_is_not_new_after_a_restart() {
        let os = os();
        os.set(&[(20, "198.51.100.7:80")]);
        let mut first = Tracker::new(KnownApps::first_run(0));
        first.poll(&os, AFTER_LEARNING).unwrap();
        let saved = first.known().clone();

        let mut second = Tracker::new(saved);
        let update = second.poll(&os, AFTER_LEARNING + 10).unwrap();

        assert!(update.new_apps.is_empty());
        assert!(!update.known_changed);
        assert_eq!(second.snapshot(0, 0).apps[0].first_seen, AFTER_LEARNING);
    }

    #[test]
    fn traffic_to_this_machine_is_ignored() {
        let os = os();
        os.set(&[(10, "127.0.0.1:8080"), (10, "[::1]:8080"), (10, "0.0.0.0:0")]);
        let mut tracker = Tracker::new(KnownApps::first_run(0));

        tracker.poll(&os, 1).unwrap();

        let snapshot = tracker.snapshot(1, 0);
        assert!(snapshot.connections.is_empty());
        assert!(snapshot.apps.is_empty());
    }

    #[test]
    fn unnamed_processes_share_one_system_app() {
        let os = os();
        os.set(&[(4, "203.0.113.5:443"), (900, "203.0.113.6:443")]);
        let mut tracker = Tracker::new(KnownApps::first_run(0));

        tracker.poll(&os, 1).unwrap();

        let apps = tracker.snapshot(1, 0).apps;
        assert_eq!(apps.len(), 1);
        assert_eq!((apps[0].id.as_str(), apps[0].connections, apps[0].servers), (SYSTEM_APP_ID, 2, 2));
        assert_eq!(apps[0].path, None);
    }

    #[test]
    fn host_names_reach_connections_once_known() {
        let os = os();
        os.set(&[(10, "203.0.113.5:443")]);
        let mut tracker = Tracker::new(KnownApps::first_run(0));

        let update = tracker.poll(&os, 1).unwrap();
        assert_eq!(update.new_ips, vec!["203.0.113.5".parse::<IpAddr>().unwrap()]);
        // The same address is only reported for lookup once.
        assert!(tracker.poll(&os, 2).unwrap().new_ips.is_empty());

        tracker.set_host("203.0.113.5".parse().unwrap(), "example.com".into());
        assert_eq!(tracker.snapshot(2, 0).connections[0].host.as_deref(), Some("example.com"));
    }

    #[test]
    fn a_reused_process_id_is_looked_up_again() {
        let mut os = os();
        os.set(&[(10, "203.0.113.5:443")]);
        let mut tracker = Tracker::new(KnownApps::first_run(0));
        tracker.poll(&os, 1).unwrap();

        // Chrome exits; later the OS hands process ID 10 to another program.
        os.set(&[]);
        tracker.poll(&os, 2).unwrap();
        os.programs.insert(10, ("C:\\Apps\\other.exe", None));
        os.set(&[(10, "203.0.113.9:443")]);
        tracker.poll(&os, 3).unwrap();

        let online: Vec<String> =
            tracker.snapshot(3, 0).apps.into_iter().filter(|a| a.connections > 0).map(|a| a.name).collect();
        assert_eq!(online, vec!["other".to_string()]);
    }

    fn tcp(pid: u32, local_port: u16, remote: &str) -> Connection {
        Connection {
            pid,
            protocol: Protocol::Tcp,
            local: SocketAddr::from(([192, 168, 1, 10], local_port)),
            remote: remote.parse().unwrap(),
        }
    }

    fn udp(pid: u32, remote: &str) -> Connection {
        Connection { protocol: Protocol::Udp, ..tcp(pid, 0, remote) }
    }

    fn seen(conn: &Connection, sent: u64, received: u64) -> Observed {
        Observed { conn: conn.clone(), sent, received }
    }

    fn detailed(open: Vec<Observed>, closed: Vec<Observed>) -> Observation {
        Observation { source: Source::Detailed, open, closed, ..Observation::default() }
    }

    #[test]
    fn a_connection_too_short_for_two_readings_still_shows_and_alerts() {
        let os = os();
        let mut tracker = Tracker::new(KnownApps::first_run(0));
        tracker.update(detailed(vec![], vec![]), &os, AFTER_LEARNING);

        let quick = tcp(20, 50_001, "198.51.100.7:443");
        let update = tracker.update(detailed(vec![], vec![seen(&quick, 300, 4_000)]), &os, AFTER_LEARNING + 1);

        assert_eq!(update.new_apps.len(), 1);
        let snapshot = tracker.snapshot(AFTER_LEARNING + 1, 0);
        let kinds: Vec<EventKind> = snapshot.events.iter().map(|e| e.kind).collect();
        assert_eq!(kinds, vec![EventKind::Opened, EventKind::Closed]);
        assert_eq!((snapshot.events[1].bytes_sent, snapshot.events[1].bytes_received), (300, 4_000));
        assert!(snapshot.connections.is_empty());
        assert_eq!((snapshot.apps[0].bytes_sent, snapshot.apps[0].bytes_received, snapshot.apps[0].total_opened), (300, 4_000, 1));
        assert_eq!(snapshot.source, Source::Detailed);
    }

    #[test]
    fn bytes_add_up_per_connection_and_per_app() {
        let os = os();
        let mut tracker = Tracker::new(KnownApps::first_run(0));
        let a = tcp(10, 50_001, "203.0.113.5:443");
        let b = tcp(10, 50_002, "203.0.113.6:443");

        tracker.update(detailed(vec![seen(&a, 100, 1_000)], vec![]), &os, 1);
        tracker.update(detailed(vec![seen(&a, 150, 5_000), seen(&b, 10, 20)], vec![]), &os, 2);
        let snapshot = tracker.snapshot(2, 0);
        let conn_a = snapshot.connections.iter().find(|c| c.local_port == 50_001).unwrap();
        assert_eq!((conn_a.bytes_sent, conn_a.bytes_received), (150, 5_000));
        assert_eq!((snapshot.apps[0].bytes_sent, snapshot.apps[0].bytes_received), (160, 5_020));

        // `a` closes; its last bytes still count, and the app keeps its total.
        tracker.update(detailed(vec![seen(&b, 10, 20)], vec![seen(&a, 200, 6_000)]), &os, 3);
        let snapshot = tracker.snapshot(3, 0);
        assert_eq!((snapshot.apps[0].bytes_sent, snapshot.apps[0].bytes_received), (210, 6_020));
        let closed = snapshot.events.iter().find(|e| e.kind == EventKind::Closed).unwrap();
        assert_eq!((closed.bytes_sent, closed.bytes_received), (200, 6_000));
        assert_eq!(snapshot.connections.len(), 1);
    }

    #[test]
    fn udp_is_tracked_but_messages_to_everyone_are_not() {
        let os = os();
        let mut tracker = Tracker::new(KnownApps::first_run(0));
        let quic = udp(10, "203.0.113.5:443");
        let mdns = udp(10, "224.0.0.251:5353");
        let ssdp = udp(10, "[ff02::c]:1900");
        let broadcast = udp(10, "255.255.255.255:67");

        tracker.update(
            detailed(vec![seen(&quic, 1, 2), seen(&mdns, 1, 0), seen(&ssdp, 1, 0), seen(&broadcast, 1, 0)], vec![]),
            &os,
            1,
        );

        let connections = tracker.snapshot(1, 0).connections;
        assert_eq!(connections.len(), 1);
        assert_eq!((connections[0].protocol, connections[0].remote_port), (Protocol::Udp, 443));
    }

    #[test]
    fn names_apps_looked_up_beat_reverse_lookups() {
        let os = os();
        let mut tracker = Tracker::new(KnownApps::first_run(0));
        let ip: IpAddr = "140.82.112.3".parse().unwrap();
        let conn = tcp(10, 50_001, "140.82.112.3:443");

        tracker.set_host(ip, "lb-140-82-112-3-iad.github.com".into());
        let mut reading = detailed(vec![seen(&conn, 0, 0)], vec![]);
        reading.names = vec![(ip, "github.com".into())];
        let update = tracker.update(reading, &os, 1);

        // Already named, so no reverse lookup is asked for.
        assert!(update.new_ips.is_empty());
        assert_eq!(tracker.snapshot(1, 0).connections[0].host.as_deref(), Some("github.com"));
        tracker.set_host(ip, "something-else.example".into());
        assert_eq!(tracker.name_of(ip), Some("github.com"));
        assert_eq!(tracker.names().get(&ip).map(String::as_str), Some("github.com"));
    }

    #[test]
    fn the_collector_names_programs_windows_hides_from_the_window() {
        let os = os();
        let mut tracker = Tracker::new(KnownApps::first_run(0));
        let conn = tcp(4, 50_001, "203.0.113.5:443");
        let mut reading = detailed(vec![seen(&conn, 0, 0)], vec![]);
        reading.processes =
            vec![ProcessRecord { pid: 4, path: "C:\\Windows\\System32\\svchost.exe".into(), name: Some("Host Process for Windows Services".into()) }];

        tracker.update(reading, &os, 1);

        let apps = tracker.snapshot(1, 0).apps;
        assert_eq!(apps[0].name, "Host Process for Windows Services");
        assert_eq!(tracker.app_of_pid(4), Some("c:\\windows\\system32\\svchost.exe"));
    }

    #[test]
    fn switching_sources_closes_what_the_new_one_does_not_list() {
        let os = os();
        os.set(&[(10, "203.0.113.5:443")]);
        let mut tracker = Tracker::new(KnownApps::first_run(0));
        let quic = udp(10, "203.0.113.9:443");
        tracker.update(detailed(vec![seen(&quic, 5, 5)], vec![]), &os, 1);

        // Back to the TCP table: the UDP flow is no longer listed, so it closes.
        tracker.poll(&os, 2).unwrap();
        let snapshot = tracker.snapshot(2, 0);
        assert_eq!(snapshot.source, Source::Basic);
        assert_eq!(snapshot.connections.len(), 1);
        assert_eq!(snapshot.connections[0].protocol, Protocol::Tcp);
        assert!(snapshot.events.iter().any(|e| e.kind == EventKind::Closed && e.protocol == Protocol::Udp));
    }

    #[test]
    fn an_update_in_a_new_folder_keeps_its_trust_without_an_alert() {
        let mut os = FakeOs::with_programs(&[
            (30, "C:\\Discord\\app-1.0.1\\Discord.exe", Some("Discord")),
            (31, "C:\\Discord\\app-1.0.2\\Discord.exe", Some("Discord")),
            (32, "C:\\Temp\\app-1.0.3\\Discord.exe", Some("Discord")),
        ]);
        os.signers.insert("c:\\discord\\app-1.0.1\\discord.exe".into(), "Discord Inc.");
        os.signers.insert("c:\\discord\\app-1.0.2\\discord.exe".into(), "Discord Inc.");
        let mut tracker = Tracker::new(KnownApps::first_run(0));
        tracker.poll(&os, AFTER_LEARNING).unwrap();

        // First sighting: a new app, with its publisher checked straight away.
        os.set(&[(30, "203.0.113.5:443")]);
        let first = tracker.poll(&os, AFTER_LEARNING + 1).unwrap();
        assert_eq!(first.new_apps.len(), 1);
        assert!(tracker.trust("c:\\discord\\app-1.0.1\\discord.exe"));

        // The update moves to a new folder: same publisher and file name.
        os.set(&[(31, "203.0.113.5:443")]);
        let update = tracker.poll(&os, AFTER_LEARNING + 2).unwrap();
        assert!(update.new_apps.is_empty());
        let app = tracker.snapshot(0, 0).apps.into_iter().find(|a| a.path.as_deref().unwrap().contains("1.0.2")).unwrap();
        assert!(app.trusted);
        assert_eq!(app.signature, Signature::Signed { publisher: "Discord Inc.".into() });
        assert_eq!(app.first_seen, AFTER_LEARNING + 1);

        // A look-alike that isn't signed by Discord is still news.
        os.set(&[(32, "203.0.113.5:443")]);
        assert_eq!(tracker.poll(&os, AFTER_LEARNING + 3).unwrap().new_apps.len(), 1);
    }

    #[test]
    fn signatures_are_checked_in_the_background_while_learning() {
        let mut os = os();
        os.signers.insert("c:\\apps\\chrome\\chrome.exe".into(), "Google LLC");
        os.set(&[(10, "203.0.113.5:443")]);
        let mut tracker = Tracker::new(KnownApps::first_run(0));

        let update = tracker.poll(&os, 1).unwrap();
        assert_eq!(*os.signature_checks.borrow(), 0);
        assert_eq!(update.check_signatures.len(), 1);
        let (id, path) = &update.check_signatures[0];
        assert_eq!(tracker.snapshot(1, 0).apps[0].signature, Signature::Unknown);

        let signature = os.signature(path);
        assert!(tracker.set_signature(id, signature.clone()));
        assert_eq!(tracker.snapshot(1, 0).apps[0].signature, signature);
        assert_eq!(tracker.known().get(id).unwrap().publisher.as_deref(), Some("Google LLC"));
        // Recording the same answer again changes nothing on disk.
        assert!(!tracker.set_signature(id, signature));
    }

    #[test]
    fn activity_records_opens_and_bytes_for_the_history() {
        let os = os();
        let mut tracker = Tracker::new(KnownApps::first_run(0));
        let conn = tcp(10, 50_001, "203.0.113.5:443");

        let first = tracker.update(detailed(vec![seen(&conn, 100, 1_000)], vec![]), &os, 1);
        assert_eq!(first.activity.len(), 1);
        assert_eq!((first.activity[0].opened, first.activity[0].sent, first.activity[0].received), (1, 100, 1_000));
        assert_eq!(first.new_addresses, vec!["203.0.113.5".parse::<IpAddr>().unwrap()]);

        // Only the new bytes count next time; a quiet reading adds nothing.
        let second = tracker.update(detailed(vec![seen(&conn, 150, 1_000)], vec![]), &os, 2);
        assert_eq!((second.activity[0].opened, second.activity[0].sent, second.activity[0].received), (0, 50, 0));
        assert!(tracker.update(detailed(vec![seen(&conn, 150, 1_000)], vec![]), &os, 3).activity.is_empty());

        // Closing reports the last bytes.
        let last = tracker.update(detailed(vec![], vec![seen(&conn, 150, 1_200)]), &os, 4);
        assert_eq!((last.activity[0].received, last.activity[0].app_name.as_str()), (200, "Google Chrome"));
    }

    #[test]
    fn owners_reach_connections() {
        let os = os();
        os.set(&[(10, "203.0.113.5:443")]);
        let mut tracker = Tracker::new(KnownApps::first_run(0));
        tracker.poll(&os, 1).unwrap();
        let owner = OwnerInfo { name: "Example Networks".into(), asn: Some(64_500), country: Some("NL".into()) };
        tracker.set_owner("203.0.113.5".parse().unwrap(), owner);

        let conn = &tracker.snapshot(1, 0).connections[0];
        assert_eq!(conn.owner.as_deref(), Some("Example Networks"));
        assert_eq!((conn.asn, conn.country.as_deref()), (Some(64_500), Some("NL")));
    }

    #[test]
    fn learning_again_trusts_everything_and_quiets_new_apps_for_a_while() {
        let os = os();
        let mut tracker = Tracker::new(KnownApps::first_run(0));
        tracker.poll(&os, AFTER_LEARNING).unwrap();
        os.set(&[(20, "198.51.100.7:80")]);
        assert_eq!(tracker.poll(&os, AFTER_LEARNING + 1).unwrap().new_apps.len(), 1);

        let now = AFTER_LEARNING + 2;
        tracker.learn_again(now);
        assert!(tracker.snapshot(now, 0).apps.iter().all(|a| a.trusted));
        assert_eq!(tracker.snapshot(now, 0).learning_seconds_left, 120);

        // A new app during the new learning period is trusted quietly.
        os.set(&[(20, "198.51.100.7:80"), (10, "203.0.113.5:443")]);
        let update = tracker.poll(&os, now + 1).unwrap();
        assert!(update.new_apps.is_empty());
        assert!(tracker.known().get("c:\\apps\\chrome\\chrome.exe").unwrap().trusted);
    }

    #[test]
    fn forgetting_keeps_only_whats_online_now() {
        let os = os();
        os.set(&[(10, "203.0.113.5:443")]);
        let mut tracker = Tracker::new(KnownApps::first_run(0));
        tracker.poll(&os, 1).unwrap();
        let mut gone = tracker.known().clone();
        gone.insert("c:\\old\\gone.exe".into(), KnownApp { name: "Gone".into(), ..KnownApp::default() });
        let mut tracker = Tracker::new(gone);
        tracker.poll(&os, 2).unwrap();
        assert_eq!(tracker.known().len(), 2);

        tracker.forget_known_apps(AFTER_LEARNING);
        assert_eq!(tracker.known().len(), 1);
        assert!(tracker.known().is_learning(AFTER_LEARNING + 1));
    }
}
