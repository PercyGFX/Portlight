// Shapes of the data the Rust core sends. Keep in step with crates/core/src/model.rs.

export interface AppView {
  /** Stable identity: the lower-cased executable path. */
  id: string;
  name: string;
  exe: string;
  path: string | null;
  /** Unix milliseconds. */
  firstSeen: number;
  /** False until the user acknowledges a new app. */
  trusted: boolean;
  /** Connections open right now. */
  connections: number;
  /** Distinct remote addresses right now. */
  servers: number;
  totalOpened: number;
}

export interface ConnView {
  id: number;
  appId: string;
  pid: number;
  remoteIp: string;
  remotePort: number;
  localPort: number;
  host: string | null;
  openedAt: number;
}

export type EventKind = "opened" | "closed";

export interface FlowEvent {
  seq: number;
  time: number;
  kind: EventKind;
  appId: string;
  appName: string;
  remoteIp: string;
  remotePort: number;
  host: string | null;
}

export interface Snapshot {
  now: number;
  /** Seconds left in the first-run learning period; 0 once alerts are live. */
  learningSecondsLeft: number;
  apps: AppView[];
  connections: ConnView[];
  events: FlowEvent[];
}

/** Keep in step with UpdateStatus in src-tauri/src/update.rs. */
export interface UpdateStatus {
  current: string;
  /** Install new versions quietly in the background. */
  autoUpdate: boolean;
  /** Only the installed copy can update itself. */
  canInstall: boolean;
  availableVersion: string | null;
  releasePage: string | null;
}
