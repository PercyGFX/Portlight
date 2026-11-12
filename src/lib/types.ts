// Shapes of the data the Rust core sends. Keep in step with crates/core/src/model.rs.

export type Protocol = "tcp" | "udp";

/** Where the monitor's data comes from: the TCP table, or the Portlight Collector service. */
export type Source = "basic" | "detailed";

/** Who made a program, from its Authenticode signature. Checked on the PC, in the background. */
export type Signature =
  | { state: "signed"; publisher: string }
  | { state: "unsigned" }
  /** Not checked yet, or no program file. */
  | { state: "unknown" };

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
  /** This session, including closed connections. 0 in basic mode. */
  bytesSent: number;
  bytesReceived: number;
  signature: Signature;
}

export interface ConnView {
  id: number;
  appId: string;
  pid: number;
  protocol: Protocol;
  remoteIp: string;
  remotePort: number;
  localPort: number;
  host: string | null;
  openedAt: number;
  /** Since Portlight first saw this connection. 0 in basic mode. */
  bytesSent: number;
  bytesReceived: number;
  /** Friendly network owner, e.g. "Cloudflare, Inc."; "Your home network" for local addresses. */
  owner: string | null;
  /** Autonomous system number; null for local or unknown. */
  asn: number | null;
  /** ISO 3166-1 alpha-2 of where the network is registered (not where the server sits). */
  country: string | null;
}

export type EventKind = "opened" | "closed";

export interface FlowEvent {
  seq: number;
  time: number;
  kind: EventKind;
  appId: string;
  appName: string;
  protocol: Protocol;
  remoteIp: string;
  remotePort: number;
  host: string | null;
  /** "closed" events carry the connection's totals; "opened" events carry 0. */
  bytesSent: number;
  bytesReceived: number;
}

export interface Snapshot {
  now: number;
  /** Seconds left in the first-run learning period; 0 once alerts are live. */
  learningSecondsLeft: number;
  source: Source;
  apps: AppView[];
  connections: ConnView[];
  events: FlowEvent[];
}

// ---------- History ----------

/** One server an app talked to on one day. */
export interface HistoryServer {
  /** Host name if known, else the IP. */
  name: string;
  ip: string;
  port: number;
  protocol: Protocol;
  /** Connections opened that day. */
  connections: number;
  bytesSent: number;
  bytesReceived: number;
  owner: string | null;
  country: string | null;
  /** First and last time that day, Unix milliseconds. */
  firstSeen: number;
  lastSeen: number;
}

export interface HistoryDay {
  /** "2026-11-12", local time. */
  day: string;
  connections: number;
  bytesSent: number;
  bytesReceived: number;
  /** Busiest first; at most 200. */
  servers: HistoryServer[];
}

/** One app's totals for one day, for the overview. */
export interface HistoryAppDay {
  day: string;
  appId: string;
  appName: string;
  connections: number;
  servers: number;
  bytesSent: number;
  bytesReceived: number;
}

export interface HistorySettings {
  enabled: boolean;
  /** 7, 30, 90 or 365. */
  keepDays: number;
  /** Size of the history file on disk. */
  sizeBytes: number;
  oldestDay: string | null;
}

// ---------- AI assistants (the portlight-mcp program) ----------

export interface ClaudeDesktop {
  installed: boolean;
  /** Portlight is in Claude Desktop's config file. */
  configured: boolean;
  configPath: string | null;
}

export interface AssistantStatus {
  /** The main switch; off by default. */
  enabled: boolean;
  /** Full path of portlight-mcp.exe; null when this copy has none (e.g. plain `npm run dev`). */
  mcpPath: string | null;
  claudeDesktop: ClaudeDesktop;
  /** `claude mcp add ...`, for pasting into a terminal. */
  claudeCodeCommand: string | null;
  /** The `claude` command was found, so the one-click Add works. */
  claudeCodeInstalled: boolean;
  /** Portlight is already in Claude Code's user-wide MCP list. */
  claudeCodeConfigured: boolean;
  /** Pretty JSON for any other app's MCP settings file. */
  configSnippet: string | null;
}

export type AssistantOutcome = "answered" | "error" | "allowed" | "denied";

export interface AssistantLogEntry {
  id: number;
  /** Unix milliseconds. */
  time: number;
  /** e.g. "Claude 1.4", "gemini-cli 0.9". */
  client: string;
  /** e.g. "get_app". */
  tool: string;
  /** Plain words, e.g. "Details of Firefox". */
  summary: string;
  outcome: AssistantOutcome;
  /** Size of what was sent to the assistant. */
  bytes: number;
}

/** An assistant wants to start a capture and waits (up to 2 minutes) for an answer. */
export interface AssistantConsent {
  id: number;
  client: string;
  /** e.g. "Record packets from Firefox for 30 seconds". */
  action: string;
  detail: string;
}

export type AssistantApp = "claudeDesktop" | "claudeCode";

// ---------- Settings and About ----------

export interface AppSettings {
  autoUpdate: boolean;
  historyEnabled: boolean;
  historyKeepDays: number;
  /** Closing the window keeps Portlight in the tray; off = closing quits. */
  closeToTray: boolean;
  /** A Windows notification when a new app goes online. */
  notifyNewApps: boolean;
  /** Ask the DNS server for server names; off = Portlight makes no DNS queries of its own. */
  reverseLookups: boolean;
  startAtSignIn: boolean;
  /** Only the installed copy can start at sign-in. */
  canStartAtSignIn: boolean;
  /** AI assistants may read Portlight (read-only mirror; change it with `set_assistant_access`). */
  assistantAccess: boolean;
}

/** The settings `set_settings` can change. */
export type SettingsChange = Partial<Pick<AppSettings, "autoUpdate" | "closeToTray" | "notifyNewApps" | "reverseLookups" | "startAtSignIn">>;

export interface About {
  version: string;
  authors: string;
  repository: string;
  license: string;
  /** When the bundled server owner list was built; Unix milliseconds. */
  ownersBuiltAt: number | null;
  /** Network ranges in that list. */
  ownerRanges: number | null;
  dataFolder: string;
}

/** Places `open_link` can open. */
export type LinkTarget = "repository" | "issues" | "releases" | "changelog" | "license" | "npcap" | "iptoasn" | "dataFolder";

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

/** The Portlight Collector, the background service behind detailed monitoring. */
export interface CollectorStatus {
  /** The Windows service exists. */
  installed: boolean;
  /** This window is receiving its feed right now (source is "detailed"). */
  connected: boolean;
  /** The service's version, when connected. */
  version: string | null;
  /** The service is older than this app. */
  outdated: boolean;
  /** False when the service program is missing (e.g. plain `npm run dev`). */
  canInstall: boolean;
  /** The last problem, in plain words. */
  error: string | null;
}

// ---------- Packet capture ----------

/** What to capture. */
export type CaptureTarget =
  | { kind: "all" }
  | { kind: "app"; appId: string; name: string }
  | { kind: "host"; ip: string; name: string | null }
  | { kind: "domain"; name: string }
  | {
      kind: "connection";
      protocol: Protocol;
      localPort: number;
      remoteIp: string;
      remotePort: number;
      appName: string;
      host: string | null;
    };

export interface CaptureInterface {
  /** Npcap device name, e.g. "\\Device\\NPF_{GUID}". */
  id: string;
  /** Friendly, e.g. "Wi-Fi". */
  name: string;
  description: string;
  addresses: string[];
  /** Up, connected and has an address; used when no interface is chosen. */
  active: boolean;
}

export interface CaptureSupport {
  /** Npcap is installed and usable. */
  available: boolean;
  npcapVersion: string | null;
  interfaces: CaptureInterface[];
  /** Plain words, e.g. "Npcap is not installed."; null when available. */
  problem: string | null;
}

/** When a capture stops by itself. All null = run until stopped. */
export interface StopAfter {
  packets: number | null;
  seconds: number | null;
  megabytes: number | null;
}

export interface CaptureStartOptions {
  target: CaptureTarget;
  /** Interface ids; empty = every active interface. */
  interfaces: string[];
  /** A BPF capture filter like Wireshark's; "" = none. */
  captureFilter: string;
  /** Optional; none when left out. */
  stopAfter?: StopAfter;
}

export type CaptureState = "idle" | "running" | "stopped";

export interface CaptureStatus {
  state: CaptureState;
  target: CaptureTarget | null;
  startedAt: number | null;
  stoppedAt: number | null;
  /** Packets kept for this capture. */
  packets: number;
  /** Packets matching the current display filter. */
  shown: number;
  /** Total bytes of kept packets. */
  bytes: number;
  /** Lost by the driver or because the capture was full. */
  dropped: number;
  /** Display filter currently applied ("" = all). */
  filter: string;
  /** Why the last filter text did not parse; the old filter stays applied. */
  filterError: string | null;
  /** The capture stopped itself at the packet or memory limit. */
  limitReached: boolean;
  error: string | null;
  stopAfter: StopAfter;
  /** Why the capture stopped by itself, e.g. "Stopped after 60 seconds, as you asked." */
  stopReason: string | null;
}

export type ExportFormat = "pcapng" | "csv" | "text" | "json";

/** Which packets to save or export. */
export type PacketSelection =
  | { kind: "all" }
  /** Matching the current search. */
  | { kind: "shown" }
  | { kind: "packets"; numbers: number[] }
  /** Frame numbers, inclusive, either order. */
  | { kind: "range"; from: number; to: number };

/** What the live view can export as CSV. */
export type LiveExport = "apps" | "connections" | "flow";

export interface PacketRow {
  /** 1-based frame number, like Wireshark's "No.". */
  number: number;
  /** Seconds since the capture started. */
  time: number;
  /** IP address, or MAC address for non-IP frames. */
  source: string;
  destination: string;
  /** "This PC", or a host name when known. */
  sourceName: string | null;
  destinationName: string | null;
  sourcePort: number | null;
  destinationPort: number | null;
  /** Highest layer understood: "TLS", "DNS", "TCP", ... */
  protocol: string;
  /** Bytes on the wire. */
  length: number;
  /** Wireshark-style technical line. */
  info: string;
  /** Plain English. */
  summary: string;
  app: string | null;
  /** Relative to this PC. */
  direction: "out" | "in" | null;
}

export interface PacketField {
  label: string;
  value: string;
  explain: string | null;
  /** Byte offset in the frame. */
  start: number;
  /** Number of bytes; 0 if not tied to bytes. */
  length: number;
  children: PacketField[];
}

export interface PacketLayer {
  /** "frame" | "eth" | "ip" | "tcp" | "dns" | "tls" ... */
  id: string;
  title: string;
  summary: string;
  explain: string;
  start: number;
  length: number;
  fields: PacketField[];
}

export interface PacketDetail {
  number: number;
  /** Seconds since capture start. */
  time: number;
  /** Unix milliseconds. */
  timestamp: number;
  length: number;
  capturedLength: number;
  interfaceName: string;
  app: string | null;
  layers: PacketLayer[];
  bytes: number[];
}
