//! The desktop shell: a window, a tray icon and notifications around
//! `portlight-core`. This crate only wires things together; the rules live in
//! the core and the operating-system calls live in `portlight-platform`.
//!
//! Command-line flags:
//! - `--background` starts hidden in the tray (used when starting with Windows).
//! - `--open` starts with the window showing, which is also the default.
//! - `--quit` asks a running Portlight to exit (used by the installer).

mod capture;
mod collector;
mod commands;
mod dns;
mod export;
mod monitor;
mod settings;
mod state;
mod tray;
mod update;

use std::sync::Arc;

use tauri::{AppHandle, Manager, WindowEvent};

use crate::capture::Capture;
use crate::state::AppState;

const MAIN_WINDOW: &str = "main";

pub fn run() {
    let args: Vec<String> = std::env::args().collect();
    let wants_quit = args.iter().any(|a| a == "--quit");
    let in_background = args.iter().any(|a| a == "--background");

    tauri::Builder::default()
        // Must be the first plugin. A second launch lands here, in the copy
        // that is already running, and the second copy exits by itself.
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            if args.iter().any(|a| a == "--quit") {
                app.exit(0);
            } else {
                show_main_window(app);
            }
        }))
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            commands::get_snapshot,
            commands::trust_app,
            commands::trust_all,
            commands::update_status,
            commands::set_auto_update,
            commands::check_for_update,
            commands::install_update,
            commands::collector_status,
            commands::install_collector,
            commands::uninstall_collector,
            commands::capture_support,
            commands::capture_start,
            commands::capture_stop,
            commands::capture_clear,
            commands::capture_status,
            commands::capture_set_filter,
            commands::capture_rows,
            commands::capture_packet,
            commands::capture_save,
            commands::capture_export_csv,
            commands::capture_export,
            commands::export_live,
            commands::open_npcap_download,
        ])
        .setup(move |app| {
            if wants_quit {
                // Nothing was running, so there is nothing to quit.
                app.handle().exit(0);
                return Ok(());
            }

            let data_dir = app.path().app_data_dir()?;
            app.manage(AppState::load(&data_dir));
            app.manage(Arc::new(Capture::default()));

            tray::create(app.handle())?;
            monitor::spawn(app.handle().clone());
            update::spawn_auto_updater(app.handle().clone());

            if !in_background {
                show_main_window(app.handle());
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the window keeps Portlight watching from the tray.
            if let WindowEvent::CloseRequested { api, .. } = event {
                let _ = window.hide();
                api.prevent_close();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running Portlight");
}

/// Brings the main window to the front, un-hiding it if needed.
pub(crate) fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}
