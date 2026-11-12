//! App identity in the background: checking who signed each program, and
//! turning program icons into images the window can show.

use std::collections::{HashMap, HashSet};
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::mpsc::{self, Sender};
use std::thread;

use base64::Engine;
use tauri::{AppHandle, Manager};

use crate::state::AppState;

/// Checks signatures one at a time on its own thread, so the monitor loop
/// never waits for one.
pub struct SignatureChecker {
    queue: Sender<(String, PathBuf)>,
}

impl SignatureChecker {
    pub fn spawn(app: AppHandle) -> Self {
        let (queue, jobs) = mpsc::channel::<(String, PathBuf)>();
        let started = thread::Builder::new().name("portlight-signatures".into()).spawn(move || {
            let mut checked = HashSet::new();
            for (app_id, path) in jobs {
                if !checked.insert(app_id.clone()) {
                    continue;
                }
                let signature = portlight_platform::signature::check(&path);
                let state = app.state::<AppState>();
                let mut tracker = state.tracker();
                if tracker.set_signature(&app_id, signature) {
                    state.save_known_apps(&tracker);
                }
            }
        });
        if let Err(error) = started {
            eprintln!("portlight: could not start the signature thread: {error}");
        }
        Self { queue }
    }

    pub fn request(&self, app_id: String, path: PathBuf) {
        let _ = self.queue.send((app_id, path));
    }
}

/// Icon size in pixels; large enough to stay sharp on high-DPI screens.
const ICON_SIZE: u32 = 48;

/// Program icons as `data:` URLs, read once per app.
#[derive(Default)]
pub struct Icons {
    cache: Mutex<HashMap<String, Option<String>>>,
}

impl Icons {
    pub fn get(&self, app_id: &str, path: Option<&Path>) -> Option<String> {
        if let Some(found) = self.cache.lock().unwrap_or_else(|p| p.into_inner()).get(app_id) {
            return found.clone();
        }
        let url = path.and_then(icon_url);
        self.cache.lock().unwrap_or_else(|p| p.into_inner()).insert(app_id.to_string(), url.clone());
        url
    }
}

fn icon_url(path: &Path) -> Option<String> {
    let image = portlight_platform::icon::program_icon(path, ICON_SIZE)?;
    let mut png = Vec::new();
    {
        let mut encoder = png::Encoder::new(Cursor::new(&mut png), image.width, image.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().ok()?;
        writer.write_image_data(&image.rgba).ok()?;
    }
    Some(format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(png)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icons_become_png_data_urls_and_are_cached() {
        let windows = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
        let notepad = Path::new(&windows).join(r"System32\notepad.exe");
        let icons = Icons::default();
        let url = icons.get("notepad", Some(&notepad)).expect("notepad has an icon");
        assert!(url.starts_with("data:image/png;base64,iVBORw0KGgo"));
        // A second request is served from the cache, even without the path.
        assert_eq!(icons.get("notepad", None), Some(url));
        assert_eq!(icons.get("nothing", None), None);
    }
}
