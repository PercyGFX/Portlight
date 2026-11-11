//! What the window can ask the Rust side to do.

use portlight_core::Snapshot;
use tauri::{AppHandle, Manager, State};

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
