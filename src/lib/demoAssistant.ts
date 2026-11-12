// Pretend AI assistants for the demo: a switch, two apps to add Portlight to, and made-up requests.

import type { Backend } from "./backend";
import type { AssistantConsent, AssistantLogEntry, AssistantOutcome, AssistantStatus, CaptureStartOptions } from "./types";

type DemoAssistantMethods = Pick<
  Backend,
  | "assistantStatus"
  | "setAssistantAccess"
  | "assistantAnswer"
  | "assistantLog"
  | "clearAssistantLog"
  | "connectAssistantApp"
  | "onAssistantConsent"
  | "onAssistantActivity"
>;

interface DemoAssistantWorld {
  /** Starts a demo capture, as the real app does after Allow. */
  startCapture(options: CaptureStartOptions): Promise<unknown>;
  /** The id of the demo's Firefox, for the capture target. */
  firefoxId: string;
}

const MCP_PATH = String.raw`C:\Users\you\AppData\Local\Programs\Portlight\portlight-mcp.exe`;
const CLAUDE_CONFIG = String.raw`C:\Users\you\AppData\Roaming\Claude\claude_desktop_config.json`;
const CLIENT = "Claude 1.4";
const LOG_LIMIT = 300;
/** When to ask for the one consent of the demo, after access is turned on. */
const CONSENT_AFTER_MS = 8000;

/** Requests the pretend assistant makes after access is turned on: delay, tool, summary, size. */
const SCRIPT: Array<[ms: number, tool: string, summary: string, bytes: number]> = [
  [1600, "get_status", "Portlight status", 420],
  [3400, "list_apps", "Apps online now (14)", 6_800],
  [5600, "get_app", "Details of Firefox", 3_200],
  [6800, "list_connections", "Open connections of Firefox", 4_100],
  [14000, "get_history", "Firefox's history, last 7 days", 9_600],
  [21000, "list_servers", "Servers and their owners", 12_400],
  [30000, "get_app", "Details of Visual Studio Code", 2_700],
];

export function createDemoAssistant(world: DemoAssistantWorld): DemoAssistantMethods & { enabled(): boolean } {
  let enabled = false;
  let desktopConfigured = false;
  let nextLog = 1;
  let consentShown = false;
  let timers: number[] = [];
  let log: AssistantLogEntry[] = [];
  const consentListeners = new Set<(consent: AssistantConsent) => void>();
  const activityListeners = new Set<(entry: AssistantLogEntry) => void>();

  const status = (): AssistantStatus => ({
    enabled,
    mcpPath: MCP_PATH,
    claudeDesktop: { installed: true, configured: desktopConfigured, configPath: CLAUDE_CONFIG },
    claudeCodeCommand: `claude mcp add --scope user portlight -- "${MCP_PATH}"`,
    claudeCodeInstalled: true,
    claudeCodeConfigured: false,
    configSnippet: JSON.stringify({ mcpServers: { portlight: { command: MCP_PATH, args: [] } } }, null, 2),
  });

  function record(tool: string, summary: string, outcome: AssistantOutcome, bytes: number) {
    const entry: AssistantLogEntry = { id: nextLog++, time: Date.now(), client: CLIENT, tool, summary, outcome, bytes };
    log = [entry, ...log].slice(0, LOG_LIMIT);
    activityListeners.forEach((listener) => listener(entry));
  }

  const later = (ms: number, work: () => void) => timers.push(window.setTimeout(work, ms));

  function play() {
    for (const [ms, tool, summary, bytes] of SCRIPT) later(ms, () => record(tool, summary, "answered", bytes));
    later(9000, () => record("get_app", "Details of an app that is not running", "error", 180));
    // The one consent of this page load, so the prompt can be seen.
    if (!consentShown) {
      consentShown = true;
      later(CONSENT_AFTER_MS, () => {
        const consent: AssistantConsent = {
          id: 1,
          client: CLIENT,
          action: "Record packets from Firefox for 30 seconds",
          detail: "Claude wants to see which servers Firefox talks to while a page loads. The packets stay on this PC; Claude reads a summary of them. The capture stops by itself.",
        };
        consentListeners.forEach((listener) => listener(consent));
      });
    }
  }

  function stop() {
    timers.forEach((timer) => window.clearTimeout(timer));
    timers = [];
  }

  const wait = (ms: number) => new Promise((resolve) => window.setTimeout(resolve, ms));

  return {
    enabled: () => enabled,
    assistantStatus: async () => status(),
    setAssistantAccess: async (next) => {
      await wait(120);
      if (next && !enabled) play();
      if (!next) stop();
      enabled = next;
      return status();
    },
    assistantAnswer: async (_id, allow) => {
      record("start_capture", "Capture of Firefox, 30 seconds", allow ? "allowed" : "denied", 0);
      if (!allow) return;
      await world.startCapture({
        target: { kind: "app", appId: world.firefoxId, name: "Firefox" },
        interfaces: [],
        captureFilter: "",
        stopAfter: { packets: null, seconds: 30, megabytes: null },
      });
      later(32_000, () => enabled && record("read_packets", "Packets from Firefox (summary)", "answered", 18_300));
    },
    assistantLog: async () => [...log],
    clearAssistantLog: async () => {
      log = [];
    },
    connectAssistantApp: async (client, connect) => {
      await wait(700);
      if (client === "claudeDesktop") desktopConfigured = connect;
      return status();
    },
    onAssistantConsent: async (listener) => {
      consentListeners.add(listener);
      return () => void consentListeners.delete(listener);
    },
    onAssistantActivity: async (listener) => {
      activityListeners.add(listener);
      return () => void activityListeners.delete(listener);
    },
  };
}
