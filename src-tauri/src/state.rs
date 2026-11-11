//! State shared between the background threads and the window's commands.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

use portlight_core::{KnownApps, Tracker};

use crate::collector::Link;
use crate::settings::Settings;
use crate::update::Release;

pub struct AppState {
    tracker: Mutex<Tracker>,
    settings: Mutex<Settings>,
    /// The newer release the last update check found, if any.
    available_update: Mutex<Option<Release>>,
    /// The connection to the Portlight Collector service.
    link: Mutex<Link>,
    /// Asks the monitor loop to look for the collector right away.
    reconnect: AtomicBool,
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
            link: Mutex::new(Link::default()),
            reconnect: AtomicBool::new(false),
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

    pub fn link(&self) -> Link {
        lock(&self.link).clone()
    }

    pub fn update_link(&self, change: impl FnOnce(&mut Link)) {
        change(&mut lock(&self.link));
    }

    /// Asks the monitor loop to look for the collector on its next tick.
    pub fn reconnect_now(&self) {
        self.reconnect.store(true, Ordering::SeqCst);
    }

    /// True once after `reconnect_now`.
    pub fn take_reconnect(&self) -> bool {
        self.reconnect.swap(false, Ordering::SeqCst)
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// The current time as Unix milliseconds.
pub fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// Today's date as YYYY-MM-DD (UTC), for file names.
pub fn today() -> String {
    let (year, month, day) = civil_date((now_ms() / 86_400_000) as i64);
    format!("{year:04}-{month:02}-{day:02}")
}

/// A Unix-milliseconds time as "2026-11-12 14:03:27", shifted into the
/// person's time zone (`utc_offset_minutes`, east of UTC is positive).
pub fn format_time(ms: u64, utc_offset_minutes: i32) -> String {
    let local = ms as i64 + i64::from(utc_offset_minutes) * 60_000;
    let (year, month, day) = civil_date(local.div_euclid(86_400_000));
    let seconds = local.rem_euclid(86_400_000) / 1000;
    format!("{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02}", seconds / 3600, seconds / 60 % 60, seconds % 60)
}

/// Days since 1970 to (year, month, day), Howard Hinnant's algorithm.
fn civil_date(days_since_1970: i64) -> (i64, i64, i64) {
    let days = days_since_1970 + 719_468;
    let era = days.div_euclid(146_097);
    let day_of_era = days.rem_euclid(146_097);
    let year_of_era = (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let mp = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times_read_as_dates_in_the_persons_time_zone() {
        // 2026-11-12 00:00:00 UTC
        let midnight = 1_794_441_600_000;
        assert_eq!(format_time(midnight, 0), "2026-11-12 00:00:00");
        assert_eq!(format_time(midnight + 3_723_000, 330), "2026-11-12 06:32:03");
        assert_eq!(format_time(midnight, -60), "2026-11-11 23:00:00");
        assert_eq!(civil_date(0), (1970, 1, 1));
    }
}
