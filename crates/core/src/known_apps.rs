//! The list of apps Portlight has already seen online, kept on disk.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// How long the first run stays quiet while it notes what is already installed.
pub const LEARNING_MS: u64 = 2 * 60 * 1000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct KnownApp {
    pub name: String,
    pub first_seen: u64,
    pub trusted: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct KnownApps {
    /// Until this time (Unix milliseconds) new apps are accepted silently.
    learning_until: u64,
    apps: BTreeMap<String, KnownApp>,
}

impl KnownApps {
    /// A fresh list for a first run: empty, and learning for a while.
    pub fn first_run(now: u64) -> Self {
        Self { learning_until: now + LEARNING_MS, apps: BTreeMap::new() }
    }

    /// Reads the list from disk. A missing or unreadable file counts as a first run.
    pub fn load(path: &Path, now: u64) -> Self {
        fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_else(|| Self::first_run(now))
    }

    /// Writes the list to disk. Writes a temporary file first so a crash
    /// half-way cannot leave a broken list behind.
    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let text = serde_json::to_string_pretty(self).map_err(io::Error::other)?;
        let temp = path.with_extension("json.tmp");
        fs::write(&temp, text)?;
        fs::rename(&temp, path)
    }

    pub fn is_learning(&self, now: u64) -> bool {
        now < self.learning_until
    }

    pub fn learning_seconds_left(&self, now: u64) -> u32 {
        (self.learning_until.saturating_sub(now)).div_ceil(1000) as u32
    }

    pub fn get(&self, id: &str) -> Option<&KnownApp> {
        self.apps.get(id)
    }

    pub fn insert(&mut self, id: String, app: KnownApp) {
        self.apps.insert(id, app);
    }

    /// Marks one app as trusted. Returns true if that changed anything.
    pub fn trust(&mut self, id: &str) -> bool {
        match self.apps.get_mut(id) {
            Some(app) if !app.trusted => {
                app.trusted = true;
                true
            }
            _ => false,
        }
    }

    /// Marks every app as trusted. Returns true if that changed anything.
    pub fn trust_all(&mut self) -> bool {
        let mut changed = false;
        for app in self.apps.values_mut() {
            changed |= !app.trusted;
            app.trusted = true;
        }
        changed
    }

    pub fn len(&self) -> usize {
        self.apps.len()
    }

    pub fn is_empty(&self) -> bool {
        self.apps.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_run_learns_for_two_minutes() {
        let known = KnownApps::first_run(1_000);
        assert!(known.is_learning(1_000));
        assert!(known.is_learning(1_000 + LEARNING_MS - 1));
        assert!(!known.is_learning(1_000 + LEARNING_MS));
        assert_eq!(known.learning_seconds_left(1_000), 120);
        assert_eq!(known.learning_seconds_left(1_000 + LEARNING_MS), 0);
    }

    #[test]
    fn survives_a_round_trip_to_disk() {
        let dir = std::env::temp_dir().join(format!("portlight-test-{}", std::process::id()));
        let path = dir.join("known-apps.json");

        let mut known = KnownApps::first_run(5);
        known.insert(
            "c:\\apps\\a.exe".into(),
            KnownApp { name: "A".into(), first_seen: 5, trusted: false },
        );
        known.save(&path).unwrap();

        assert_eq!(KnownApps::load(&path, 999), known);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn missing_file_is_a_first_run() {
        let known = KnownApps::load(Path::new("definitely/not/here.json"), 10);
        assert!(known.is_empty());
        assert!(known.is_learning(10));
    }

    #[test]
    fn trust_reports_whether_anything_changed() {
        let mut known = KnownApps::default();
        known.insert("a".into(), KnownApp { name: "A".into(), first_seen: 0, trusted: false });
        assert!(known.trust("a"));
        assert!(!known.trust("a"));
        assert!(!known.trust("missing"));
        assert!(!known.trust_all());
    }
}
