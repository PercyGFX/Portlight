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
}

impl Default for Settings {
    fn default() -> Self {
        Self { auto_update: true }
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
}
