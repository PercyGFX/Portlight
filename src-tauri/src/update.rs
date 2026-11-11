//! Self-update from GitHub Releases.
//!
//! A release is a tag like `v0.2.0` with an asset named `Portlight-Setup-0.2.0.exe`
//! (built by .github/workflows/release.yml). The app reads the latest release from
//! the public API, downloads the installer, checks it against the SHA-256 digest
//! GitHub publishes for every asset, and runs it silently. The installer closes
//! Portlight, upgrades it in place and starts it again.
//!
//! This is the only request Portlight makes to a server of its own accord.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use serde::Serialize;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter, Manager};

use crate::state::AppState;

pub const REPO: &str = "PercyGFX/Portlight";
const ASSET_PREFIX: &str = "Portlight-Setup-";
const USER_AGENT: &str = concat!("Portlight/", env!("CARGO_PKG_VERSION"));

/// Event sent to the window when the update picture changes. Payload: `UpdateStatus`.
pub const STATUS_EVENT: &str = "update:status";

const FIRST_CHECK_AFTER: Duration = Duration::from_secs(120);
const CHECK_EVERY: Duration = Duration::from_secs(12 * 3600);

#[derive(Clone, Debug, PartialEq)]
pub struct Release {
    pub version: String,
    pub url: String,
    pub size: u64,
    /// Lowercase hex SHA-256 from GitHub, when available.
    pub sha256: Option<String>,
    /// The release page, for "what's new".
    pub page: String,
}

/// What the window shows about updates.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatus {
    pub current: &'static str,
    pub auto_update: bool,
    /// Only the installed copy can update itself.
    pub can_install: bool,
    pub available_version: Option<String>,
    pub release_page: Option<String>,
}

pub fn current() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

pub fn status(state: &AppState) -> UpdateStatus {
    let available = state.available_update();
    UpdateStatus {
        current: current(),
        auto_update: state.settings().auto_update,
        can_install: running_installed_copy(),
        available_version: available.as_ref().map(|r| r.version.clone()),
        release_page: available.map(|r| r.page),
    }
}

fn parse(v: &str) -> Option<(u64, u64, u64)> {
    let v = v.trim().trim_start_matches(['v', 'V']);
    let core = v.split(['-', '+']).next()?;
    let mut it = core.split('.').map(|p| p.parse::<u64>());
    Some((it.next()?.ok()?, it.next().unwrap_or(Ok(0)).ok()?, it.next().unwrap_or(Ok(0)).ok()?))
}

pub fn is_newer(candidate: &str, than: &str) -> bool {
    match (parse(candidate), parse(than)) {
        (Some(a), Some(b)) => a > b,
        _ => false,
    }
}

/// Where the installer puts the app. Automatic installs only happen from here, so a
/// development build or a copy run from Downloads never replaces itself.
pub fn install_dir() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(|d| PathBuf::from(d).join("Programs").join("Portlight"))
}

pub fn running_installed_copy() -> bool {
    let norm = |p: &Path| p.to_string_lossy().trim_end_matches('\\').to_ascii_lowercase();
    let Ok(exe) = std::env::current_exe() else { return false };
    match (install_dir(), exe.parent()) {
        (Some(dir), Some(parent)) => norm(parent) == norm(&dir),
        _ => false,
    }
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(15)).timeout_read(Duration::from_secs(60)).build()
}

/// Returns the latest release if it is newer than this build.
pub fn check() -> Result<Option<Release>, String> {
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let resp = match agent().get(&url).set("User-Agent", USER_AGENT).set("Accept", "application/vnd.github+json").call() {
        Ok(r) => r,
        // No releases published yet.
        Err(ureq::Error::Status(404, _)) => return Ok(None),
        Err(e) => return Err(format!("Couldn't reach GitHub ({e})")),
    };
    let json: serde_json::Value = resp.into_json().map_err(|e| format!("Unexpected reply from GitHub ({e})"))?;
    let tag = json["tag_name"].as_str().unwrap_or_default();
    if !is_newer(tag, current()) {
        return Ok(None);
    }
    let asset = json["assets"]
        .as_array()
        .and_then(|a| {
            a.iter().find(|x| x["name"].as_str().is_some_and(|n| n.starts_with(ASSET_PREFIX) && n.ends_with(".exe")))
        })
        .ok_or_else(|| format!("Release {tag} has no installer attached"))?;
    Ok(Some(Release {
        version: tag.trim_start_matches(['v', 'V']).to_string(),
        url: asset["browser_download_url"].as_str().unwrap_or_default().to_string(),
        size: asset["size"].as_u64().unwrap_or(0),
        sha256: asset["digest"].as_str().and_then(|d| d.strip_prefix("sha256:")).map(|s| s.to_ascii_lowercase()),
        page: json["html_url"].as_str().unwrap_or_default().to_string(),
    }))
}

/// Downloads the installer to %TEMP% and checks it against GitHub's digest.
pub fn download(rel: &Release) -> Result<PathBuf, String> {
    if !rel.url.starts_with("https://") {
        return Err("Refusing a non-HTTPS download".into());
    }
    let resp = agent().get(&rel.url).set("User-Agent", USER_AGENT).call().map_err(|e| format!("Download failed ({e})"))?;
    let path = std::env::temp_dir().join(format!("{ASSET_PREFIX}{}.exe", rel.version));
    let mut file = std::fs::File::create(&path).map_err(|e| e.to_string())?;
    let mut reader = resp.into_reader();
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = reader.read(&mut buf).map_err(|e| format!("Download interrupted ({e})"))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        file.write_all(&buf[..n]).map_err(|e| e.to_string())?;
    }
    drop(file);
    if let Some(expected) = &rel.sha256 {
        let actual: String = hasher.finalize().iter().map(|b| format!("{b:02x}")).collect();
        if &actual != expected {
            let _ = std::fs::remove_file(&path);
            return Err("The download didn't match GitHub's checksum, so it was discarded".into());
        }
    }
    Ok(path)
}

/// Starts the installer silently. `launch` is what the installer runs afterwards:
/// "--background" for a quiet update, "--open" to come back with the window open.
pub fn install(installer: &Path, launch: &str) -> Result<(), String> {
    std::process::Command::new(installer)
        .args(["/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART", "/SP-", &format!("/launch={launch}")])
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Couldn't start the installer ({e})"))
}

/// Checks now, remembers the result and tells the window.
pub fn refresh(app: &AppHandle) -> Result<UpdateStatus, String> {
    let state = app.state::<AppState>();
    let found = check()?;
    state.set_available_update(found);
    let status = status(&state);
    let _ = app.emit(STATUS_EVENT, &status);
    Ok(status)
}

/// Downloads and installs the update the last check found, then exits so the
/// installer can replace the files.
pub fn install_available(app: &AppHandle, launch: &str) -> Result<(), String> {
    if !running_installed_copy() {
        return Err("Only the installed app can update itself. Get the new version from the releases page.".into());
    }
    let release = app.state::<AppState>().available_update().ok_or("There is no update to install")?;
    let installer = download(&release)?;
    install(&installer, launch)?;
    app.exit(0);
    Ok(())
}

/// Background updates: checks a couple of minutes after start and then twice a
/// day. With automatic updates on it installs quietly; with them off the window
/// shows "Update to x.y.z" and waits. Only the installed copy checks.
pub fn spawn_auto_updater(app: AppHandle) {
    if cfg!(debug_assertions) || !running_installed_copy() {
        return;
    }
    let started = thread::Builder::new().name("portlight-update".into()).spawn(move || {
        thread::sleep(FIRST_CHECK_AFTER);
        loop {
            match refresh(&app) {
                Ok(status) if status.available_version.is_some() && status.auto_update => {
                    match install_available(&app, "--background") {
                        Ok(()) => return,
                        Err(e) => eprintln!("portlight: update failed: {e}"),
                    }
                }
                Ok(_) => {}
                Err(e) => eprintln!("portlight: update check failed: {e}"),
            }
            thread::sleep(CHECK_EVERY);
        }
    });
    if let Err(error) = started {
        eprintln!("portlight: could not start the update thread: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_versions() {
        assert!(is_newer("v0.2.0", "0.1.0"));
        assert!(is_newer("1.0", "0.9.9"));
        assert!(!is_newer("v0.1.0", "0.1.0"));
        assert!(!is_newer("v0.1.0", "0.2.0"));
        assert!(is_newer("v0.10.0", "0.9.0"));
        assert!(!is_newer("garbage", "0.1.0"));
    }
}
