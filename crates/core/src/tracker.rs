//! Turns raw connection snapshots into apps, live connections and events.
//!
//! Call [`Tracker::update`] on a timer. Each call compares the connections that
//! are open now with the ones from the previous call: anything new is an
//! "opened" event, anything gone is a "closed" event.

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::io;
use std::net::IpAddr;

use crate::known_apps::{KnownApp, KnownApps};
use crate::model::{
    AppView, ConnView, Connection, EventKind, FlowEvent, NewApp, Platform, ProcessInfo, Snapshot,
};

/// All processes the OS will not name are grouped under this one app.
pub const SYSTEM_APP_ID: &str = "system";
const SYSTEM_APP_NAME: &str = "Windows system services";

/// How many past events are kept for front-ends that start late.
const MAX_EVENTS: usize = 300;

struct AppRecord {
    name: String,
    exe: String,
    path: Option<String>,
    first_seen: u64,
    trusted: bool,
    total_opened: u64,
}

struct ConnRecord {
    id: u64,
    app_id: String,
    opened_at: u64,
}

/// What one [`Tracker::update`] call found.
#[derive(Debug, Default)]
pub struct Update {
    /// Apps that went online for the first time ever. Empty while learning.
    pub new_apps: Vec<NewApp>,
    /// Remote addresses seen for the first time, for name lookups.
    pub new_ips: Vec<IpAddr>,
    /// The known-apps list changed and should be saved.
    pub known_changed: bool,
}

pub struct Tracker {
    known: KnownApps,
    apps: BTreeMap<String, AppRecord>,
    open: HashMap<Connection, ConnRecord>,
    /// Which app each process ID belongs to, while it has open connections.
    pid_apps: HashMap<u32, String>,
    hosts: HashMap<IpAddr, String>,
    seen_ips: HashSet<IpAddr>,
    events: VecDeque<FlowEvent>,
    next_event: u64,
    next_conn: u64,
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
            hosts: HashMap::new(),
            seen_ips: HashSet::new(),
            events: VecDeque::new(),
            next_event: 1,
            next_conn: 1,
            primed: false,
        }
    }

    /// Reads the current connections and records what changed since last time.
    pub fn update(&mut self, platform: &dyn Platform, now: u64) -> io::Result<Update> {
        let mut current: Vec<Connection> =
            platform.tcp_connections()?.into_iter().filter(leaves_this_machine).collect();
        // A fixed order keeps events and tests repeatable.
        current.sort_by_key(|c| (c.pid, c.remote, c.local));
        let current_set: HashSet<&Connection> = current.iter().collect();

        let mut update = Update::default();

        let closed: Vec<Connection> =
            self.open.keys().filter(|c| !current_set.contains(c)).cloned().collect();
        for conn in closed {
            if let Some(record) = self.open.remove(&conn) {
                self.push_event(EventKind::Closed, &record.app_id, &conn, now);
            }
        }

        for conn in &current {
            if self.open.contains_key(conn) {
                continue;
            }
            let app_id = self.app_for(conn.pid, platform, now, &mut update);
            if let Some(app) = self.apps.get_mut(&app_id) {
                app.total_opened += 1;
            }
            if self.seen_ips.insert(conn.remote.ip()) {
                update.new_ips.push(conn.remote.ip());
            }
            if self.primed {
                self.push_event(EventKind::Opened, &app_id, conn, now);
            }
            let record = ConnRecord { id: self.next_conn, app_id, opened_at: now };
            self.next_conn += 1;
            self.open.insert(conn.clone(), record);
        }

        // A process ID can be reused by a different program later, so only
        // remember it while it still has connections.
        let live_pids: HashSet<u32> = self.open.keys().map(|c| c.pid).collect();
        self.pid_apps.retain(|pid, _| live_pids.contains(pid));

        self.primed = true;
        Ok(update)
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
                }
            })
            .collect();
        apps.sort_by(|a, b| {
            b.connections.cmp(&a.connections).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });

        let mut connections: Vec<ConnView> = self
            .open
            .iter()
            .map(|(conn, record)| ConnView {
                id: record.id,
                app_id: record.app_id.clone(),
                pid: conn.pid,
                remote_ip: conn.remote.ip().to_string(),
                remote_port: conn.remote.port(),
                local_port: conn.local.port(),
                host: self.hosts.get(&conn.remote.ip()).cloned(),
                opened_at: record.opened_at,
            })
            .collect();
        connections.sort_by_key(|c| c.id);

        Snapshot {
            now,
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

    /// Records the name a remote address resolves to.
    pub fn set_host(&mut self, ip: IpAddr, host: String) {
        self.hosts.insert(ip, host);
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

    pub fn known(&self) -> &KnownApps {
        &self.known
    }

    /// Finds, or creates, the app a process belongs to.
    fn app_for(&mut self, pid: u32, platform: &dyn Platform, now: u64, update: &mut Update) -> String {
        if let Some(id) = self.pid_apps.get(&pid) {
            return id.clone();
        }

        let (id, record) = describe(platform.process_info(pid));
        self.pid_apps.insert(pid, id.clone());
        if self.apps.contains_key(&id) {
            return id;
        }

        let record = match self.known.get(&id) {
            Some(known) => AppRecord { first_seen: known.first_seen, trusted: known.trusted, ..record },
            None => {
                // First time this app has ever been online.
                let learning = self.known.is_learning(now);
                self.known.insert(
                    id.clone(),
                    KnownApp { name: record.name.clone(), first_seen: now, trusted: learning },
                );
                update.known_changed = true;
                if !learning {
                    update.new_apps.push(NewApp { id: id.clone(), name: record.name.clone() });
                }
                AppRecord { first_seen: now, trusted: learning, ..record }
            }
        };
        self.apps.insert(id.clone(), record);
        id
    }

    fn push_event(&mut self, kind: EventKind, app_id: &str, conn: &Connection, now: u64) {
        let app_name = self.apps.get(app_id).map(|a| a.name.clone()).unwrap_or_default();
        self.events.push_back(FlowEvent {
            seq: self.next_event,
            time: now,
            kind,
            app_id: app_id.to_string(),
            app_name,
            remote_ip: conn.remote.ip().to_string(),
            remote_port: conn.remote.port(),
            host: self.hosts.get(&conn.remote.ip()).cloned(),
        });
        self.next_event += 1;
        if self.events.len() > MAX_EVENTS {
            self.events.pop_front();
        }
    }
}

/// Connections to this same machine are not internet access.
fn leaves_this_machine(conn: &Connection) -> bool {
    let ip = conn.remote.ip();
    !ip.is_loopback() && !ip.is_unspecified()
}

/// Works out an app's identity and names from what the OS reported.
fn describe(info: Option<ProcessInfo>) -> (String, AppRecord) {
    let blank = |name: String, exe: String, path: Option<String>| AppRecord {
        name,
        exe,
        path,
        first_seen: 0,
        trusted: true,
        total_opened: 0,
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

    /// A pretend operating system the tests can script.
    #[derive(Default)]
    struct FakeOs {
        connections: RefCell<Vec<Connection>>,
        programs: HashMap<u32, (&'static str, Option<&'static str>)>,
    }

    impl FakeOs {
        fn with_programs(programs: &[(u32, &'static str, Option<&'static str>)]) -> Self {
            Self {
                connections: RefCell::default(),
                programs: programs.iter().map(|(pid, path, name)| (*pid, (*path, *name))).collect(),
            }
        }

        fn set(&self, connections: &[(u32, &str)]) {
            *self.connections.borrow_mut() = connections
                .iter()
                .enumerate()
                .map(|(i, (pid, remote))| Connection {
                    pid: *pid,
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

        tracker.update(&os, 1).unwrap();
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
        tracker.update(&os, 1).unwrap();

        os.set(&[(10, "203.0.113.5:443")]);
        tracker.update(&os, 2).unwrap();
        os.set(&[]);
        tracker.update(&os, 3).unwrap();

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

        let update = tracker.update(&os, 1).unwrap();

        assert!(update.new_apps.is_empty());
        assert!(update.known_changed);
        assert!(tracker.snapshot(1, 0).apps[0].trusted);
    }

    #[test]
    fn a_new_app_after_learning_raises_an_alert_once() {
        let os = os();
        let mut tracker = Tracker::new(KnownApps::first_run(0));
        tracker.update(&os, AFTER_LEARNING).unwrap();

        os.set(&[(20, "198.51.100.7:80")]);
        let update = tracker.update(&os, AFTER_LEARNING + 1).unwrap();
        assert_eq!(update.new_apps, vec![NewApp { id: "c:\\apps\\tool.exe".into(), name: "tool".into() }]);
        assert!(!tracker.snapshot(AFTER_LEARNING + 1, 0).apps[0].trusted);

        // A second connection from the same app is not news.
        os.set(&[(20, "198.51.100.7:80"), (20, "198.51.100.8:80")]);
        assert!(tracker.update(&os, AFTER_LEARNING + 2).unwrap().new_apps.is_empty());

        assert!(tracker.trust("c:\\apps\\tool.exe"));
        assert!(tracker.snapshot(AFTER_LEARNING + 3, 0).apps[0].trusted);
        assert!(tracker.known().get("c:\\apps\\tool.exe").unwrap().trusted);
    }

    #[test]
    fn a_known_app_is_not_new_after_a_restart() {
        let os = os();
        os.set(&[(20, "198.51.100.7:80")]);
        let mut first = Tracker::new(KnownApps::first_run(0));
        first.update(&os, AFTER_LEARNING).unwrap();
        let saved = first.known().clone();

        let mut second = Tracker::new(saved);
        let update = second.update(&os, AFTER_LEARNING + 10).unwrap();

        assert!(update.new_apps.is_empty());
        assert!(!update.known_changed);
        assert_eq!(second.snapshot(0, 0).apps[0].first_seen, AFTER_LEARNING);
    }

    #[test]
    fn traffic_to_this_machine_is_ignored() {
        let os = os();
        os.set(&[(10, "127.0.0.1:8080"), (10, "[::1]:8080"), (10, "0.0.0.0:0")]);
        let mut tracker = Tracker::new(KnownApps::first_run(0));

        tracker.update(&os, 1).unwrap();

        let snapshot = tracker.snapshot(1, 0);
        assert!(snapshot.connections.is_empty());
        assert!(snapshot.apps.is_empty());
    }

    #[test]
    fn unnamed_processes_share_one_system_app() {
        let os = os();
        os.set(&[(4, "203.0.113.5:443"), (900, "203.0.113.6:443")]);
        let mut tracker = Tracker::new(KnownApps::first_run(0));

        tracker.update(&os, 1).unwrap();

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

        let update = tracker.update(&os, 1).unwrap();
        assert_eq!(update.new_ips, vec!["203.0.113.5".parse::<IpAddr>().unwrap()]);
        // The same address is only reported for lookup once.
        assert!(tracker.update(&os, 2).unwrap().new_ips.is_empty());

        tracker.set_host("203.0.113.5".parse().unwrap(), "example.com".into());
        assert_eq!(tracker.snapshot(2, 0).connections[0].host.as_deref(), Some("example.com"));
    }

    #[test]
    fn a_reused_process_id_is_looked_up_again() {
        let mut os = os();
        os.set(&[(10, "203.0.113.5:443")]);
        let mut tracker = Tracker::new(KnownApps::first_run(0));
        tracker.update(&os, 1).unwrap();

        // Chrome exits; later the OS hands process ID 10 to another program.
        os.set(&[]);
        tracker.update(&os, 2).unwrap();
        os.programs.insert(10, ("C:\\Apps\\other.exe", None));
        os.set(&[(10, "203.0.113.9:443")]);
        tracker.update(&os, 3).unwrap();

        let online: Vec<String> =
            tracker.snapshot(3, 0).apps.into_iter().filter(|a| a.connections > 0).map(|a| a.name).collect();
        assert_eq!(online, vec!["other".to_string()]);
    }
}
