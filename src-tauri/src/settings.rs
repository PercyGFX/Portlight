//! User settings, kept as a small JSON file next to the known-apps list.

use std::fs;
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    /// Install new versions quietly. When off, the window offers the update.
    pub auto_update: bool,
    /// Keep a daily history of which app talked to which server.
    pub history_enabled: bool,
    /// Days of history to keep.
    pub history_keep_days: u32,
    /// Closing the window keeps Portlight running in the tray. When off,
    /// closing the window quits.
    pub close_to_tray: bool,
    /// Show a Windows notification when a new app goes online.
    pub notify_new_apps: bool,
    /// Ask the DNS server for the names of server addresses. When off,
    /// Portlight makes no DNS queries of its own.
    pub reverse_lookups: bool,
    /// Let AI assistants (through portlight-mcp) read what Portlight knows.
    /// Off until the person turns it on.
    pub assistant_access: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            auto_update: true,
            history_enabled: true,
            history_keep_days: 90,
            close_to_tray: true,
            notify_new_apps: true,
            reverse_lookups: true,
            assistant_access: false,
        }
    }
}

impl Settings {
    /// Reads the settings. A missing or unreadable file means the defaults.
    pub fn load(path: &Path) -> Self {
        fs::read_to_string(path).ok().and_then(|text| serde_json::from_str(&text).ok()).unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        fs::write(path, serde_json::to_string_pretty(self).map_err(io::Error::other)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn updates_are_automatic_unless_turned_off() {
        assert!(Settings::default().auto_update);
        assert!(serde_json::from_str::<Settings>("{}").unwrap().auto_update);
        assert!(!serde_json::from_str::<Settings>(r#"{"autoUpdate":false}"#).unwrap().auto_update);
    }

    #[test]
    fn ai_assistants_are_off_until_turned_on() {
        assert!(!Settings::default().assistant_access);
        assert!(!serde_json::from_str::<Settings>("{}").unwrap().assistant_access);
    }

    #[test]
    fn history_is_on_for_90_days_unless_changed() {
        let old: Settings = serde_json::from_str(r#"{"autoUpdate":true}"#).unwrap();
        assert!(old.history_enabled);
        assert_eq!(old.history_keep_days, 90);
    }
}
