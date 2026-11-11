//! The background loop: read connections, update the tracker, tell the window.
//!
//! When the Portlight Collector service is running, readings come from its
//! live network events ("detailed"). Otherwise the loop reads the TCP table
//! itself twice a second ("basic"), and tries the service again every few
//! seconds.

use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use portlight_core::NewApp;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_notification::NotificationExt;

use crate::capture::Capture;
use crate::collector::{self, Drained, Feed};
use crate::dns::Resolver;
use crate::state::{AppState, now_ms};

/// Event the window listens to. Its payload is a `Snapshot`.
pub const TICK_EVENT: &str = "monitor:tick";

const POLL_EVERY: Duration = Duration::from_millis(500);
/// How often to look for the collector while in basic mode.
const RETRY_COLLECTOR_EVERY: Duration = Duration::from_secs(3);

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
    let capture = app.state::<Arc<Capture>>();
    let mut last_event = 0;
    let mut feed: Option<Feed> = None;
    let mut last_try: Option<Instant> = None;

    loop {
        let now = now_ms();

        if feed.is_none() && (state.take_reconnect() || last_try.is_none_or(|at| at.elapsed() >= RETRY_COLLECTOR_EVERY)) {
            last_try = Some(Instant::now());
            feed = open_feed(&app);
        }

        // Keep the lock only while touching the tracker, never while
        // notifying or talking to the window.
        let result = {
            let mut tracker = state.tracker();
            let update = match &feed {
                Some(open) => match open.drain() {
                    Drained::Reading(reading) => Some(Ok(tracker.update(reading, &platform, now))),
                    Drained::Nothing => None,
                    Drained::Ended => {
                        feed = None;
                        state.update_link(|link| link.connected = false);
                        collector::emit_status(&app);
                        Some(tracker.poll(&platform, now))
                    }
                },
                None => Some(tracker.poll(&platform, now)),
            };
            if capture.is_active() {
                capture.refresh(&tracker);
            }
            update.map(|update| {
                update.map(|update| {
                    if update.known_changed {
                        state.save_known_apps(&tracker);
                    }
                    let snapshot = tracker.snapshot(now, last_event);
                    last_event = tracker.last_event_seq();
                    (update, snapshot)
                })
            })
        };

        match result {
            Some(Ok((update, snapshot))) => {
                for ip in update.new_ips {
                    resolver.request(ip);
                }
                if !update.new_apps.is_empty() {
                    notify_new_apps(&app, &update.new_apps);
                }
                let _ = app.emit(TICK_EVENT, &snapshot);
            }
            Some(Err(error)) => eprintln!("portlight: could not read connections: {error}"),
            None => {}
        }

        thread::sleep(POLL_EVERY);
    }
}

/// Connects to the collector if it is running, and records the outcome for
/// the window.
fn open_feed(app: &AppHandle) -> Option<Feed> {
    let state = app.state::<AppState>();
    let before = state.link();
    let (feed, link) = match collector::connect() {
        Ok((feed, hello)) => {
            let mut tracker = state.tracker();
            for (ip, name) in hello.names {
                tracker.set_asked_name(ip, name);
            }
            let outdated = collector::is_outdated(&hello.version);
            let link = collector::Link { connected: true, version: Some(hello.version), outdated, error: None };
            (Some(feed), link)
        }
        Err((version, _)) => {
            // It answered but speaks a different version: it needs updating.
            let outdated = version.is_some();
            (None, collector::Link { connected: false, version, outdated, error: before.error.clone() })
        }
    };
    let changed = link.connected != before.connected || link.version != before.version || link.outdated != before.outdated;
    state.update_link(|current| *current = link);
    if changed {
        collector::emit_status(app);
    }
    feed
}

/// One Windows notification per update, however many apps went online in it,
/// so a burst of new apps does not flood the screen.
fn notify_new_apps(app: &AppHandle, new_apps: &[NewApp]) {
    let (title, body) = notification_text(new_apps);
    let shown = app.notification().builder().title(title).body(body).show();
    if let Err(error) = shown {
        eprintln!("portlight: could not show a notification: {error}");
    }
}

fn notification_text(new_apps: &[NewApp]) -> (String, String) {
    let names: Vec<&str> = new_apps.iter().map(|a| a.name.as_str()).collect();
    match names.as_slice() {
        [one] => ("New app online".into(), format!("{one} just connected to the internet for the first time.")),
        [a, b] => ("2 new apps online".into(), format!("{a} and {b} just connected for the first time.")),
        [a, b, rest @ ..] => (
            format!("{} new apps online", names.len()),
            format!("{a}, {b} and {} more just connected for the first time. Open Portlight to review them.", rest.len()),
        ),
        [] => (String::new(), String::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apps(names: &[&str]) -> Vec<NewApp> {
        names.iter().map(|n| NewApp { id: n.to_lowercase(), name: (*n).into() }).collect()
    }

    #[test]
    fn many_new_apps_make_one_short_notification() {
        assert_eq!(notification_text(&apps(&["Chrome"])).0, "New app online");
        assert_eq!(notification_text(&apps(&["Chrome", "Git"])).1, "Chrome and Git just connected for the first time.");
        let (title, body) = notification_text(&apps(&["Chrome", "Git", "Node", "Steam"]));
        assert_eq!(title, "4 new apps online");
        assert!(body.starts_with("Chrome, Git and 2 more"));
    }
}
