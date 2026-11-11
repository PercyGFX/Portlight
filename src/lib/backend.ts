// The one place the UI talks to the outside. Inside the desktop app it calls
// the Rust side; in a plain browser (`npm run dev`) it uses built-in demo data,
// which is also what the README screenshots show.

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import { createDemoBackend } from "./demo";
import type { Snapshot, UpdateStatus } from "./types";

export interface Backend {
  /** True when showing demo data instead of this PC's real connections. */
  readonly demo: boolean;
  /** The full current picture, including recent events. */
  getSnapshot(): Promise<Snapshot>;
  /** Calls `onTick` about twice a second. Returns a function that stops it. */
  onTick(onTick: (snapshot: Snapshot) => void): Promise<() => void>;
  trustApp(id: string): Promise<void>;
  trustAll(): Promise<void>;

  updateStatus(): Promise<UpdateStatus>;
  /** Calls `onStatus` when a background check finds something. */
  onUpdateStatus(onStatus: (status: UpdateStatus) => void): Promise<() => void>;
  /** Asks GitHub right now. Rejects with a readable message when offline. */
  checkForUpdate(): Promise<UpdateStatus>;
  setAutoUpdate(enabled: boolean): Promise<UpdateStatus>;
  /** Downloads and installs the update, then restarts the app. */
  installUpdate(): Promise<void>;
}

const tauriBackend: Backend = {
  demo: false,
  getSnapshot: () => invoke<Snapshot>("get_snapshot"),
  onTick: (onTick) => listen<Snapshot>("monitor:tick", (event) => onTick(event.payload)),
  trustApp: (id) => invoke("trust_app", { id }),
  trustAll: () => invoke("trust_all"),
  updateStatus: () => invoke<UpdateStatus>("update_status"),
  onUpdateStatus: (onStatus) => listen<UpdateStatus>("update:status", (event) => onStatus(event.payload)),
  checkForUpdate: () => invoke<UpdateStatus>("check_for_update"),
  setAutoUpdate: (enabled) => invoke<UpdateStatus>("set_auto_update", { enabled }),
  installUpdate: () => invoke("install_update"),
};

const insideDesktopApp = "__TAURI_INTERNALS__" in window;

export const backend: Backend = insideDesktopApp ? tauriBackend : createDemoBackend();
