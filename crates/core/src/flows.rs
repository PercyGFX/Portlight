//! Turns a stream of low-level network events (a connection opened, some
//! bytes went out) into readings the [`Tracker`](crate::Tracker) understands.
//!
//! The Portlight Collector feeds every event Windows reports into a
//! [`FlowTable`] and calls [`FlowTable::take`] twice a second. Events can be
//! missed (a connection that was open before the collector started, a lost
//! "closed" event), so TCP flows are also checked against the TCP table now
//! and then with [`FlowTable::reconcile_tcp`].

use std::collections::{HashMap, HashSet};
use std::net::{IpAddr, SocketAddr};

use crate::model::{Connection, Observation, Observed, Protocol, Source};
use crate::tracker::leaves_this_machine;

/// A UDP conversation counts as over after this long without traffic.
pub const UDP_IDLE_MS: u64 = 20_000;
/// A TCP flow missing from the TCP table is only closed once it is older
/// than this and has been quiet this long, because the table and the events
/// are not read at the same instant.
const TCP_GRACE_MS: u64 = 3_000;
/// Stray events for a connection that just closed are ignored for this long.
const RECENTLY_CLOSED_MS: u64 = 5_000;
/// Names kept for clients that connect later; the oldest are dropped past this.
const MAX_NAMES: usize = 50_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetEventKind {
    /// This PC opened a TCP connection.
    Connect,
    /// This PC accepted a TCP connection from elsewhere.
    Accept,
    /// A TCP connection closed.
    Disconnect,
    Send,
    Receive,
}

/// One event as the operating system reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetEvent {
    pub kind: NetEventKind,
    pub protocol: Protocol,
    pub pid: u32,
    pub local: SocketAddr,
    pub remote: SocketAddr,
    /// Bytes, for send and receive events.
    pub size: u32,
}

struct Flow {
    sent: u64,
    received: u64,
    first_seen: u64,
    last_seen: u64,
}

#[derive(Default)]
pub struct FlowTable {
    flows: HashMap<Connection, Flow>,
    /// Ended since the last `take`.
    closed: Vec<Observed>,
    recently_closed: HashMap<Connection, u64>,
    names: HashMap<IpAddr, (String, u64)>,
    new_names: Vec<(IpAddr, String)>,
}

impl FlowTable {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn apply(&mut self, event: &NetEvent, now: u64) {
        let mut conn = key(event);
        // Data handled outside any program's context can be reported with
        // process ID 0; it belongs to whoever owns the connection.
        if conn.pid == 0
            && let Some(owner) = self.flows.keys().find(|c| {
                c.protocol == conn.protocol && c.local == conn.local && c.remote == conn.remote
            })
        {
            conn = owner.clone();
        }
        if !leaves_this_machine(&conn) {
            return;
        }

        match event.kind {
            NetEventKind::Disconnect => self.close(&conn, now),
            NetEventKind::Connect | NetEventKind::Accept => {
                // The same addresses and ports used again: a new connection.
                self.recently_closed.remove(&conn);
                self.flows.entry(conn).or_insert(Flow { sent: 0, received: 0, first_seen: now, last_seen: now });
            }
            NetEventKind::Send | NetEventKind::Receive => {
                let size = u64::from(event.size);
                let outgoing = event.kind == NetEventKind::Send;
                if let Some(flow) = self.flows.get_mut(&conn) {
                    flow.last_seen = now;
                    if outgoing { flow.sent += size } else { flow.received += size }
                } else if self.recently_closed.contains_key(&conn) {
                    // The last packets of a connection that has just closed.
                    if let Some(ended) = self.closed.iter_mut().rev().find(|o| o.conn == conn) {
                        if outgoing { ended.sent += size } else { ended.received += size }
                    }
                } else {
                    // UDP, or a TCP connection that was open before we started.
                    let (sent, received) = if outgoing { (size, 0) } else { (0, size) };
                    self.flows.insert(conn, Flow { sent, received, first_seen: now, last_seen: now });
                }
            }
        }
    }

    /// Brings TCP flows in line with the OS's TCP table: adds connections the
    /// events never mentioned and closes ones the OS no longer lists.
    pub fn reconcile_tcp(&mut self, table: &[Connection], now: u64) {
        // Process ID 0 owns connections in TIME_WAIT, which are already closed.
        let listed: HashSet<&Connection> =
            table.iter().filter(|c| c.pid != 0 && c.protocol == Protocol::Tcp && leaves_this_machine(c)).collect();

        for conn in &listed {
            if !self.flows.contains_key(*conn) && !self.recently_closed.contains_key(*conn) {
                let flow = Flow { sent: 0, received: 0, first_seen: now, last_seen: now };
                self.flows.insert((*conn).clone(), flow);
            }
        }

        let vanished: Vec<Connection> = self
            .flows
            .iter()
            .filter(|(conn, flow)| {
                conn.protocol == Protocol::Tcp
                    && !listed.contains(conn)
                    && now.saturating_sub(flow.first_seen) > TCP_GRACE_MS
                    && now.saturating_sub(flow.last_seen) > TCP_GRACE_MS
            })
            .map(|(conn, _)| conn.clone())
            .collect();
        for conn in vanished {
            self.close(&conn, now);
        }
    }

    /// Records the addresses a site name resolved to.
    pub fn add_name(&mut self, name: &str, ips: &[IpAddr], now: u64) {
        let name = name.trim().trim_end_matches('.').to_lowercase();
        if name.is_empty() {
            return;
        }
        for ip in ips {
            let ip = ip.to_canonical();
            if self.names.get(&ip).map(|(known, _)| known) != Some(&name) {
                self.new_names.push((ip, name.clone()));
            }
            self.names.insert(ip, (name.clone(), now));
        }
        if self.names.len() > MAX_NAMES {
            let mut ages: Vec<u64> = self.names.values().map(|(_, at)| *at).collect();
            ages.sort_unstable();
            let cutoff = ages[ages.len() / 2];
            self.names.retain(|_, (_, at)| *at > cutoff);
        }
    }

    /// Every name known, for a client that has just connected.
    pub fn all_names(&self) -> Vec<(IpAddr, String)> {
        self.names.iter().map(|(ip, (name, _))| (*ip, name.clone())).collect()
    }

    /// The current reading: open flows, flows that ended since the last call,
    /// and names learned since the last call.
    pub fn take(&mut self, now: u64) -> Observation {
        let idle: Vec<Connection> = self
            .flows
            .iter()
            .filter(|(conn, flow)| conn.protocol == Protocol::Udp && now.saturating_sub(flow.last_seen) > UDP_IDLE_MS)
            .map(|(conn, _)| conn.clone())
            .collect();
        for conn in idle {
            self.close(&conn, now);
        }
        self.recently_closed.retain(|_, at| now.saturating_sub(*at) <= RECENTLY_CLOSED_MS);

        Observation {
            source: Source::Detailed,
            open: self
                .flows
                .iter()
                .map(|(conn, flow)| Observed { conn: conn.clone(), sent: flow.sent, received: flow.received })
                .collect(),
            closed: std::mem::take(&mut self.closed),
            names: std::mem::take(&mut self.new_names),
            processes: Vec::new(),
        }
    }

    fn close(&mut self, conn: &Connection, now: u64) {
        if let Some(flow) = self.flows.remove(conn) {
            self.closed.push(Observed { conn: conn.clone(), sent: flow.sent, received: flow.received });
            self.recently_closed.insert(conn.clone(), now);
        }
    }
}

impl Observation {
    /// Folds a later reading into this one, for when several arrive between
    /// two ticks: the later one says what is open, and nothing that ended or
    /// was learned in between is lost.
    pub fn merge(&mut self, later: Observation) {
        self.source = later.source;
        self.open = later.open;
        self.closed.extend(later.closed);
        self.names.extend(later.names);
        self.processes.extend(later.processes);
    }
}

/// The flow an event belongs to. UDP flows ignore the local port, so the many
/// short exchanges an app has with one server count as one conversation.
fn key(event: &NetEvent) -> Connection {
    let local = SocketAddr::new(event.local.ip().to_canonical(), event.local.port());
    let remote = SocketAddr::new(event.remote.ip().to_canonical(), event.remote.port());
    let local = match event.protocol {
        Protocol::Tcp => local,
        Protocol::Udp => SocketAddr::new(local.ip(), 0),
    };
    Connection { pid: event.pid, protocol: event.protocol, local, remote }
}

/// Reads the addresses out of a Windows DNS result list, which looks like
/// `"type:  5 github.com;::ffff:140.82.112.3;2606:50c0::1;"`. Aliases
/// (`type: 5 ...`) are skipped and IPv4-mapped addresses become plain IPv4.
pub fn parse_dns_results(results: &str) -> Vec<IpAddr> {
    results
        .split(';')
        .map(str::trim)
        .filter(|part| !part.is_empty() && !part.starts_with("type"))
        .filter_map(|part| part.parse::<IpAddr>().ok())
        .map(|ip| ip.to_canonical())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const ME: [u8; 4] = [192, 168, 1, 10];

    fn event(kind: NetEventKind, protocol: Protocol, local_port: u16, remote: &str, size: u32) -> NetEvent {
        NetEvent {
            kind,
            protocol,
            pid: 10,
            local: SocketAddr::from((ME, local_port)),
            remote: remote.parse().unwrap(),
            size,
        }
    }

    fn tcp(kind: NetEventKind, size: u32) -> NetEvent {
        event(kind, Protocol::Tcp, 50_000, "203.0.113.5:443", size)
    }

    #[test]
    fn a_short_connection_is_reported_once_closed_with_its_bytes() {
        let mut table = FlowTable::new();
        table.apply(&tcp(NetEventKind::Connect, 0), 0);
        table.apply(&tcp(NetEventKind::Send, 500), 1);
        table.apply(&tcp(NetEventKind::Receive, 9_000), 2);
        table.apply(&tcp(NetEventKind::Disconnect, 0), 3);
        // A late packet after the close still counts, and does not reopen it.
        table.apply(&tcp(NetEventKind::Receive, 40), 4);

        let reading = table.take(5);
        assert!(reading.open.is_empty());
        assert_eq!(reading.closed.len(), 1);
        assert_eq!((reading.closed[0].sent, reading.closed[0].received), (500, 9_040));
        assert_eq!(reading.source, Source::Detailed);
        assert!(table.take(6).closed.is_empty());
    }

    #[test]
    fn data_without_a_process_goes_to_the_connection_owner() {
        let mut table = FlowTable::new();
        table.apply(&tcp(NetEventKind::Connect, 0), 0);
        table.apply(&NetEvent { pid: 0, ..tcp(NetEventKind::Receive, 70) }, 1);
        let reading = table.take(2);
        assert_eq!(reading.open.len(), 1);
        assert_eq!((reading.open[0].conn.pid, reading.open[0].received), (10, 70));
    }

    #[test]
    fn traffic_on_an_unknown_connection_opens_a_flow() {
        let mut table = FlowTable::new();
        table.apply(&tcp(NetEventKind::Receive, 100), 0);
        let reading = table.take(1);
        assert_eq!(reading.open.len(), 1);
        assert_eq!(reading.open[0].received, 100);
    }

    #[test]
    fn udp_exchanges_with_one_server_are_one_conversation_until_quiet() {
        let mut table = FlowTable::new();
        table.apply(&event(NetEventKind::Send, Protocol::Udp, 61_000, "192.168.1.1:53", 40), 0);
        table.apply(&event(NetEventKind::Receive, Protocol::Udp, 61_000, "192.168.1.1:53", 120), 1);
        table.apply(&event(NetEventKind::Send, Protocol::Udp, 61_001, "192.168.1.1:53", 40), 2);

        let reading = table.take(3);
        assert_eq!(reading.open.len(), 1);
        assert_eq!(reading.open[0].conn.local.port(), 0);
        assert_eq!((reading.open[0].sent, reading.open[0].received), (80, 120));

        let later = table.take(2 + UDP_IDLE_MS + 1);
        assert!(later.open.is_empty());
        assert_eq!(later.closed.len(), 1);
    }

    #[test]
    fn the_tcp_table_fills_gaps_and_clears_stale_flows() {
        let mut table = FlowTable::new();
        let before_start = Connection {
            pid: 10,
            protocol: Protocol::Tcp,
            local: SocketAddr::from((ME, 50_100)),
            remote: "198.51.100.7:443".parse().unwrap(),
        };
        let time_wait = Connection { pid: 0, ..before_start.clone() };
        table.apply(&tcp(NetEventKind::Connect, 0), 0);

        // Shortly after, the table only lists the older connection. The new
        // one is too young to close yet.
        table.reconcile_tcp(&[before_start.clone(), time_wait], 1_000);
        assert_eq!(table.take(1_000).open.len(), 2);

        // Much later the event-only flow is still missing, so it closes.
        table.reconcile_tcp(std::slice::from_ref(&before_start), 10_000);
        let reading = table.take(10_000);
        assert_eq!(reading.open.len(), 1);
        assert_eq!(reading.open[0].conn, before_start);
        assert_eq!(reading.closed.len(), 1);
    }

    #[test]
    fn traffic_to_this_machine_is_ignored() {
        let mut table = FlowTable::new();
        table.apply(&event(NetEventKind::Send, Protocol::Tcp, 50_000, "127.0.0.1:8080", 10), 0);
        table.apply(&event(NetEventKind::Send, Protocol::Udp, 50_000, "[::1]:53", 10), 0);
        assert!(table.take(1).open.is_empty());
    }

    #[test]
    fn ipv4_mapped_addresses_match_plain_ipv4() {
        let mut table = FlowTable::new();
        table.apply(&event(NetEventKind::Connect, Protocol::Tcp, 50_000, "[::ffff:203.0.113.5]:443", 0), 0);
        table.apply(&tcp(NetEventKind::Send, 10), 1);
        let reading = table.take(2);
        assert_eq!(reading.open.len(), 1);
        assert_eq!(reading.open[0].sent, 10);
    }

    #[test]
    fn names_are_reported_once_and_kept_for_late_clients() {
        let mut table = FlowTable::new();
        let ips = parse_dns_results("type:  5 github.map.fastly.net;::ffff:140.82.112.3;2606:50c0:8000::153;");
        assert_eq!(ips, vec!["140.82.112.3".parse::<IpAddr>().unwrap(), "2606:50c0:8000::153".parse().unwrap()]);

        table.add_name("GitHub.com.", &ips, 0);
        table.add_name("github.com", &ips, 1);
        assert_eq!(table.take(2).names.len(), 2);
        assert!(table.take(3).names.is_empty());
        assert_eq!(table.all_names().len(), 2);
        assert!(table.all_names().iter().all(|(_, name)| name == "github.com"));
    }

    #[test]
    fn merged_readings_keep_everything_that_ended() {
        let conn = Connection {
            pid: 1,
            protocol: Protocol::Tcp,
            local: SocketAddr::from((ME, 1)),
            remote: "203.0.113.5:443".parse().unwrap(),
        };
        let mut first = Observation { closed: vec![Observed::bare(conn.clone())], ..Observation::default() };
        let second = Observation { source: Source::Detailed, open: vec![Observed::bare(conn)], ..Observation::default() };
        first.merge(second);
        assert_eq!((first.open.len(), first.closed.len(), first.source), (1, 1, Source::Detailed));
    }
}
