//! The history: which app talked to which server on which day, how often and
//! how much data. Kept in SQLite at `history.sqlite` in Portlight's data
//! folder. Only counts are stored, never packet contents.
//!
//! Activity arrives every tick from the monitor loop, is added up in memory
//! and written every 15 seconds.

use std::collections::HashMap;
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use portlight_core::{Activity, Protocol};
use rusqlite::{Connection, params};
use serde::Serialize;

use crate::owners;
use crate::state::{format_time, now_ms};

const WRITE_EVERY: Duration = Duration::from_secs(15);
const PRUNE_EVERY: Duration = Duration::from_secs(6 * 60 * 60);
/// Servers listed per day in an app's history.
const SERVERS_PER_DAY: usize = 200;

// Keep in step with src/lib/types.ts.

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HistoryServer {
    pub name: String,
    pub ip: String,
    pub port: u16,
    pub protocol: Protocol,
    pub connections: u64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub owner: Option<String>,
    pub country: Option<String>,
    pub first_seen: u64,
    pub last_seen: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HistoryDay {
    pub day: String,
    pub connections: u64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub servers: Vec<HistoryServer>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HistoryAppDay {
    pub day: String,
    pub app_id: String,
    pub app_name: String,
    pub connections: u64,
    pub servers: u64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HistorySettings {
    pub enabled: bool,
    pub keep_days: u32,
    pub size_bytes: u64,
    pub oldest_day: Option<String>,
}

#[derive(Hash, PartialEq, Eq, Clone)]
struct Key {
    day: String,
    app_id: String,
    ip: String,
    port: u16,
    protocol: Protocol,
}

struct Totals {
    app_name: String,
    host: Option<String>,
    connections: u64,
    sent: u64,
    received: u64,
    first_seen: u64,
    last_seen: u64,
}

struct Inner {
    db: Option<Connection>,
    pending: HashMap<Key, Totals>,
    written_at: Instant,
    pruned_at: Option<Instant>,
}

pub struct History {
    path: PathBuf,
    inner: Mutex<Inner>,
}

fn protocol_text(protocol: Protocol) -> &'static str {
    match protocol {
        Protocol::Tcp => "tcp",
        Protocol::Udp => "udp",
    }
}

fn protocol_from(text: &str) -> Protocol {
    if text == "udp" { Protocol::Udp } else { Protocol::Tcp }
}

/// The local calendar day of a time, e.g. "2026-11-12".
fn day_of(ms: u64, utc_offset_minutes: i32) -> String {
    format_time(ms, utc_offset_minutes)[..10].to_string()
}

fn open(path: &Path) -> rusqlite::Result<Connection> {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let db = Connection::open(path)?;
    db.busy_timeout(Duration::from_secs(2))?;
    db.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA synchronous = NORMAL;
         CREATE TABLE IF NOT EXISTS daily (
             day TEXT NOT NULL,
             app_id TEXT NOT NULL,
             app_name TEXT NOT NULL,
             ip TEXT NOT NULL,
             port INTEGER NOT NULL,
             protocol TEXT NOT NULL,
             host TEXT,
             connections INTEGER NOT NULL DEFAULT 0,
             bytes_sent INTEGER NOT NULL DEFAULT 0,
             bytes_received INTEGER NOT NULL DEFAULT 0,
             first_seen INTEGER NOT NULL,
             last_seen INTEGER NOT NULL,
             PRIMARY KEY (day, app_id, ip, port, protocol)
         ) WITHOUT ROWID;
         CREATE INDEX IF NOT EXISTS daily_by_app ON daily (app_id, day);
         PRAGMA user_version = 1;",
    )?;
    Ok(db)
}

impl History {
    /// Opens (or creates) the history file. A file that can't be opened
    /// leaves history off for this run rather than stopping Portlight.
    pub fn open(path: PathBuf) -> History {
        let db = match open(&path) {
            Ok(db) => Some(db),
            Err(error) => {
                eprintln!("portlight: history is unavailable ({}): {error}", path.display());
                None
            }
        };
        History {
            path,
            inner: Mutex::new(Inner { db, pending: HashMap::new(), written_at: Instant::now(), pruned_at: None }),
        }
    }

    fn inner(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Adds one tick's activity. Writes to disk every so often.
    pub fn record(&self, activity: &[Activity], keep_days: u32) {
        let offset = portlight_platform::clock::local_utc_offset_minutes();
        let mut inner = self.inner();
        for item in activity {
            let key = Key {
                day: day_of(item.time, offset),
                app_id: item.app_id.clone(),
                ip: item.remote_ip.to_string(),
                port: item.remote_port,
                protocol: item.protocol,
            };
            let totals = inner.pending.entry(key).or_insert_with(|| Totals {
                app_name: item.app_name.clone(),
                host: None,
                connections: 0,
                sent: 0,
                received: 0,
                first_seen: item.time,
                last_seen: item.time,
            });
            totals.app_name.clone_from(&item.app_name);
            if item.host.is_some() {
                totals.host.clone_from(&item.host);
            }
            totals.connections += u64::from(item.opened);
            totals.sent += item.sent;
            totals.received += item.received;
            totals.first_seen = totals.first_seen.min(item.time);
            totals.last_seen = totals.last_seen.max(item.time);
        }
        if inner.written_at.elapsed() >= WRITE_EVERY || inner.pending.len() > 5_000 {
            write(&mut inner);
        }
        if inner.pruned_at.is_none_or(|at| at.elapsed() >= PRUNE_EVERY) {
            prune(&mut inner, keep_days, offset);
        }
    }

    /// Writes what is waiting, e.g. when Portlight quits.
    pub fn flush(&self) {
        write(&mut self.inner());
    }

    /// One app's days, newest first, up to `days` days back.
    pub fn app_days(&self, app_id: &str, days: u32) -> Vec<HistoryDay> {
        let offset = portlight_platform::clock::local_utc_offset_minutes();
        let since = day_of(now_ms().saturating_sub(u64::from(days.max(1) - 1) * 86_400_000), offset);
        let mut inner = self.inner();
        write(&mut inner);
        let Some(db) = &inner.db else { return Vec::new() };

        let rows = db
            .prepare_cached(
                "SELECT day, ip, port, protocol, host, connections, bytes_sent, bytes_received, first_seen, last_seen
                 FROM daily WHERE app_id = ?1 AND day >= ?2
                 ORDER BY day DESC, connections DESC, bytes_sent + bytes_received DESC",
            )
            .and_then(|mut statement| {
                statement
                    .query_map(params![app_id, since], |row| {
                        let ip: String = row.get(1)?;
                        let host: Option<String> = row.get(4)?;
                        Ok((
                            row.get::<_, String>(0)?,
                            HistoryServer {
                                name: host.unwrap_or_else(|| ip.clone()),
                                port: row.get(2)?,
                                protocol: protocol_from(&row.get::<_, String>(3)?),
                                connections: row.get(5)?,
                                bytes_sent: row.get(6)?,
                                bytes_received: row.get(7)?,
                                owner: None,
                                country: None,
                                first_seen: row.get(8)?,
                                last_seen: row.get(9)?,
                                ip,
                            },
                        ))
                    })?
                    .collect::<rusqlite::Result<Vec<_>>>()
            })
            .unwrap_or_default();

        let mut out: Vec<HistoryDay> = Vec::new();
        for (day, mut server) in rows {
            if out.last().is_none_or(|last| last.day != day) {
                out.push(HistoryDay { day, connections: 0, bytes_sent: 0, bytes_received: 0, servers: Vec::new() });
            }
            let entry = out.last_mut().expect("just pushed");
            entry.connections += server.connections;
            entry.bytes_sent += server.bytes_sent;
            entry.bytes_received += server.bytes_received;
            if entry.servers.len() < SERVERS_PER_DAY {
                if let Ok(ip) = server.ip.parse::<IpAddr>()
                    && let Some(owner) = owners::lookup(ip)
                {
                    server.owner = Some(owner.name);
                    server.country = owner.country;
                }
                entry.servers.push(server);
            }
        }
        out
    }

    /// Every app's totals per day, newest first.
    pub fn summary(&self, days: u32) -> Vec<HistoryAppDay> {
        let offset = portlight_platform::clock::local_utc_offset_minutes();
        let since = day_of(now_ms().saturating_sub(u64::from(days.max(1) - 1) * 86_400_000), offset);
        let mut inner = self.inner();
        write(&mut inner);
        let Some(db) = &inner.db else { return Vec::new() };
        db.prepare_cached(
            "SELECT day, app_id, MAX(app_name), SUM(connections), COUNT(DISTINCT ip), SUM(bytes_sent), SUM(bytes_received)
             FROM daily WHERE day >= ?1 GROUP BY day, app_id
             ORDER BY day DESC, SUM(connections) DESC",
        )
        .and_then(|mut statement| {
            statement
                .query_map(params![since], |row| {
                    Ok(HistoryAppDay {
                        day: row.get(0)?,
                        app_id: row.get(1)?,
                        app_name: row.get(2)?,
                        connections: row.get(3)?,
                        servers: row.get(4)?,
                        bytes_sent: row.get(5)?,
                        bytes_received: row.get(6)?,
                    })
                })?
                .collect()
        })
        .unwrap_or_default()
    }

    pub fn settings(&self, enabled: bool, keep_days: u32) -> HistorySettings {
        let inner = self.inner();
        let oldest_day = inner
            .db
            .as_ref()
            .and_then(|db| db.query_row("SELECT MIN(day) FROM daily", [], |row| row.get::<_, Option<String>>(0)).ok())
            .flatten();
        let size = |path: PathBuf| std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        let wal = PathBuf::from(format!("{}-wal", self.path.display()));
        HistorySettings { enabled, keep_days, size_bytes: size(self.path.clone()) + size(wal), oldest_day }
    }

    /// Removes days older than `keep_days`, now.
    pub fn prune_now(&self, keep_days: u32) {
        prune(&mut self.inner(), keep_days, portlight_platform::clock::local_utc_offset_minutes());
    }

    /// Deletes everything.
    pub fn clear(&self) {
        let mut inner = self.inner();
        inner.pending.clear();
        if let Some(db) = &inner.db {
            let _ = db.execute_batch("DELETE FROM daily; PRAGMA wal_checkpoint(TRUNCATE); VACUUM;");
        }
    }

    /// Forgets activity that hasn't been written yet, e.g. when history is
    /// turned off.
    pub fn drop_pending(&self) {
        self.inner().pending.clear();
    }
}

fn write(inner: &mut Inner) {
    inner.written_at = Instant::now();
    if inner.pending.is_empty() {
        return;
    }
    let pending = std::mem::take(&mut inner.pending);
    let Some(db) = &mut inner.db else { return };
    let result = (|| -> rusqlite::Result<()> {
        let tx = db.transaction()?;
        {
            let mut upsert = tx.prepare_cached(
                "INSERT INTO daily (day, app_id, app_name, ip, port, protocol, host, connections, bytes_sent,
                                    bytes_received, first_seen, last_seen)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
                 ON CONFLICT (day, app_id, ip, port, protocol) DO UPDATE SET
                     app_name = excluded.app_name,
                     host = COALESCE(excluded.host, host),
                     connections = connections + excluded.connections,
                     bytes_sent = bytes_sent + excluded.bytes_sent,
                     bytes_received = bytes_received + excluded.bytes_received,
                     first_seen = MIN(first_seen, excluded.first_seen),
                     last_seen = MAX(last_seen, excluded.last_seen)",
            )?;
            for (key, t) in &pending {
                upsert.execute(params![
                    key.day,
                    key.app_id,
                    t.app_name,
                    key.ip,
                    key.port,
                    protocol_text(key.protocol),
                    t.host,
                    t.connections,
                    t.sent,
                    t.received,
                    t.first_seen,
                    t.last_seen
                ])?;
            }
        }
        tx.commit()
    })();
    if let Err(error) = result {
        eprintln!("portlight: could not write history: {error}");
    }
}

fn prune(inner: &mut Inner, keep_days: u32, offset: i32) {
    inner.pruned_at = Some(Instant::now());
    let cutoff = day_of(now_ms().saturating_sub(u64::from(keep_days.max(1)) * 86_400_000), offset);
    if let Some(db) = &inner.db {
        let _ = db.execute("DELETE FROM daily WHERE day <= ?1", params![cutoff]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn activity(app: &str, ip: &str, opened: u32, sent: u64, time: u64) -> Activity {
        Activity {
            app_id: format!("c:\\{app}.exe"),
            app_name: app.to_string(),
            protocol: Protocol::Tcp,
            remote_ip: ip.parse().unwrap(),
            remote_port: 443,
            host: Some(format!("{ip}.example")),
            opened,
            sent,
            received: sent * 10,
            time,
        }
    }

    fn temp_history(name: &str) -> (History, PathBuf) {
        let dir = std::env::temp_dir().join(format!("portlight-history-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        (History::open(dir.join("history.sqlite")), dir)
    }

    #[test]
    fn adds_up_activity_per_app_server_and_day() {
        let (history, dir) = temp_history("totals");
        let now = now_ms();
        history.record(&[activity("chrome", "203.0.113.5", 1, 100, now), activity("chrome", "203.0.113.5", 1, 50, now + 1)], 90);
        history.record(&[activity("chrome", "203.0.113.6", 1, 10, now + 2), activity("steam", "198.51.100.1", 2, 0, now)], 90);

        let days = history.app_days("c:\\chrome.exe", 7);
        assert_eq!(days.len(), 1);
        let today = &days[0];
        assert_eq!((today.connections, today.bytes_sent, today.bytes_received), (3, 160, 1_600));
        assert_eq!(today.servers.len(), 2);
        // Busiest first, named by host.
        assert_eq!(today.servers[0].name, "203.0.113.5.example");
        assert_eq!((today.servers[0].connections, today.servers[0].first_seen, today.servers[0].last_seen), (2, now, now + 1));

        let summary = history.summary(7);
        assert_eq!(summary.len(), 2);
        assert_eq!((summary[0].app_name.as_str(), summary[0].connections, summary[0].servers), ("chrome", 3, 2));

        let settings = history.settings(true, 90);
        assert!(settings.size_bytes > 0);
        assert!(settings.oldest_day.is_some());

        history.clear();
        assert!(history.app_days("c:\\chrome.exe", 7).is_empty());
        drop(history);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn old_days_are_pruned_and_survive_a_restart_otherwise() {
        let (history, dir) = temp_history("prune");
        let now = now_ms();
        let long_ago = now - 40 * 86_400_000;
        history.record(&[activity("chrome", "203.0.113.5", 1, 1, long_ago), activity("chrome", "203.0.113.5", 1, 1, now)], 365);
        history.flush();
        assert_eq!(history.app_days("c:\\chrome.exe", 60).len(), 2);
        assert_eq!(history.app_days("c:\\chrome.exe", 7).len(), 1);

        history.prune_now(30);
        drop(history);
        let reopened = History::open(dir.join("history.sqlite"));
        assert_eq!(reopened.app_days("c:\\chrome.exe", 60).len(), 1);
        drop(reopened);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
