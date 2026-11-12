//! Turns remote addresses into names in the background.
//!
//! This is a reverse lookup: it asks your DNS server which name an address
//! belongs to. The answer is the server's own name, which is often less
//! friendly than the site you visited (a CDN host instead of "github.com").
//! Reading the names apps actually asked for is on the roadmap.

use std::net::IpAddr;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;

use tauri::{AppHandle, Manager};

use crate::state::AppState;

/// Lookups can each take seconds, so a few run side by side.
const WORKERS: usize = 4;

pub struct Resolver {
    queue: Sender<IpAddr>,
}

impl Resolver {
    pub fn spawn(app: AppHandle) -> Self {
        let (queue, jobs) = mpsc::channel::<IpAddr>();
        let jobs = Arc::new(Mutex::new(jobs));
        for n in 0..WORKERS {
            let (app, jobs) = (app.clone(), Arc::clone(&jobs));
            let started = thread::Builder::new().name(format!("portlight-dns-{n}")).spawn(move || work(app, jobs));
            if let Err(error) = started {
                eprintln!("portlight: could not start a name-lookup thread: {error}");
            }
        }
        Self { queue }
    }

    /// Queues an address for lookup. The name shows up on a later tick.
    pub fn request(&self, ip: IpAddr) {
        let _ = self.queue.send(ip);
    }
}

fn work(app: AppHandle, jobs: Arc<Mutex<Receiver<IpAddr>>>) {
    loop {
        // Hold the lock only to take one job, so the other workers stay busy.
        let job = jobs.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).recv();
        let Ok(ip) = job else { return };
        if !app.state::<AppState>().settings().reverse_lookups {
            continue;
        }

        match dns_lookup::lookup_addr(&ip) {
            // With no name on record the lookup hands the address back as text.
            Ok(name) if name != ip.to_string() && !name.is_empty() => {
                app.state::<AppState>().tracker().set_host(ip, name);
            }
            _ => {}
        }
    }
}
