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
