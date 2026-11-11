//! The collector's work: network events in, readings out to every window
//! that is listening.

use std::collections::{HashMap, HashSet};
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use portlight_core::wire::{self, Message};
use portlight_core::{FlowTable, Platform, ProcessRecord};
use portlight_platform::etw::{EventSession, Feed};
use portlight_platform::pipe::PipeServer;

/// How often new events are read and new process IDs looked up. Short, so
/// programs that only run for a moment can still be named.
const DRAIN_EVERY: Duration = Duration::from_millis(100);
/// How often a reading goes out to the windows.
const SEND_EVERY: Duration = Duration::from_millis(500);
/// How often TCP flows are checked against the TCP table.
const RECONCILE_EVERY: Duration = Duration::from_secs(2);
/// Events waiting to be read before new ones are dropped.
const EVENT_BACKLOG: usize = 200_000;
/// Readings waiting for a slow window before it is disconnected.
const CLIENT_BACKLOG: usize = 32;

type Clients = Arc<Mutex<Vec<SyncSender<Arc<str>>>>>;

/// Runs until `stop` is set or the event session fails. With `verbose`, prints
/// a line per reading (for `portlight-service console`).
pub fn run(stop: &AtomicBool, verbose: bool) -> io::Result<()> {
    let (events, incoming) = mpsc::sync_channel::<Feed>(EVENT_BACKLOG);
    let session = EventSession::start(move |feed| {
        let _ = events.try_send(feed);
    })?;
    if verbose {
        println!("Reading network events. Press Ctrl+C to stop.");
    }

    let table = Arc::new(Mutex::new(FlowTable::new()));
    let clients: Clients = Arc::default();
    spawn_listener(Arc::clone(&table), Arc::clone(&clients))?;

    let platform = portlight_platform::system();
    let mut processes = ProcessNames::default();
    let mut last_send = Instant::now();
    let mut last_reconcile = Instant::now() - RECONCILE_EVERY;

    while !stop.load(Ordering::SeqCst) {
        thread::sleep(DRAIN_EVERY);
        if !session.is_running() {
            return Err(io::Error::other("the network event session ended"));
        }

        let now = now_ms();
        drain(&incoming, &mut lock(&table), &mut processes, &platform, now);

        if last_reconcile.elapsed() >= RECONCILE_EVERY {
            last_reconcile = Instant::now();
            match platform.tcp_connections() {
                Ok(listed) => lock(&table).reconcile_tcp(&listed, now),
                Err(error) => log(&format!("could not read the TCP table: {error}")),
            }
        }

        if last_send.elapsed() < SEND_EVERY {
            continue;
        }
        last_send = Instant::now();

        let mut observation = lock(&table).take(now);
        let pids: HashSet<u32> =
            observation.open.iter().chain(&observation.closed).map(|o| o.conn.pid).collect();
        observation.processes = pids.iter().filter_map(|pid| processes.get(*pid, &platform)).collect();
        processes.forget_except(&observation.open.iter().map(|o| o.conn.pid).collect());

        if verbose {
            println!(
                "{} open, {} closed, {} names, {} windows listening",
                observation.open.len(),
                observation.closed.len(),
                observation.names.len(),
                lock(&clients).len()
            );
        }
        let line: Arc<str> = Message::Reading { observation }.encode().into();
        // A window that has fallen far behind is dropped; it reconnects and
        // starts afresh rather than miss readings in the middle.
        lock(&clients).retain(|client| match client.try_send(Arc::clone(&line)) {
            Ok(()) => true,
            Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) => false,
        });
    }
    Ok(())
}

fn drain(
    incoming: &Receiver<Feed>,
    table: &mut FlowTable,
    processes: &mut ProcessNames,
    platform: &dyn Platform,
    now: u64,
) {
    for feed in incoming.try_iter() {
        match feed {
            Feed::Net(event) => {
                // Look the program up while it is surely still running.
                processes.get(event.pid, platform);
                table.apply(&event, now);
            }
            Feed::Name { name, ips } => table.add_name(&name, &ips, now),
        }
    }
}

/// Accepts windows on the pipe, each with its own writer thread.
fn spawn_listener(table: Arc<Mutex<FlowTable>>, clients: Clients) -> io::Result<()> {
    thread::Builder::new().name("portlight-pipe".into()).spawn(move || {
        let mut server = PipeServer::new();
        loop {
            let mut writer = match server.accept() {
                Ok(writer) => writer,
                Err(error) => {
                    log(&format!("pipe: {error}"));
                    thread::sleep(Duration::from_secs(1));
                    continue;
                }
            };
            let hello = Message::Hello {
                version: env!("CARGO_PKG_VERSION").to_string(),
                protocol: wire::PROTOCOL,
                names: lock(&table).all_names(),
            };
            let (send, lines) = mpsc::sync_channel::<Arc<str>>(CLIENT_BACKLOG);
            lock(&clients).push(send);
            let _ = thread::Builder::new().name("portlight-pipe-client".into()).spawn(move || {
                if writer.write_all(hello.encode().as_bytes()).is_err() {
                    return;
                }
                for line in lines {
                    if writer.write_all(line.as_bytes()).is_err() {
                        return;
                    }
                }
            });
        }
    })?;
    Ok(())
}

/// Programs behind process IDs, looked up once per process.
#[derive(Default)]
struct ProcessNames {
    known: HashMap<u32, Option<ProcessRecord>>,
}

impl ProcessNames {
    fn get(&mut self, pid: u32, platform: &dyn Platform) -> Option<ProcessRecord> {
        self.known
            .entry(pid)
            .or_insert_with(|| {
                platform.process_info(pid).map(|info| ProcessRecord {
                    pid,
                    path: info.path.to_string_lossy().into_owned(),
                    name: info.display_name,
                })
            })
            .clone()
    }

    /// Process IDs get reused, so forget the ones with nothing open.
    fn forget_except(&mut self, live: &HashSet<u32>) {
        self.known.retain(|pid, _| live.contains(pid));
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// Appends a line to `%ProgramData%\Portlight\collector.log`, since a
/// service has no console. The log is restarted when it passes 1 MB.
pub fn log(message: &str) {
    let dir = std::env::var_os("ProgramData").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"));
    let dir = dir.join("Portlight");
    let path = dir.join("collector.log");
    let _ = fs::create_dir_all(&dir);
    if fs::metadata(&path).is_ok_and(|m| m.len() > 1024 * 1024) {
        let _ = fs::remove_file(&path);
    }
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(file, "{} {message}", now_ms());
    }
    eprintln!("portlight-service: {message}");
}
