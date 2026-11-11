// The one place the UI talks to the outside. Inside the desktop app it calls
// the Rust side; in a plain browser (`npm run dev`) it uses built-in demo data,
// which is also what the README screenshots show.

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import { createDemoBackend } from "./demo";
import type {
  CaptureStartOptions,
  CaptureStatus,
  CaptureSupport,
  CollectorStatus,
  ExportFormat,
  LiveExport,
  PacketDetail,
  PacketSelection,
  PacketRow,
  Snapshot,
  UpdateStatus,
} from "./types";

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

  /** The background service behind detailed monitoring. */
  collectorStatus(): Promise<CollectorStatus>;
  /** One administrator prompt; installs or updates the service and starts it. Rejects with a readable message. */
  installCollector(): Promise<CollectorStatus>;
  /** One administrator prompt; back to basic monitoring. */
  uninstallCollector(): Promise<CollectorStatus>;
  onCollectorStatus(onStatus: (status: CollectorStatus) => void): Promise<() => void>;

  /** Is Npcap installed, and which network cards can be captured. */
  captureSupport(): Promise<CaptureSupport>;
  /** Starts a new capture, replacing any earlier one. Rejects with a readable message. */
  captureStart(options: CaptureStartOptions): Promise<CaptureStatus>;
  captureStop(): Promise<CaptureStatus>;
  /** Forgets the packets; state goes back to "idle". */
  captureClear(): Promise<CaptureStatus>;
  captureStatus(): Promise<CaptureStatus>;
  /** About four times a second while running, and after every change. */
  onCaptureStatus(onStatus: (status: CaptureStatus) => void): Promise<() => void>;
  /** Applies a display filter; a bad one comes back as `filterError`. */
  captureSetFilter(filter: string): Promise<CaptureStatus>;
  /** Rows by position in the filtered list. */
  captureRows(start: number, count: number): Promise<PacketRow[]>;
  /** One packet, by frame number. */
  capturePacket(number: number): Promise<PacketDetail>;
  /** Asks where to save a .pcapng; the saved path, or null if cancelled. */
  captureSave(onlyShown: boolean): Promise<string | null>;
  /** Same, the packet list as CSV. */
  captureExportCsv(onlyShown: boolean): Promise<string | null>;
  /** Asks where to save, then writes the chosen packets in a format; the saved path, or null if cancelled. */
  captureExport(format: ExportFormat, selection: PacketSelection): Promise<string | null>;
  /** The live view as CSV: apps, open connections, or the connection history (last 2,000 events). */
  exportLive(what: LiveExport): Promise<string | null>;
  /** Opens the Npcap download page in the browser. */
  openNpcapDownload(): Promise<void>;
}

/** Minutes ahead of UTC, so exported times match the clock on this PC. */
const utcOffsetMinutes = () => -new Date().getTimezoneOffset();

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

  collectorStatus: () => invoke<CollectorStatus>("collector_status"),
  installCollector: () => invoke<CollectorStatus>("install_collector"),
  uninstallCollector: () => invoke<CollectorStatus>("uninstall_collector"),
  onCollectorStatus: (onStatus) => listen<CollectorStatus>("collector:status", (event) => onStatus(event.payload)),

  captureSupport: () => invoke<CaptureSupport>("capture_support"),
  captureStart: (options) => invoke<CaptureStatus>("capture_start", { options }),
  captureStop: () => invoke<CaptureStatus>("capture_stop"),
  captureClear: () => invoke<CaptureStatus>("capture_clear"),
  captureStatus: () => invoke<CaptureStatus>("capture_status"),
  onCaptureStatus: (onStatus) => listen<CaptureStatus>("capture:status", (event) => onStatus(event.payload)),
  captureSetFilter: (filter) => invoke<CaptureStatus>("capture_set_filter", { filter }),
  captureRows: (start, count) => invoke<PacketRow[]>("capture_rows", { start, count }),
  capturePacket: (number) => invoke<PacketDetail>("capture_packet", { number }),
  captureSave: (onlyShown) => invoke<string | null>("capture_save", { onlyShown }),
  captureExportCsv: (onlyShown) => invoke<string | null>("capture_export_csv", { onlyShown }),
  captureExport: (format, selection) =>
    invoke<string | null>("capture_export", { format, selection, utcOffsetMinutes: utcOffsetMinutes() }),
  exportLive: (what) => invoke<string | null>("export_live", { what, utcOffsetMinutes: utcOffsetMinutes() }),
  openNpcapDownload: () => invoke("open_npcap_download"),
};

const insideDesktopApp = "__TAURI_INTERNALS__" in window;

export const backend: Backend = insideDesktopApp ? tauriBackend : createDemoBackend();
