//! Console view of Portlight's live feed: which app opens which connection.
//!
//! Usage: `cargo run -p portlight-cli` (Ctrl+C to stop)
//!        `cargo run -p portlight-cli -- --seconds 10` to stop by itself
//!
//! It keeps no list between runs, so every app is "known" from the start and
//! only apps that go online while it is running are flagged as new.

use std::thread::sleep;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use portlight_core::{EventKind, KnownApps, Tracker};

const POLL_EVERY: Duration = Duration::from_millis(500);

fn main() -> std::io::Result<()> {
    let run_for = seconds_arg().map(Duration::from_secs);
    let started = Instant::now();
    let platform = portlight_platform::system();

    // `default()` has no learning period, so new apps are flagged right away.
    let mut tracker = Tracker::new(KnownApps::default());
    tracker.poll(&platform, now_ms())?;
    tracker.trust_all();

    let start = tracker.snapshot(now_ms(), 0);
    println!("Already online: {} apps, {} connections", start.apps.len(), start.connections.len());
    for app in &start.apps {
        println!("  {:<36} {} connection(s) to {} server(s)", app.name, app.connections, app.servers);
    }
    println!("\nWatching for new connections...\n");

    let mut last_seq = 0;
    loop {
        if run_for.is_some_and(|limit| started.elapsed() >= limit) {
            return Ok(());
        }
        sleep(POLL_EVERY);

        let update = tracker.poll(&platform, now_ms())?;
        for app in &update.new_apps {
            println!("! NEW APP ONLINE: {}", app.name);
        }
        for event in tracker.snapshot(now_ms(), last_seq).events {
            let sign = if event.kind == EventKind::Opened { '+' } else { '-' };
            println!("{sign} {:<36} -> {}:{}", event.app_name, event.remote_ip, event.remote_port);
            last_seq = event.seq;
        }
    }
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

fn seconds_arg() -> Option<u64> {
    let args: Vec<String> = std::env::args().collect();
    let at = args.iter().position(|a| a == "--seconds")?;
    args.get(at + 1)?.parse().ok()
}
