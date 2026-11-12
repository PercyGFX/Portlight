//! AI assistants (MCP): answering `portlight-mcp`'s requests.
//!
//! The window listens on a pipe only this Windows user can open. Each
//! connection carries one request and one reply. Nothing is answered unless
//! the person has turned on Settings › AI assistants, every request is
//! logged for them to see, and starting a capture waits for their OK.

use std::collections::{HashMap, VecDeque};
use std::io::{BufRead, BufReader, Write};
use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};

use portlight_core::assistant::{PIPE_PREFIX, Reply, Request};
use portlight_core::{AppView, Signature, Snapshot};
use serde::Serialize;
use serde_json::{Value, json};
use tauri::{AppHandle, Emitter, Manager};

use crate::capture::{Capture, CaptureStartOptions, CaptureState, CaptureTarget, StopAfter};
use crate::owners;
use crate::state::{AppState, format_time, now_ms};

/// Event: an assistant asks for permission. Payload: `ConsentRequest`.
pub const CONSENT_EVENT: &str = "assistant:consent";
/// Event: a request was handled. Payload: `LogEntry`.
pub const ACTIVITY_EVENT: &str = "assistant:activity";

const MCP_EXE: &str = "portlight-mcp.exe";
const SERVER_NAME: &str = "portlight";
/// How long a permission prompt waits before counting as "no".
const CONSENT_TIMEOUT: Duration = Duration::from_secs(120);
const LOG_SIZE: usize = 300;
/// Requests allowed per minute, so a runaway assistant can't hammer the PC.
const REQUESTS_PER_MINUTE: usize = 120;

// ---------- Shapes the window sees (keep in step with src/lib/types.ts) ----------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogEntry {
    pub id: u64,
    pub time: u64,
    pub client: String,
    pub tool: String,
    /// What was asked, in words, e.g. "Details of Firefox".
    pub summary: String,
    /// "answered" | "error" | "allowed" | "denied"
    pub outcome: String,
    /// Size of the answer sent to the assistant.
    pub bytes: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConsentRequest {
    pub id: u64,
    pub client: String,
    /// e.g. "Start capturing packets from Firefox for 30 seconds"
    pub action: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistantStatus {
    pub enabled: bool,
    /// Full path of portlight-mcp.exe, if it is next to Portlight.
    pub mcp_path: Option<String>,
    pub claude_desktop: ClaudeDesktop,
    /// `claude mcp add ...` for Claude Code.
    pub claude_code_command: Option<String>,
    /// The `claude` command was found, so Portlight can run it for you.
    pub claude_code_installed: bool,
    /// Portlight is in Claude Code's user-wide MCP list, pointing at this copy.
    pub claude_code_configured: bool,
    /// `{ "mcpServers": { "portlight": ... } }` for other apps.
    pub config_snippet: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeDesktop {
    /// Claude Desktop seems to be installed (its settings folder exists).
    pub installed: bool,
    /// Portlight is already in its MCP settings, pointing at this copy.
    pub configured: bool,
    pub config_path: Option<String>,
}

// ---------- State ----------

#[derive(Default)]
pub struct Assistant {
    log: Mutex<VecDeque<LogEntry>>,
    pending: Mutex<HashMap<u64, Sender<bool>>>,
    recent: Mutex<VecDeque<Instant>>,
    next_id: AtomicU64,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|p| p.into_inner())
}

impl Assistant {
    pub fn log(&self) -> Vec<LogEntry> {
        lock(&self.log).iter().rev().cloned().collect()
    }

    pub fn clear_log(&self) {
        lock(&self.log).clear();
    }

    /// The person's answer to a permission prompt.
    pub fn answer(&self, id: u64, allow: bool) {
        if let Some(reply) = lock(&self.pending).remove(&id) {
            let _ = reply.send(allow);
        }
    }

    fn next(&self) -> u64 {
        self.next_id.fetch_add(1, Ordering::SeqCst) + 1
    }

    fn record(&self, app: &AppHandle, entry: LogEntry) {
        let _ = app.emit(ACTIVITY_EVENT, &entry);
        let mut log = lock(&self.log);
        log.push_back(entry);
        while log.len() > LOG_SIZE {
            log.pop_front();
        }
    }

    fn within_rate(&self) -> bool {
        let mut recent = lock(&self.recent);
        while recent.front().is_some_and(|t| t.elapsed() > Duration::from_secs(60)) {
            recent.pop_front();
        }
        recent.push_back(Instant::now());
        recent.len() <= REQUESTS_PER_MINUTE
    }
}

/// Starts listening for `portlight-mcp`.
pub fn spawn(app: AppHandle) {
    let started = thread::Builder::new().name("portlight-assistant".into()).spawn(move || {
        let mut server = match portlight_platform::user_pipe::UserPipeServer::new(PIPE_PREFIX) {
            Ok(server) => server,
            Err(error) => {
                eprintln!("portlight: AI assistant pipe unavailable: {error}");
                return;
            }
        };
        loop {
            match server.accept() {
                Ok(pipe) => {
                    let app = app.clone();
                    thread::spawn(move || serve(&app, pipe));
                }
                Err(error) => {
                    eprintln!("portlight: AI assistant pipe: {error}");
                    thread::sleep(Duration::from_secs(1));
                }
            }
        }
    });
    if let Err(error) = started {
        eprintln!("portlight: could not start the AI assistant thread: {error}");
    }
}

/// One connection: read a request, write the reply.
fn serve(app: &AppHandle, pipe: std::fs::File) {
    let mut line = String::new();
    if BufReader::new(&pipe).read_line(&mut line).is_err() {
        return;
    }
    let reply = match serde_json::from_str::<Request>(&line) {
        Ok(request) => answer(app, &request),
        Err(error) => Reply::Error(format!("Portlight could not read the request: {error}")),
    };
    let mut text = serde_json::to_string(&reply).unwrap_or_default();
    text.push('\n');
    let _ = (&pipe).write_all(text.as_bytes());
}

fn answer(app: &AppHandle, request: &Request) -> Reply {
    let state = app.state::<AppState>();
    let assistant = app.state::<Assistant>();
    if !state.settings().assistant_access {
        return Reply::Error(
            "AI assistant access is turned off in Portlight. Ask the user to turn it on in Portlight: Settings › AI \
             assistants."
                .into(),
        );
    }
    if !assistant.within_rate() {
        return Reply::Error("Too many requests in the last minute. Wait a little and try again.".into());
    }

    let id = assistant.next();
    let summary = describe(request);
    let (reply, outcome) = match run(app, request) {
        Ok((value, outcome)) => (Reply::Ok(value), outcome),
        Err(message) => (Reply::Error(message), "error"),
    };
    let bytes = serde_json::to_string(&reply).map(|s| s.len()).unwrap_or(0);
    assistant.record(
        app,
        LogEntry {
            id,
            time: now_ms(),
            client: request.client.clone(),
            tool: request.tool.clone(),
            summary,
            outcome: outcome.into(),
            bytes,
        },
    );
    reply
}

/// What the request asks for, in words, for the activity log.
fn describe(request: &Request) -> String {
    let arg = |key: &str| request.arguments.get(key).and_then(Value::as_str).map(str::to_string);
    let num = |key: &str| request.arguments.get(key).and_then(Value::as_u64);
    match request.tool.as_str() {
        "portlight_status" => "Overview of this PC".into(),
        "list_apps" => "List of apps".into(),
        "get_app" => format!("Details of {}", arg("app").unwrap_or_default()),
        "list_connections" => match arg("app") {
            Some(app) => format!("Open connections of {app}"),
            None => "Open connections".into(),
        },
        "recent_activity" => format!("Activity of the last {} minutes", num("minutes").unwrap_or(15)),
        "new_apps" => "New apps".into(),
        "lookup_server" => format!("Who owns {}", arg("server").unwrap_or_default()),
        "app_history" => format!("{} days of history for {}", num("days").unwrap_or(7), arg("app").unwrap_or_default()),
        "history_summary" => format!("{} days of history for all apps", num("days").unwrap_or(7)),
        "capture_status" => "Capture status".into(),
        "read_packets" => match arg("filter") {
            Some(filter) if !filter.is_empty() => format!("Captured packets matching \"{filter}\""),
            _ => "Captured packets".into(),
        },
        "explain_packet" => format!("Breakdown of packet {}", num("number").unwrap_or(0)),
        "start_capture" => "Start a packet capture".into(),
        "stop_capture" => "Stop the packet capture".into(),
        other => other.to_string(),
    }
}

type Answer = Result<(Value, &'static str), String>;

fn ok(value: Value) -> Answer {
    Ok((value, "answered"))
}

fn run(app: &AppHandle, request: &Request) -> Answer {
    let state = app.state::<AppState>();
    let capture = app.state::<Arc<Capture>>();
    let args = &request.arguments;
    let offset = portlight_platform::clock::local_utc_offset_minutes();
    let time = |ms: u64| format_time(ms, offset);
    let snapshot = || state.tracker().snapshot(now_ms(), 0);

    match request.tool.as_str() {
        "portlight_status" => {
            let snap = snapshot();
            let status = capture.status();
            let servers: std::collections::HashSet<&str> = snap.connections.iter().map(|c| c.remote_ip.as_str()).collect();
            ok(json!({
                "portlightVersion": env!("CARGO_PKG_VERSION"),
                "monitoring": match snap.source {
                    portlight_core::Source::Detailed => "detailed (every connection, UDP, data used, real site names)",
                    portlight_core::Source::Basic => "basic (TCP only, checked twice a second, no data counts)",
                },
                "appsOnline": snap.apps.iter().filter(|a| a.connections > 0).count(),
                "openConnections": snap.connections.len(),
                "servers": servers.len(),
                "newAppsToReview": snap.apps.iter().filter(|a| !a.trusted).count(),
                "learningSecondsLeft": snap.learning_seconds_left,
                "capture": capture_summary(&capture, &status),
                "historyKeptDays": state.settings().history_enabled.then(|| state.settings().history_keep_days),
                "localTime": time(now_ms()),
            }))
        }
        "list_apps" => {
            let only_online = args.get("only_online").and_then(Value::as_bool).unwrap_or(true);
            let only_new = args.get("only_new").and_then(Value::as_bool).unwrap_or(false);
            let apps: Vec<Value> = snapshot()
                .apps
                .iter()
                .filter(|a| !only_online || a.connections > 0)
                .filter(|a| !only_new || !a.trusted)
                .take(200)
                .map(|a| app_json(a, &time))
                .collect();
            ok(json!({ "count": apps.len(), "apps": apps }))
        }
        "get_app" => {
            let snap = snapshot();
            let app_view = find_app(&snap, args)?;
            let mut value = app_json(app_view, &time);
            let connections: Vec<Value> = snap
                .connections
                .iter()
                .filter(|c| c.app_id == app_view.id)
                .map(|c| connection_json(&snap, c, &time))
                .collect();
            value["openConnections"] = json!(connections);
            ok(value)
        }
        "list_connections" => {
            let snap = snapshot();
            let filter_app = if args.get("app").is_some() { Some(find_app(&snap, args)?.id.clone()) } else { None };
            let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(100).clamp(1, 500) as usize;
            let mut list: Vec<&portlight_core::ConnView> =
                snap.connections.iter().filter(|c| filter_app.as_ref().is_none_or(|id| &c.app_id == id)).collect();
            list.sort_by_key(|c| std::cmp::Reverse(c.bytes_sent + c.bytes_received));
            let total = list.len();
            let rows: Vec<Value> = list.into_iter().take(limit).map(|c| connection_json(&snap, c, &time)).collect();
            ok(json!({ "total": total, "shown": rows.len(), "connections": rows }))
        }
        "recent_activity" => {
            let snap = snapshot();
            let filter_app = if args.get("app").is_some() { Some(find_app(&snap, args)?.id.clone()) } else { None };
            let minutes = args.get("minutes").and_then(Value::as_u64).unwrap_or(15).clamp(1, 1440);
            let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(100).clamp(1, 500) as usize;
            let since = now_ms().saturating_sub(minutes * 60_000);
            let events: Vec<Value> = snap
                .events
                .iter()
                .rev()
                .filter(|e| e.time >= since && filter_app.as_ref().is_none_or(|id| &e.app_id == id))
                .take(limit)
                .map(|e| {
                    let owner = e.remote_ip.parse().ok().and_then(owners::lookup);
                    json!({
                        "time": time(e.time),
                        "event": match e.kind { portlight_core::EventKind::Opened => "opened", portlight_core::EventKind::Closed => "closed" },
                        "app": e.app_name,
                        "protocol": protocol(e.protocol),
                        "server": e.host.clone().unwrap_or_else(|| e.remote_ip.clone()),
                        "address": e.remote_ip,
                        "port": e.remote_port,
                        "owner": owner.as_ref().map(|o| o.name.clone()),
                        "country": owner.and_then(|o| o.country),
                        "dataSent": human_bytes(e.bytes_sent),
                        "dataReceived": human_bytes(e.bytes_received),
                    })
                })
                .collect();
            ok(json!({ "minutes": minutes, "count": events.len(), "events": events, "note": "Portlight keeps the last 2,000 events." }))
        }
        "new_apps" => {
            let apps: Vec<Value> = snapshot().apps.iter().filter(|a| !a.trusted).map(|a| app_json(a, &time)).collect();
            ok(json!({ "count": apps.len(), "apps": apps }))
        }
        "lookup_server" => lookup_server(&state, args),
        "app_history" => {
            let snap = snapshot();
            let app_view = find_app(&snap, args)?;
            if !state.settings().history_enabled {
                return Err("History is turned off in Portlight (Settings › Privacy & data), so there is none to read.".into());
            }
            let days = args.get("days").and_then(Value::as_u64).unwrap_or(7).clamp(1, 90) as u32;
            let mut history = state.history.app_days(&app_view.id, days);
            for day in &mut history {
                day.servers.truncate(20);
            }
            ok(json!({ "app": app_view.name, "days": days, "history": history, "note": "Up to 20 busiest servers per day." }))
        }
        "history_summary" => {
            if !state.settings().history_enabled {
                return Err("History is turned off in Portlight (Settings › Privacy & data), so there is none to read.".into());
            }
            let days = args.get("days").and_then(Value::as_u64).unwrap_or(7).clamp(1, 90) as u32;
            let mut rows = state.history.summary(days);
            rows.truncate(500);
            ok(json!({ "days": days, "rows": rows }))
        }
        "capture_status" => {
            let status = capture.status();
            ok(capture_summary(&capture, &status))
        }
        "read_packets" => {
            let filter = args.get("filter").and_then(Value::as_str).unwrap_or("");
            let start = args.get("start").and_then(Value::as_u64).unwrap_or(0) as usize;
            let count = args.get("count").and_then(Value::as_u64).unwrap_or(50).clamp(1, 200) as usize;
            if capture.status().packets == 0 {
                return Err("There are no captured packets. Use start_capture first.".into());
            }
            let (rows, matched) = capture.rows_matching(filter, start, count)?;
            ok(json!({ "matching": matched, "start": start, "rows": rows }))
        }
        "explain_packet" => {
            let number = args.get("number").and_then(Value::as_u64).ok_or("explain_packet needs a packet number.")?;
            let detail = capture.packet(number as u32).ok_or_else(|| format!("There is no packet number {number} in the capture."))?;
            let layers: Vec<Value> = detail
                .layers
                .iter()
                .map(|layer| {
                    json!({
                        "layer": layer.title,
                        "summary": layer.summary,
                        "whatItIs": layer.explain,
                        "fields": layer.fields.iter().map(field_json).collect::<Vec<_>>(),
                    })
                })
                .collect();
            let mut value = json!({
                "number": detail.number,
                "secondsIntoCapture": detail.time,
                "time": time(detail.timestamp as u64),
                "length": detail.length,
                "networkCard": detail.interface_name,
                "app": detail.app,
                "layers": layers,
            });
            if args.get("include_bytes").and_then(Value::as_bool).unwrap_or(false) {
                value["bytesHex"] = json!(detail.bytes.iter().map(|b| format!("{b:02x}")).collect::<String>());
            }
            ok(value)
        }
        "start_capture" => start_capture(app, request),
        "stop_capture" => {
            let status = capture.stop();
            let _ = app.emit(crate::capture::STATUS_EVENT, &status);
            ok(capture_summary(&capture, &status))
        }
        other => Err(format!("Unknown tool: {other}")),
    }
}

fn start_capture(app: &AppHandle, request: &Request) -> Answer {
    let state = app.state::<AppState>();
    let capture = app.state::<Arc<Capture>>();
    let assistant = app.state::<Assistant>();
    let args = &request.arguments;
    let seconds = args.get("seconds").and_then(Value::as_u64).unwrap_or(30).clamp(5, 300) as u32;
    let packets = args.get("max_packets").and_then(Value::as_u64).unwrap_or(5_000).clamp(10, 20_000) as u32;

    let chosen = [args.get("app").is_some(), args.get("server").is_some(), args.get("everything") == Some(&json!(true))];
    if chosen.iter().filter(|c| **c).count() != 1 {
        return Err("Say what to capture: exactly one of app, server or everything.".into());
    }
    let (target, words) = if args.get("app").is_some() {
        let snap = state.tracker().snapshot(now_ms(), 0);
        let found = find_app(&snap, args)?;
        (CaptureTarget::App { app_id: found.id.clone(), name: found.name.clone() }, format!("packets from {}", found.name))
    } else if let Some(server) = args.get("server").and_then(Value::as_str) {
        let server = server.trim().to_string();
        match server.parse::<IpAddr>() {
            Ok(ip) => {
                let name = state.tracker().name_of(ip).map(str::to_string);
                (CaptureTarget::Host { ip: ip.to_string(), name }, format!("packets to and from {server}"))
            }
            Err(_) => (CaptureTarget::Domain { name: server.to_lowercase() }, format!("packets to and from {server}")),
        }
    } else {
        (CaptureTarget::All, "every packet on this PC's network".to_string())
    };

    let support = capture.support();
    if !support.available {
        return Err(format!(
            "Packet capture isn't available: {} Npcap (free, from https://npcap.com) is needed.",
            support.problem.unwrap_or_default()
        ));
    }

    // Ask the person, in Portlight.
    let id = assistant.next();
    let (send, answer) = mpsc::channel();
    lock(&assistant.pending).insert(id, send);
    let consent = ConsentRequest {
        id,
        client: request.client.clone(),
        action: format!("Record {words} for {seconds} seconds"),
        detail: format!(
            "{} asked for this. It stops by itself after {seconds} seconds or {packets} packets. The assistant can then read \
             the packets' summaries and breakdowns. Encrypted contents stay unreadable.",
            request.client
        ),
    };
    crate::show_main_window(app);
    let _ = app.emit(CONSENT_EVENT, &consent);
    let allowed = answer.recv_timeout(CONSENT_TIMEOUT).unwrap_or(false);
    lock(&assistant.pending).remove(&id);
    if !allowed {
        return Ok((
            json!({ "started": false, "message": "The user did not allow the capture (or didn't answer within 2 minutes)." }),
            "denied",
        ));
    }

    let options = CaptureStartOptions {
        target,
        interfaces: Vec::new(),
        capture_filter: String::new(),
        stop_after: StopAfter { packets: Some(packets), seconds: Some(seconds), megabytes: None },
    };
    let status = capture.start(app, options)?;
    let _ = app.emit(crate::capture::STATUS_EVENT, &status);
    Ok((
        json!({
            "started": true,
            "capturing": words,
            "stopsAfterSeconds": seconds,
            "stopsAfterPackets": packets,
            "next": "Wait, then call capture_status, read_packets and explain_packet.",
        }),
        "allowed",
    ))
}

fn lookup_server(state: &AppState, args: &Value) -> Answer {
    let query = args.get("server").and_then(Value::as_str).ok_or("lookup_server needs a server address or name.")?;
    let query = query.trim().trim_end_matches('.').to_lowercase();
    let tracker = state.tracker();
    let snap = tracker.snapshot(now_ms(), 0);
    let ips: Vec<IpAddr> = match query.parse::<IpAddr>() {
        Ok(ip) => vec![ip],
        Err(_) => {
            let suffix = format!(".{query}");
            tracker
                .names()
                .into_iter()
                .filter(|(_, name)| {
                    let name = name.to_lowercase();
                    name == query || name.ends_with(&suffix)
                })
                .map(|(ip, _)| ip)
                .take(50)
                .collect()
        }
    };
    if ips.is_empty() {
        return Ok((
            json!({ "server": query, "addresses": [], "note": "No app on this PC has looked this name up or connected to it while Portlight was watching." }),
            "answered",
        ));
    }
    let addresses: Vec<Value> = ips
        .iter()
        .map(|ip| {
            let owner = owners::lookup(*ip);
            let apps: Vec<&str> = {
                let mut apps: Vec<&str> = snap
                    .connections
                    .iter()
                    .filter(|c| c.remote_ip == ip.to_string())
                    .filter_map(|c| snap.apps.iter().find(|a| a.id == c.app_id).map(|a| a.name.as_str()))
                    .collect();
                apps.sort_unstable();
                apps.dedup();
                apps
            };
            json!({
                "address": ip.to_string(),
                "name": tracker.name_of(*ip),
                "owner": owner.as_ref().map(|o| o.name.clone()),
                "asn": owner.as_ref().and_then(|o| o.asn),
                "countryOfNetworkRegistration": owner.and_then(|o| o.country),
                "appsConnectedNow": apps,
            })
        })
        .collect();
    Ok((json!({ "server": query, "addresses": addresses }), "answered"))
}

fn capture_summary(capture: &Capture, status: &crate::capture::CaptureStatus) -> Value {
    json!({
        "state": match status.state { CaptureState::Idle => "none", CaptureState::Running => "recording", CaptureState::Stopped => "stopped" },
        "capturing": (status.state != CaptureState::Idle).then(|| capture.target_words()),
        "packets": status.packets,
        "size": human_bytes(status.bytes),
        "dropped": status.dropped,
        "stopReason": status.stop_reason,
    })
}

fn find_app<'a>(snapshot: &'a Snapshot, args: &Value) -> Result<&'a AppView, String> {
    let query = args.get("app").and_then(Value::as_str).ok_or("Say which app (its name or program file).")?;
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return Err("Say which app (its name or program file).".into());
    }
    let apps = &snapshot.apps;
    let exact: Vec<&AppView> = apps
        .iter()
        .filter(|a| a.id == needle || a.name.to_lowercase() == needle || a.exe.to_lowercase() == needle)
        .collect();
    let found = if exact.is_empty() {
        apps.iter().filter(|a| a.name.to_lowercase().contains(&needle) || a.exe.to_lowercase().contains(&needle)).collect()
    } else {
        exact
    };
    match found.as_slice() {
        [one] => Ok(one),
        [] => Err(format!(
            "No app called \"{query}\" has been online since Portlight started. Use list_apps with only_online false to see the names."
        )),
        many => Err(format!(
            "Several apps match \"{query}\": {}. Use the full name.",
            many.iter().map(|a| a.name.as_str()).take(10).collect::<Vec<_>>().join(", ")
        )),
    }
}

fn app_json(app: &AppView, time: &dyn Fn(u64) -> String) -> Value {
    json!({
        "name": app.name,
        "program": app.exe,
        "path": app.path,
        "signedBy": match &app.signature {
            Signature::Signed { publisher } => Value::String(publisher.clone()),
            Signature::Unsigned => Value::String("not signed".into()),
            Signature::Unknown => Value::Null,
        },
        "new": !app.trusted,
        "openConnections": app.connections,
        "servers": app.servers,
        "connectionsOpenedThisSession": app.total_opened,
        "dataSent": human_bytes(app.bytes_sent),
        "dataReceived": human_bytes(app.bytes_received),
        "firstSeen": time(app.first_seen),
    })
}

fn connection_json(snapshot: &Snapshot, c: &portlight_core::ConnView, time: &dyn Fn(u64) -> String) -> Value {
    json!({
        "app": snapshot.apps.iter().find(|a| a.id == c.app_id).map(|a| a.name.clone()),
        "protocol": protocol(c.protocol),
        "server": c.host.clone().unwrap_or_else(|| c.remote_ip.clone()),
        "address": c.remote_ip,
        "port": c.remote_port,
        "owner": c.owner,
        "countryOfNetworkRegistration": c.country,
        "openedAt": time(c.opened_at),
        "dataSent": human_bytes(c.bytes_sent),
        "dataReceived": human_bytes(c.bytes_received),
    })
}

fn field_json(field: &portlight_packets::Field) -> Value {
    let mut value = json!({ "field": field.label, "value": field.value });
    if let Some(explain) = &field.explain {
        value["meaning"] = json!(explain);
    }
    if !field.children.is_empty() {
        value["parts"] = json!(field.children.iter().map(field_json).collect::<Vec<_>>());
    }
    value
}

fn protocol(protocol: portlight_core::Protocol) -> &'static str {
    match protocol {
        portlight_core::Protocol::Tcp => "TCP",
        portlight_core::Protocol::Udp => "UDP",
    }
}

/// "12.4 MB"
pub fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["bytes", "kB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1000.0 && unit < UNITS.len() - 1 {
        value /= 1000.0;
        unit += 1;
    }
    if unit == 0 { format!("{bytes} bytes") } else { format!("{value:.1} {}", UNITS[unit]) }
}

// ---------- Setting up assistant apps ----------

/// portlight-mcp.exe next to this program.
pub fn mcp_path() -> Option<PathBuf> {
    let path = std::env::current_exe().ok()?.parent()?.join(MCP_EXE);
    path.exists().then_some(path)
}

/// Where Claude Desktop keeps its settings: the regular install, or the
/// Microsoft Store one (under its package folder).
fn claude_desktop_config() -> Option<PathBuf> {
    let roaming = std::env::var_os("APPDATA").map(PathBuf::from);
    let local = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
    if let Some(packages) = local.map(|l| l.join("Packages"))
        && let Ok(entries) = std::fs::read_dir(&packages)
    {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            let dir = entry.path().join("LocalCache").join("Roaming").join("Claude");
            if name.starts_with("Claude_") && dir.exists() {
                return Some(dir.join("claude_desktop_config.json"));
            }
        }
    }
    let dir = roaming?.join("Claude");
    dir.exists().then(|| dir.join("claude_desktop_config.json"))
}

fn server_entry(path: &std::path::Path) -> Value {
    json!({ "command": path.display().to_string(), "args": [] })
}

pub fn status(app: &AppHandle) -> AssistantStatus {
    let enabled = app.state::<AppState>().settings().assistant_access;
    let path = mcp_path();
    let config = claude_desktop_config();
    let configured = match (&config, &path) {
        (Some(config), Some(path)) => std::fs::read_to_string(config)
            .ok()
            .and_then(|text| serde_json::from_str::<Value>(&text).ok())
            .is_some_and(|value| value["mcpServers"][SERVER_NAME]["command"] == json!(path.display().to_string())),
        _ => false,
    };
    AssistantStatus {
        enabled,
        mcp_path: path.as_ref().map(|p| p.display().to_string()),
        claude_desktop: ClaudeDesktop {
            installed: config.is_some(),
            configured,
            config_path: config.map(|p| p.display().to_string()),
        },
        claude_code_command: path
            .as_ref()
            .map(|p| format!("claude mcp add --scope user {SERVER_NAME} -- \"{}\"", p.display())),
        claude_code_installed: claude_command().is_some(),
        claude_code_configured: path.as_ref().is_some_and(|p| claude_code_points_at(p)),
        config_snippet: path.as_ref().map(|p| {
            serde_json::to_string_pretty(&json!({ "mcpServers": { SERVER_NAME: server_entry(p) } })).unwrap_or_default()
        }),
    }
}

/// Adds Portlight to Claude Desktop's MCP settings, keeping everything else
/// in the file. A copy of the old file is kept next to it.
pub fn add_to_claude_desktop() -> Result<(), String> {
    let path = mcp_path().ok_or("portlight-mcp.exe isn't next to Portlight, so there's nothing to add.")?;
    let config = claude_desktop_config().ok_or("Claude Desktop doesn't seem to be installed for this Windows user.")?;
    let mut value = match std::fs::read_to_string(&config) {
        Ok(text) if !text.trim().is_empty() => serde_json::from_str::<Value>(&text).map_err(|e| {
            format!("Claude Desktop's settings file isn't valid JSON, so Portlight left it alone ({e}). Fix or remove it, then try again.")
        })?,
        _ => json!({}),
    };
    if !value.is_object() {
        return Err("Claude Desktop's settings file has an unexpected shape, so Portlight left it alone.".into());
    }
    if !value["mcpServers"].is_object() {
        value["mcpServers"] = json!({});
    }
    value["mcpServers"][SERVER_NAME] = server_entry(&path);
    if config.exists() {
        let _ = std::fs::copy(&config, config.with_extension("json.portlight-backup"));
    }
    let text = serde_json::to_string_pretty(&value).map_err(|e| e.to_string())?;
    std::fs::write(&config, text).map_err(|e| format!("Could not write {}: {e}", config.display()))
}

/// Takes Portlight out of Claude Desktop's MCP settings.
pub fn remove_from_claude_desktop() -> Result<(), String> {
    let Some(config) = claude_desktop_config() else { return Ok(()) };
    let Ok(text) = std::fs::read_to_string(&config) else { return Ok(()) };
    let mut value: Value = serde_json::from_str(&text).map_err(|e| format!("Claude Desktop's settings file isn't valid JSON ({e})."))?;
    if let Some(servers) = value.get_mut("mcpServers").and_then(Value::as_object_mut)
        && servers.remove(SERVER_NAME).is_some()
    {
        let text = serde_json::to_string_pretty(&value).map_err(|e| e.to_string())?;
        std::fs::write(&config, text).map_err(|e| format!("Could not write {}: {e}", config.display()))?;
    }
    Ok(())
}

/// True when Claude Code's user settings (`~/.claude.json`, where `claude mcp
/// add --scope user` writes) already start this copy of portlight-mcp.
fn claude_code_points_at(path: &std::path::Path) -> bool {
    let Some(home) = std::env::var_os("USERPROFILE") else { return false };
    let Ok(text) = std::fs::read_to_string(PathBuf::from(home).join(".claude.json")) else { return false };
    let Ok(value) = serde_json::from_str::<Value>(&text) else { return false };
    let command = &value["mcpServers"][SERVER_NAME]["command"];
    command.as_str().is_some_and(|c| std::path::Path::new(c) == path)
}

/// The Claude Code command line program, if it is on the PATH.
fn claude_command() -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .flat_map(|dir| ["claude.exe", "claude.cmd"].map(|name| dir.join(name)))
        .find(|candidate| candidate.is_file())
}

/// Runs `claude mcp ...` and returns a readable error if it fails.
fn run_claude(args: &[&str]) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let claude = claude_command().ok_or("Claude Code isn't installed (the claude command wasn't found).")?;
    let output = std::process::Command::new(claude)
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("Could not run Claude Code: {e}"))?;
    if output.status.success() {
        Ok(())
    } else {
        let said = String::from_utf8_lossy(&output.stderr);
        let said = if said.trim().is_empty() { String::from_utf8_lossy(&output.stdout) } else { said };
        Err(format!("Claude Code said: {}", said.trim()))
    }
}

/// Adds Portlight to Claude Code for every project of this user. Replaces
/// an earlier entry, e.g. one pointing at an older copy.
pub fn add_to_claude_code() -> Result<(), String> {
    let path = mcp_path().ok_or("portlight-mcp.exe isn't next to Portlight, so there's nothing to add.")?;
    let _ = run_claude(&["mcp", "remove", "--scope", "user", SERVER_NAME]);
    run_claude(&["mcp", "add", "--scope", "user", SERVER_NAME, "--", &path.display().to_string()])
}

/// Takes Portlight out of Claude Code.
pub fn remove_from_claude_code() -> Result<(), String> {
    run_claude(&["mcp", "remove", "--scope", "user", SERVER_NAME])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_read_naturally() {
        assert_eq!(human_bytes(512), "512 bytes");
        assert_eq!(human_bytes(12_400_000), "12.4 MB");
        assert_eq!(human_bytes(1_500), "1.5 kB");
    }

    #[test]
    fn requests_are_described_in_words_for_the_log() {
        let request = |tool: &str, arguments: Value| Request { client: "Claude".into(), tool: tool.into(), arguments };
        assert_eq!(describe(&request("get_app", json!({ "app": "Firefox" }))), "Details of Firefox");
        assert_eq!(describe(&request("read_packets", json!({ "filter": "dns" }))), "Captured packets matching \"dns\"");
        assert_eq!(describe(&request("app_history", json!({ "app": "Steam", "days": 3 }))), "3 days of history for Steam");
    }
}
