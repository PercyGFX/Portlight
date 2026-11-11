//! State shared between the background threads and the window's commands.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

use portlight_core::{KnownApps, Tracker};

use crate::settings::Settings;
use crate::update::Release;

pub struct AppState {
    tracker: Mutex<Tracker>,
    settings: Mutex<Settings>,
    /// The newer release the last update check found, if any.
    available_update: Mutex<Option<Release>>,
    known_apps_file: PathBuf,
    settings_file: PathBuf,
}

impl AppState {
    /// Loads everything Portlight keeps on disk from its data folder. Missing
    /// files mean a first run.
    pub fn load(data_dir: &Path) -> Self {
        let known_apps_file = data_dir.join("known-apps.json");
        let settings_file = data_dir.join("settings.json");
        Self {
            tracker: Mutex::new(Tracker::new(KnownApps::load(&known_apps_file, now_ms()))),
            settings: Mutex::new(Settings::load(&settings_file)),
            available_update: Mutex::new(None),
            known_apps_file,
            settings_file,
        }
    }

    /// Locks the tracker. A panic on another thread must not take the whole
    /// app down, so a poisoned lock is used as it is.
    pub fn tracker(&self) -> MutexGuard<'_, Tracker> {
        lock(&self.tracker)
    }

    /// Writes the known-apps list to disk. Failing to save is not fatal: the
    /// worst case is an alert repeating after a restart.
    pub fn save_known_apps(&self, tracker: &Tracker) {
        if let Err(error) = tracker.known().save(&self.known_apps_file) {
            eprintln!("portlight: could not save {}: {error}", self.known_apps_file.display());
        }
    }

    pub fn settings(&self) -> Settings {
        *lock(&self.settings)
    }

    /// Changes the settings and saves them.
    pub fn update_settings(&self, change: impl FnOnce(&mut Settings)) {
        let mut settings = lock(&self.settings);
        change(&mut settings);
        if let Err(error) = settings.save(&self.settings_file) {
            eprintln!("portlight: could not save {}: {error}", self.settings_file.display());
        }
    }

    pub fn available_update(&self) -> Option<Release> {
        lock(&self.available_update).clone()
    }

    pub fn set_available_update(&self, release: Option<Release>) {
        *lock(&self.available_update) = release;
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// The current time as Unix milliseconds.
pub fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}
