//! The list of apps Portlight has already seen online, kept on disk.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// How long the first run stays quiet while it notes what is already installed.
pub const LEARNING_MS: u64 = 2 * 60 * 1000;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct KnownApp {
    pub name: String,
    pub first_seen: u64,
    pub trusted: bool,
    /// Who signed the program, once checked. Lets an update that moves to a
    /// new folder be recognised as the same app.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publisher: Option<String>,
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

    /// Starts a new learning period: apps seen during it are trusted quietly.
    pub fn learn_again(&mut self, now: u64) {
        self.learning_until = now + LEARNING_MS;
    }

    /// Forgets every app and starts learning again, like a first run.
    pub fn forget_all(&mut self, now: u64) {
        *self = Self::first_run(now);
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

    /// A known app signed by the same publisher whose program file has the
    /// same name, e.g. an earlier copy of `Discord.exe` from another folder.
    pub fn same_program(&self, publisher: &str, exe: &str) -> Option<&KnownApp> {
        let exe = exe.to_lowercase();
        self.apps
            .iter()
            .filter(|(_, app)| app.publisher.as_deref() == Some(publisher))
            .find(|(id, _)| file_name(id) == exe)
            .map(|(_, app)| app)
    }

    /// Records who signed an app. Returns true if that changed anything.
    pub fn set_publisher(&mut self, id: &str, publisher: Option<String>) -> bool {
        match self.apps.get_mut(id) {
            Some(app) if app.publisher != publisher => {
                app.publisher = publisher;
                true
            }
            _ => false,
        }
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

/// The file name at the end of an app id (a lower-cased path).
fn file_name(id: &str) -> &str {
    id.rsplit(['\\', '/']).next().unwrap_or(id)
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
            KnownApp { name: "A".into(), first_seen: 5, trusted: false, publisher: Some("Maker".into()) },
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
        known.insert("a".into(), KnownApp { name: "A".into(), first_seen: 0, trusted: false, publisher: None });
        assert!(known.trust("a"));
        assert!(!known.trust("a"));
        assert!(!known.trust("missing"));
        assert!(!known.trust_all());
    }

    #[test]
    fn finds_the_same_program_signed_by_the_same_publisher() {
        let mut known = KnownApps::default();
        let discord = KnownApp { name: "Discord".into(), publisher: Some("Discord Inc.".into()), ..KnownApp::default() };
        known.insert("c:\\users\\me\\discord\\app-1.0.1\\discord.exe".into(), discord);
        assert!(known.same_program("Discord Inc.", "Discord.exe").is_some());
        assert!(known.same_program("Someone Else", "Discord.exe").is_none());
        assert!(known.same_program("Discord Inc.", "Update.exe").is_none());
        // Lists saved before publishers were recorded still load.
        let old: KnownApp = serde_json::from_str(r#"{"name":"A","first_seen":1,"trusted":true}"#).unwrap();
        assert_eq!(old.publisher, None);
    }
}
