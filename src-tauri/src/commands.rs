//! What the window can ask the Rust side to do.

use std::path::PathBuf;
use std::sync::Arc;

use portlight_core::Snapshot;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;

use crate::capture::{
    self, Capture, CaptureStartOptions, CaptureStatus, CaptureSupport, ExportFormat, PacketDetail, PacketRow, Selection,
};
use crate::export::LiveExport;
use crate::history::{HistoryAppDay, HistoryDay, HistorySettings};
use crate::collector::{self, CollectorStatus};
use crate::state::{AppState, now_ms};
use crate::update::{self, UpdateStatus};

/// The full current picture, with every buffered event. The window calls
/// this once when it opens, then follows the tick events.
#[tauri::command]
pub fn get_snapshot(state: State<'_, AppState>) -> Snapshot {
    state.tracker().snapshot(now_ms(), 0)
}

/// Marks one new app as acknowledged.
#[tauri::command]
pub fn trust_app(state: State<'_, AppState>, id: String) {
    let mut tracker = state.tracker();
    if tracker.trust(&id) {
        state.save_known_apps(&tracker);
    }
}

/// Marks every new app as acknowledged.
#[tauri::command]
pub fn trust_all(state: State<'_, AppState>) {
    let mut tracker = state.tracker();
    if tracker.trust_all() {
        state.save_known_apps(&tracker);
    }
}

/// This version, the update setting, and any update already found.
#[tauri::command]
pub fn update_status(state: State<'_, AppState>) -> UpdateStatus {
    update::status(&state)
}

/// Turns quiet automatic updates on or off.
#[tauri::command]
pub fn set_auto_update(state: State<'_, AppState>, enabled: bool) -> UpdateStatus {
    state.update_settings(|settings| settings.auto_update = enabled);
    update::status(&state)
}

/// Asks GitHub for a newer release right now. Runs off the UI thread because
/// it waits on the network.
#[tauri::command]
pub async fn check_for_update(app: AppHandle) -> Result<UpdateStatus, String> {
    tauri::async_runtime::spawn_blocking(move || update::refresh(&app)).await.map_err(|e| e.to_string())?
}

/// Downloads and installs the update that was found, then restarts Portlight
/// with its window open.
#[tauri::command]
pub async fn install_update(app: AppHandle) -> Result<(), String> {
    if app.state::<AppState>().available_update().is_none() {
        return Err("There is no update to install".into());
    }
    tauri::async_runtime::spawn_blocking(move || update::install_available(&app, "--open"))
        .await
        .map_err(|e| e.to_string())?
}

// ---------- The Portlight Collector service ----------

#[tauri::command]
pub fn collector_status(app: AppHandle) -> CollectorStatus {
    collector::status(&app)
}

/// Installs or updates the service. Shows one Windows administrator prompt.
#[tauri::command]
pub async fn install_collector(app: AppHandle) -> Result<CollectorStatus, String> {
    tauri::async_runtime::spawn_blocking(move || collector::install(&app)).await.map_err(|e| e.to_string())?
}

/// Removes the service. Shows one Windows administrator prompt.
#[tauri::command]
pub async fn uninstall_collector(app: AppHandle) -> Result<CollectorStatus, String> {
    tauri::async_runtime::spawn_blocking(move || collector::uninstall(&app)).await.map_err(|e| e.to_string())?
}

// ---------- Packet capture ----------

#[tauri::command]
pub async fn capture_support(capture: State<'_, Arc<Capture>>) -> Result<CaptureSupport, String> {
    let capture = Arc::clone(&capture);
    tauri::async_runtime::spawn_blocking(move || capture.support()).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn capture_start(
    app: AppHandle,
    capture: State<'_, Arc<Capture>>,
    options: CaptureStartOptions,
) -> Result<CaptureStatus, String> {
    let capture = Arc::clone(&capture);
    tauri::async_runtime::spawn_blocking(move || capture.start(&app, options)).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn capture_stop(app: AppHandle, capture: State<'_, Arc<Capture>>) -> CaptureStatus {
    let status = capture.stop();
    let _ = app.emit(capture::STATUS_EVENT, &status);
    status
}

#[tauri::command]
pub fn capture_clear(app: AppHandle, capture: State<'_, Arc<Capture>>) -> CaptureStatus {
    let status = capture.clear();
    let _ = app.emit(capture::STATUS_EVENT, &status);
    status
}

#[tauri::command]
pub fn capture_status(capture: State<'_, Arc<Capture>>) -> CaptureStatus {
    capture.status()
}

/// Applies a display filter. Can take a moment over a large capture.
#[tauri::command]
pub async fn capture_set_filter(
    app: AppHandle,
    capture: State<'_, Arc<Capture>>,
    filter: String,
) -> Result<CaptureStatus, String> {
    let capture = Arc::clone(&capture);
    let status =
        tauri::async_runtime::spawn_blocking(move || capture.set_filter(&filter)).await.map_err(|e| e.to_string())?;
    let _ = app.emit(capture::STATUS_EVENT, &status);
    Ok(status)
}

#[tauri::command]
pub fn capture_rows(capture: State<'_, Arc<Capture>>, start: u32, count: u32) -> Vec<PacketRow> {
    capture.rows(start, count)
}

#[tauri::command]
pub fn capture_packet(capture: State<'_, Arc<Capture>>, number: u32) -> Result<PacketDetail, String> {
    capture.packet(number).ok_or_else(|| format!("There is no packet number {number}."))
}

/// Asks where to save, then writes a .pcapng file Wireshark can open.
/// Returns the path, or nothing if the person cancelled.
#[tauri::command]
pub async fn capture_save(app: AppHandle, capture: State<'_, Arc<Capture>>, only_shown: bool) -> Result<Option<String>, String> {
    let selection = if only_shown { Selection::Shown } else { Selection::All };
    export(app, Arc::clone(&capture), ExportFormat::Pcapng, selection, 0).await
}

/// Asks where to save, then writes the packet list as CSV.
#[tauri::command]
pub async fn capture_export_csv(
    app: AppHandle,
    capture: State<'_, Arc<Capture>>,
    only_shown: bool,
) -> Result<Option<String>, String> {
    let selection = if only_shown { Selection::Shown } else { Selection::All };
    export(app, Arc::clone(&capture), ExportFormat::Csv, selection, 0).await
}

/// Asks where to save, then writes the chosen packets as pcapng, CSV, a
/// readable text breakdown or JSON. `utc_offset_minutes` puts times in the
/// person's time zone.
#[tauri::command]
pub async fn capture_export(
    app: AppHandle,
    capture: State<'_, Arc<Capture>>,
    format: ExportFormat,
    selection: Selection,
    utc_offset_minutes: Option<i32>,
) -> Result<Option<String>, String> {
    export(app, Arc::clone(&capture), format, selection, utc_offset_minutes.unwrap_or(0)).await
}

async fn export(
    app: AppHandle,
    capture: Arc<Capture>,
    format: ExportFormat,
    selection: Selection,
    utc_offset_minutes: i32,
) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let name = capture.suggested_name(format.extension());
        save_as(&app, &name, format.kind(), format.extension(), |path| {
            capture.export(path, format, &selection, utc_offset_minutes)
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Asks where to save, then writes part of the live view as CSV: the apps,
/// the open connections, or the connection flow history.
#[tauri::command]
pub async fn export_live(app: AppHandle, what: LiveExport, utc_offset_minutes: Option<i32>) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let snapshot = app.state::<AppState>().tracker().snapshot(now_ms(), 0);
        let name = format!("portlight-{}-{}.csv", what.slug(), crate::state::today());
        save_as(&app, &name, "CSV (spreadsheet)", "csv", |path| {
            crate::export::live_csv(path, &snapshot, what, utc_offset_minutes.unwrap_or(0))
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Opens Npcap's download page in the browser.
#[tauri::command]
pub fn open_npcap_download() -> Result<(), String> {
    portlight_platform::service::open_url("https://npcap.com/#download").map_err(|e| e.to_string())
}

fn save_as(
    app: &AppHandle,
    file_name: &str,
    kind: &str,
    extension: &str,
    write: impl FnOnce(&PathBuf) -> std::io::Result<()>,
) -> Result<Option<String>, String> {
    let chosen = app.dialog().file().set_file_name(file_name).add_filter(kind, &[extension]).blocking_save_file();
    let Some(chosen) = chosen else { return Ok(None) };
    let path = chosen.into_path().map_err(|e| e.to_string())?;
    write(&path).map_err(|e| format!("Could not save {}: {e}", path.display()))?;
    Ok(Some(path.display().to_string()))
}

// ---------- App identity and history ----------

/// An app's icon as a `data:image/png;base64,...` URL, or nothing.
#[tauri::command]
pub async fn app_icon(app: AppHandle, app_id: String) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let path = state.tracker().app_path(&app_id).map(std::path::PathBuf::from);
        state.icons.get(&app_id, path.as_deref())
    })
    .await
    .map_err(|e| e.to_string())
}

/// One app's history, newest day first.
#[tauri::command]
pub async fn app_history(app: AppHandle, app_id: String, days: u32) -> Result<Vec<HistoryDay>, String> {
    tauri::async_runtime::spawn_blocking(move || app.state::<AppState>().history.app_days(&app_id, days.min(366)))
        .await
        .map_err(|e| e.to_string())
}

/// Every app's totals per day.
#[tauri::command]
pub async fn history_summary(app: AppHandle, days: u32) -> Result<Vec<HistoryAppDay>, String> {
    tauri::async_runtime::spawn_blocking(move || app.state::<AppState>().history.summary(days.min(366)))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn history_settings(state: State<'_, AppState>) -> HistorySettings {
    let settings = state.settings();
    state.history.settings(settings.history_enabled, settings.history_keep_days)
}

/// Turns the history on or off and sets how long it is kept.
#[tauri::command]
pub fn set_history_settings(state: State<'_, AppState>, enabled: bool, keep_days: u32) -> HistorySettings {
    let keep_days = keep_days.clamp(1, 3650);
    state.update_settings(|settings| {
        settings.history_enabled = enabled;
        settings.history_keep_days = keep_days;
    });
    if !enabled {
        state.history.drop_pending();
    }
    state.history.prune_now(keep_days);
    state.history.settings(enabled, keep_days)
}

/// Deletes the whole history.
#[tauri::command]
pub fn clear_history(state: State<'_, AppState>) -> HistorySettings {
    state.history.clear();
    let settings = state.settings();
    state.history.settings(settings.history_enabled, settings.history_keep_days)
}

// ---------- Settings and About ----------

/// Everything the Settings page shows. Keep in step with src/lib/types.ts.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub auto_update: bool,
    pub history_enabled: bool,
    pub history_keep_days: u32,
    pub close_to_tray: bool,
    pub notify_new_apps: bool,
    pub reverse_lookups: bool,
    pub start_at_sign_in: bool,
    /// Only the installed copy can start at sign-in.
    pub can_start_at_sign_in: bool,
}

/// Any subset of the settings to change.
#[derive(serde::Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct SettingsChange {
    pub auto_update: Option<bool>,
    pub close_to_tray: Option<bool>,
    pub notify_new_apps: Option<bool>,
    pub reverse_lookups: Option<bool>,
    pub start_at_sign_in: Option<bool>,
}

fn app_settings(state: &AppState) -> AppSettings {
    let settings = state.settings();
    AppSettings {
        auto_update: settings.auto_update,
        history_enabled: settings.history_enabled,
        history_keep_days: settings.history_keep_days,
        close_to_tray: settings.close_to_tray,
        notify_new_apps: settings.notify_new_apps,
        reverse_lookups: settings.reverse_lookups,
        start_at_sign_in: portlight_platform::startup::is_enabled(),
        can_start_at_sign_in: update::running_installed_copy(),
    }
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> AppSettings {
    app_settings(&state)
}

#[tauri::command]
pub fn set_settings(state: State<'_, AppState>, change: SettingsChange) -> Result<AppSettings, String> {
    if let Some(enabled) = change.start_at_sign_in {
        if enabled && !update::running_installed_copy() {
            return Err("Only the installed copy of Portlight can start when you sign in.".into());
        }
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        portlight_platform::startup::set(enabled, &format!("\"{}\" --background", exe.display()))
            .map_err(|e| format!("Windows did not allow the change: {e}"))?;
    }
    state.update_settings(|settings| {
        if let Some(v) = change.auto_update {
            settings.auto_update = v;
        }
        if let Some(v) = change.close_to_tray {
            settings.close_to_tray = v;
        }
        if let Some(v) = change.notify_new_apps {
            settings.notify_new_apps = v;
        }
        if let Some(v) = change.reverse_lookups {
            settings.reverse_lookups = v;
        }
    });
    Ok(app_settings(&state))
}

/// Trusts every app seen so far and starts a new two-minute learning period.
#[tauri::command]
pub fn learn_again(state: State<'_, AppState>) {
    let mut tracker = state.tracker();
    tracker.learn_again(now_ms());
    state.save_known_apps(&tracker);
}

/// Forgets every app Portlight has seen and starts learning again.
#[tauri::command]
pub fn forget_known_apps(state: State<'_, AppState>) {
    let mut tracker = state.tracker();
    tracker.forget_known_apps(now_ms());
    state.save_known_apps(&tracker);
}

/// What the About section shows. Keep in step with src/lib/types.ts.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct About {
    pub version: String,
    pub authors: String,
    pub repository: String,
    pub license: String,
    /// When the server owner list was built (Unix ms), if there is one.
    pub owners_built_at: Option<u64>,
    pub owner_ranges: Option<usize>,
    pub data_folder: String,
}

#[tauri::command]
pub fn about(app: AppHandle) -> About {
    let info = crate::owners::list_info();
    About {
        version: env!("CARGO_PKG_VERSION").into(),
        authors: env!("CARGO_PKG_AUTHORS").replace(':', ", "),
        repository: env!("CARGO_PKG_REPOSITORY").into(),
        license: env!("CARGO_PKG_LICENSE").into(),
        owners_built_at: info.map(|(at, _)| at * 1000),
        owner_ranges: info.map(|(_, ranges)| ranges),
        data_folder: app.path().app_data_dir().map(|p| p.display().to_string()).unwrap_or_default(),
    }
}

/// Opens one of Portlight's own web pages, or its data folder. Only these
/// fixed places can be opened, never an address from the window.
#[tauri::command]
pub fn open_link(app: AppHandle, link: String) -> Result<(), String> {
    let repository = env!("CARGO_PKG_REPOSITORY");
    let target = match link.as_str() {
        "repository" => repository.to_string(),
        "issues" => format!("{repository}/issues"),
        "releases" => format!("{repository}/releases"),
        "changelog" => format!("{repository}/blob/main/CHANGELOG.md"),
        "license" => format!("{repository}/blob/main/LICENSE"),
        "npcap" => "https://npcap.com/#download".into(),
        "iptoasn" => "https://iptoasn.com/".into(),
        "dataFolder" => app.path().app_data_dir().map_err(|e| e.to_string())?.display().to_string(),
        _ => return Err(format!("Unknown link: {link}")),
    };
    portlight_platform::service::open_url(&target).map_err(|e| e.to_string())
}
