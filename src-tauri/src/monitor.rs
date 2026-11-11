//! The background loop: read connections, update the tracker, tell the window.

use std::thread;
use std::time::Duration;

use portlight_core::NewApp;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_notification::NotificationExt;

use crate::dns::Resolver;
use crate::state::{AppState, now_ms};

/// Event the window listens to. Its payload is a `Snapshot`.
pub const TICK_EVENT: &str = "monitor:tick";

const POLL_EVERY: Duration = Duration::from_millis(500);

pub fn spawn(app: AppHandle) {
    thread::Builder::new()
        .name("portlight-monitor".into())
        .spawn(move || run(app))
        .expect("could not start the monitor thread");
}

fn run(app: AppHandle) {
    let platform = portlight_platform::system();
    let resolver = Resolver::spawn(app.clone());
    let state = app.state::<AppState>();
    let mut last_event = 0;

    loop {
        let now = now_ms();

        // Keep the lock only while touching the tracker, never while
        // notifying or talking to the window.
        let result = {
            let mut tracker = state.tracker();
            tracker.update(&platform, now).map(|update| {
                if update.known_changed {
                    state.save_known_apps(&tracker);
                }
                let snapshot = tracker.snapshot(now, last_event);
                last_event = tracker.last_event_seq();
                (update, snapshot)
            })
        };

        match result {
            Ok((update, snapshot)) => {
                for ip in update.new_ips {
                    resolver.request(ip);
                }
                for new_app in &update.new_apps {
                    notify_new_app(&app, new_app);
                }
                let _ = app.emit(TICK_EVENT, &snapshot);
            }
            Err(error) => eprintln!("portlight: could not read connections: {error}"),
        }

        thread::sleep(POLL_EVERY);
    }
}

fn notify_new_app(app: &AppHandle, new_app: &NewApp) {
    let shown = app
        .notification()
        .builder()
        .title("New app online")
        .body(format!("{} just connected to the internet for the first time.", new_app.name))
        .show();
    if let Err(error) = shown {
        eprintln!("portlight: could not show a notification: {error}");
    }
}
