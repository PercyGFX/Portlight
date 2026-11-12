// A pretend PC for running the UI in a plain browser (`npm run dev`).
// Every app, name and address here is invented; the addresses come from the
// ranges reserved for documentation, so nothing real is shown or contacted.

import type { Backend } from "./backend";
import { createDemoAssistant } from "./demoAssistant";
import { createDemoCapture } from "./demoCapture";
import { createDemoHistory } from "./demoHistory";
import { demoIcon } from "./demoIcons";
import { HOME_NETWORK } from "./owners";
import type { AppSettings, AppView, CollectorStatus, ConnView, EventKind, FlowEvent, Protocol, Signature, Snapshot, Source, UpdateStatus } from "./types";

interface DemoApp {
  id: string;
  name: string;
  exe: string;
  path: string | null;
  trusted: boolean;
  /** Seconds after start when the app first goes online. */
  appearsAt: number;
  /** Most connections it keeps open at once. */
  limit: number;
  targets: Array<[host: string | null, port: number, protocol?: Protocol]>;
  /** Typical bytes per second received on one connection, for the data counters. */
  rate: number;
  /** Which made-up icon it has, if any. */
  icon: string | null;
  signature: Signature;
  /** For the made-up history: connections on a typical weekday, weekend activity, days it has been around, extra sites. */
  daily: number;
  weekend: number;
  since: number;
  extra: number;
}

const APPS: DemoApp[] = [
  {
    id: "c:\\program files\\mozilla firefox\\firefox.exe",
    name: "Firefox",
    exe: "firefox.exe",
    path: "C:\\Program Files\\Mozilla Firefox\\firefox.exe",
    trusted: true,
    appearsAt: 0,
    limit: 9,
    targets: [
      ["github.com", 443],
      ["developer.mozilla.org", 443],
      ["en.wikipedia.org", 443],
      ["video.example.net", 443],
      ["cdn.example.net", 443],
      ["news.example.org", 80],
      ["video.example.net", 443, "udp"],
    ],
    rate: 90_000,
    icon: "firefox",
    signature: { state: "signed", publisher: "Mozilla Corporation" },
    daily: 420,
    weekend: 1.3,
    since: 40,
    extra: 16,
  },
  {
    id: "c:\\apps\\code\\code.exe",
    name: "Visual Studio Code",
    exe: "Code.exe",
    path: "C:\\Apps\\Code\\Code.exe",
    trusted: true,
    appearsAt: 0,
    limit: 4,
    targets: [
      ["marketplace.example.com", 443],
      ["api.github.com", 443],
      ["update.code.example.com", 443],
    ],
    rate: 6_000,
    icon: "code",
    signature: { state: "signed", publisher: "Microsoft Corporation" },
    daily: 64,
    weekend: 0.25,
    since: 40,
    extra: 1,
  },
  {
    id: "c:\\apps\\tunes\\tunes.exe",
    name: "Tunes Player",
    exe: "tunes.exe",
    path: "C:\\Apps\\Tunes\\tunes.exe",
    trusted: true,
    appearsAt: 0,
    limit: 5,
    targets: [
      ["audio.tunes.example", 443],
      ["api.tunes.example", 443],
      [null, 4070],
      ["living-room-speaker.local", 8009],
    ],
    rate: 40_000,
    icon: "tunes",
    signature: { state: "signed", publisher: "Tunes Media Ltd." },
    daily: 90,
    weekend: 1.4,
    since: 40,
    extra: 0,
  },
  {
    id: "c:\\apps\\chatter\\chatter.exe",
    name: "Chatter",
    exe: "chatter.exe",
    path: "C:\\Apps\\Chatter\\chatter.exe",
    trusted: true,
    appearsAt: 0,
    limit: 3,
    targets: [
      ["gateway.chatter.example", 443],
      ["media.chatter.example", 443],
      ["media.chatter.example", 3478, "udp"],
    ],
    rate: 12_000,
    icon: "chatter",
    signature: { state: "signed", publisher: "Chatter Inc." },
    daily: 70,
    weekend: 1.1,
    since: 40,
    extra: 0,
  },
  {
    id: "c:\\apps\\mail\\mail.exe",
    name: "Mail",
    exe: "mail.exe",
    path: "C:\\Apps\\Mail\\mail.exe",
    trusted: true,
    appearsAt: 0,
    limit: 2,
    targets: [
      ["imap.mail.example", 993],
      ["smtp.mail.example", 587],
    ],
    rate: 2_000,
    icon: "mail",
    signature: { state: "signed", publisher: "Microsoft Corporation" },
    daily: 36,
    weekend: 0.5,
    since: 40,
    extra: 0,
  },
  {
    id: "system",
    name: "Windows system services",
    exe: "",
    path: null,
    trusted: true,
    appearsAt: 0,
    limit: 3,
    targets: [
      ["update.windows.example", 443],
      ["time.windows.example", 443],
      ["time.windows.example", 123, "udp"],
    ],
    rate: 3_000,
    icon: null,
    signature: { state: "unknown" },
    daily: 60,
    weekend: 0.9,
    since: 40,
    extra: 0,
  },
  {
    id: "c:\\games\\launcher\\launcher.exe",
    name: "Game Launcher",
    exe: "launcher.exe",
    path: "C:\\Games\\Launcher\\launcher.exe",
    trusted: true,
    appearsAt: 0,
    limit: 2,
    targets: [["store.games.example", 443]],
    rate: 25_000,
    icon: "games",
    signature: { state: "signed", publisher: "Valve Corp." },
    daily: 18,
    weekend: 3.2,
    since: 40,
    extra: 0,
  },
  {
    id: "c:\\users\\you\\downloads\\photo-sync\\photosync.exe",
    name: "Photo Sync Helper",
    exe: "photosync.exe",
    path: "C:\\Users\\you\\Downloads\\photo-sync\\photosync.exe",
    trusted: false,
    appearsAt: 6,
    limit: 2,
    targets: [
      ["upload.photosync.example", 8080],
      [null, 443],
    ],
    rate: 60_000,
    icon: "generic",
    signature: { state: "unsigned" },
    daily: 40,
    weekend: 1,
    since: 0,
    extra: 0,
  },
];

/** Who runs each made-up server's network: owner, AS number, country. */
const OWNERS: Record<string, [owner: string, asn: number, country: string]> = {
  "github.com": ["GitHub, Inc.", 36459, "US"],
  "api.github.com": ["GitHub, Inc.", 36459, "US"],
  "developer.mozilla.org": ["Google LLC", 15169, "US"],
  "en.wikipedia.org": ["Wikimedia Foundation, Inc.", 14907, "US"],
  "video.example.net": ["Akamai International B.V.", 20940, "NL"],
  "cdn.example.net": ["Fastly, Inc.", 54113, "US"],
  "news.example.org": ["OVH SAS", 16276, "FR"],
  "marketplace.example.com": ["Microsoft Corporation", 8075, "US"],
  "update.code.example.com": ["Microsoft Corporation", 8075, "US"],
  "audio.tunes.example": ["Cloudflare, Inc.", 13335, "US"],
  "api.tunes.example": ["Google LLC", 15169, "US"],
  "gateway.chatter.example": ["Cloudflare, Inc.", 13335, "US"],
  "media.chatter.example": ["Amazon.com, Inc.", 16509, "US"],
  "imap.mail.example": ["Microsoft Corporation", 8075, "US"],
  "smtp.mail.example": ["Microsoft Corporation", 8075, "US"],
  "update.windows.example": ["Microsoft Corporation", 8075, "US"],
  "time.windows.example": ["Microsoft Corporation", 8075, "US"],
  "store.games.example": ["Valve Corporation", 32590, "US"],
  "upload.photosync.example": ["Hetzner Online GmbH", 24940, "DE"],
  "c:\\apps\\tunes\\tunes.exe:4070": ["Hetzner Online GmbH", 24940, "DE"],
  "c:\\users\\you\\downloads\\photo-sync\\photosync.exe:443": ["OVH SAS", 16276, "FR"],
};

/** Networks for the extra sites in the made-up history. */
const POOL: Array<[owner: string, asn: number, country: string]> = [
  ["Cloudflare, Inc.", 13335, "US"],
  ["Google LLC", 15169, "US"],
  ["Fastly, Inc.", 54113, "US"],
  ["Amazon.com, Inc.", 16509, "US"],
  ["Akamai International B.V.", 20940, "NL"],
  ["Hetzner Online GmbH", 24940, "DE"],
  ["OVH SAS", 16276, "FR"],
  ["DigitalOcean, LLC", 14061, "US"],
];

/** Owner fields for a host (or `appId:port` when it has no name). */
function ownerOf(host: string | null, key: string): Pick<ConnView, "owner" | "asn" | "country"> {
  if (host?.endsWith(".local")) return { owner: HOME_NETWORK, asn: null, country: null };
  let entry = OWNERS[host ?? key];
  if (!entry && host) {
    let hash = 0;
    for (const ch of host) hash = (hash * 31 + ch.charCodeAt(0)) >>> 0;
    entry = POOL[hash % POOL.length];
  }
  return entry ? { owner: entry[0], asn: entry[1], country: entry[2] } : { owner: null, asn: null, country: null };
}

/** How long a signature check takes in the demo. */
const SIGNATURE_MS = 1600;

const TICK_MS = 500;
const DOC_RANGES = ["203.0.113", "198.51.100", "192.0.2"];

/** Small repeatable random-number generator, so the demo plays the same every time. */
function seeded(seed: number): () => number {
  let state = seed;
  return () => {
    state = (state * 1664525 + 1013904223) >>> 0;
    return state / 0x100000000;
  };
}

/** A stable made-up address for a host name. Names ending in ".local" live on the home network. */
function addressFor(key: string): string {
  let hash = 0;
  for (const ch of key) hash = (hash * 31 + ch.charCodeAt(0)) >>> 0;
  if (key.endsWith(".local")) return `192.168.1.${(hash % 200) + 30}`;
  return `${DOC_RANGES[hash % DOC_RANGES.length]}.${(hash >>> 8) % 250 + 2}`;
}

/** `?mode=basic` starts the demo in basic mode, to see what that looks like. */
function initialSource(): Source {
  return new URLSearchParams(window.location.search).get("mode") === "basic" ? "basic" : "detailed";
}

const wait = (ms: number) => new Promise((resolve) => window.setTimeout(resolve, ms));

export function createDemoBackend(): Backend {
  const random = seeded(7);
  const started = Date.now();
  const firstSeen = new Map<string, number>();
  const trusted = new Map<string, boolean>(APPS.map((app) => [app.id, app.trusted]));
  const totalOpened = new Map<string, number>();
  /** Bytes of connections already closed, per app. */
  const closedBytes = new Map<string, [sent: number, received: number]>();
  const events: FlowEvent[] = [];
  let connections: ConnView[] = [];
  let nextConn = 1;
  let nextEvent = 1;
  let source = initialSource();
  /** "Learn again" in Settings starts a fresh learning period. */
  let learningUntil = 0;
  const settings: AppSettings = {
    autoUpdate: true,
    historyEnabled: true,
    historyKeepDays: 90,
    closeToTray: true,
    notifyNewApps: true,
    reverseLookups: true,
    startAtSignIn: false,
    canStartAtSignIn: false,
    assistantAccess: false,
  };

  const pick = <T,>(items: T[]): T => items[Math.floor(random() * items.length)];
  const online = (app: DemoApp, now: number) => now - started >= app.appearsAt * 1000;

  function open(app: DemoApp, now: number, quiet = false) {
    const [host, port, protocol = "tcp"] = pick(app.targets);
    const key = `${app.id}:${port}`;
    const remoteIp = addressFor(host ?? key);
    if (!firstSeen.has(app.id)) firstSeen.set(app.id, quiet ? now - 86_400_000 * 9 : now);
    // Connections open at the start already have some history behind them.
    const history = quiet ? 20 + random() * 400 : 0;
    const conn: ConnView = {
      id: nextConn++,
      appId: app.id,
      pid: 1000 + APPS.indexOf(app) * 412,
      protocol,
      remoteIp,
      remotePort: port,
      localPort: 49152 + Math.floor(random() * 16000),
      host,
      openedAt: now,
      bytesSent: Math.round(history * app.rate * 0.08),
      bytesReceived: Math.round(history * app.rate),
      ...ownerOf(host, key),
    };
    connections.push(conn);
    totalOpened.set(app.id, (totalOpened.get(app.id) ?? 0) + 1);
    if (!quiet) record("opened", app, conn, now);
  }

  function record(kind: EventKind, app: DemoApp, conn: ConnView, now: number) {
    events.push({
      seq: nextEvent++,
      time: now,
      kind,
      appId: app.id,
      appName: app.name,
      protocol: conn.protocol,
      remoteIp: conn.remoteIp,
      remotePort: conn.remotePort,
      host: conn.host,
      bytesSent: kind === "closed" ? conn.bytesSent : 0,
      bytesReceived: kind === "closed" ? conn.bytesReceived : 0,
    });
    if (events.length > 300) events.shift();
  }

  function step(now: number) {
    const seconds = TICK_MS / 1000;
    for (const app of APPS) {
      if (!online(app, now)) continue;
      const mine = connections.filter((c) => c.appId === app.id);
      // Data comes in bursts: each connection moves between nothing and twice the usual rate.
      for (const conn of mine) {
        const burst = random() < 0.35 ? random() * 2 : random() * 0.2;
        conn.bytesReceived += Math.round(app.rate * burst * seconds);
        conn.bytesSent += Math.round(app.rate * burst * seconds * (0.04 + random() * 0.1));
      }
      if (!firstSeen.has(app.id)) {
        open(app, now);
      } else if (mine.length < app.limit && random() < 0.16) {
        open(app, now);
      } else if (mine.length > 1 && random() < 0.1) {
        const closing = pick(mine);
        connections = connections.filter((c) => c !== closing);
        const [sent, received] = closedBytes.get(app.id) ?? [0, 0];
        closedBytes.set(app.id, [sent + closing.bytesSent, received + closing.bytesReceived]);
        record("closed", app, closing, now);
      }
    }
  }

  // Basic mode only sees TCP and never counts bytes, like the real TCP table.
  const asSeen = <T extends { bytesSent: number; bytesReceived: number }>(item: T): T =>
    source === "detailed" ? { ...item } : { ...item, bytesSent: 0, bytesReceived: 0 };

  function snapshot(now: number, eventsAfter: number): Snapshot {
    const visible = connections.filter((c) => source === "detailed" || c.protocol === "tcp");
    const apps: AppView[] = APPS.filter((app) => firstSeen.has(app.id)).map((app) => {
      const mine = visible.filter((c) => c.appId === app.id);
      const all = connections.filter((c) => c.appId === app.id);
      const [closedSent, closedReceived] = closedBytes.get(app.id) ?? [0, 0];
      return asSeen({
        id: app.id,
        name: app.name,
        exe: app.exe,
        path: app.path,
        firstSeen: firstSeen.get(app.id) ?? now,
        trusted: trusted.get(app.id) ?? true,
        connections: mine.length,
        servers: new Set(mine.map((c) => c.remoteIp)).size,
        totalOpened: totalOpened.get(app.id) ?? 0,
        bytesSent: closedSent + all.reduce((sum, c) => sum + c.bytesSent, 0),
        bytesReceived: closedReceived + all.reduce((sum, c) => sum + c.bytesReceived, 0),
        // The signature is checked a moment after the app first shows up.
        signature: now - Math.max(started, firstSeen.get(app.id) ?? now) < SIGNATURE_MS ? { state: "unknown" } : app.signature,
      });
    });
    apps.sort((a, b) => b.connections - a.connections || a.name.localeCompare(b.name));
    return {
      now,
      learningSecondsLeft: Math.max(0, Math.ceil((learningUntil - now) / 1000)),
      source,
      apps,
      connections: visible.map(asSeen),
      events: events
        .filter((e) => e.seq > eventsAfter && (source === "detailed" || e.protocol === "tcp"))
        .map(asSeen),
    };
  }

  const update: UpdateStatus = {
    current: "0.4.0",
    autoUpdate: true,
    canInstall: false,
    availableVersion: null,
    releasePage: null,
  };

  const collectorListeners = new Set<(status: CollectorStatus) => void>();
  const collector = (): CollectorStatus => ({
    installed: source === "detailed",
    connected: source === "detailed",
    version: source === "detailed" ? "0.4.0" : null,
    outdated: false,
    canInstall: true,
    error: null,
  });
  // Pretends to wait for the administrator prompt, then switches mode.
  async function switchSource(next: Source): Promise<CollectorStatus> {
    await wait(1400);
    source = next;
    const status = collector();
    collectorListeners.forEach((listener) => listener(status));
    return status;
  }

  // The starting picture: every app that is online from the start, already busy.
  for (const app of APPS.filter((a) => a.appearsAt === 0)) {
    const count = Math.max(1, Math.round(app.limit * 0.6));
    for (let i = 0; i < count; i++) open(app, started, true);
  }

  const history = createDemoHistory(
    APPS.map((app) => ({
      id: app.id,
      name: app.name,
      targets: app.targets,
      daily: app.daily,
      weekend: app.weekend,
      since: app.since,
      perConnection: app.rate * 45,
      extra: app.extra,
    })),
    (host, key) => {
      const ip = addressFor(host ?? key);
      const { owner, country } = ownerOf(host, key);
      return { ip, owner, country };
    },
  );

  const capture = createDemoCapture({
    connections: () => connections,
    appName: (id) => APPS.find((app) => app.id === id)?.name ?? id,
  });
  const { enabled: assistantEnabled, ...assistant } = createDemoAssistant({
    startCapture: capture.captureStart,
    firefoxId: APPS[0].id,
  });

  return {
    demo: true,
    getSnapshot: async () => snapshot(Date.now(), 0),
    onTick: async (onTick) => {
      let last = nextEvent - 1;
      const timer = window.setInterval(() => {
        const now = Date.now();
        step(now);
        onTick(snapshot(now, last));
        last = nextEvent - 1;
      }, TICK_MS);
      return () => window.clearInterval(timer);
    },
    trustApp: async (id) => void trusted.set(id, true),
    trustAll: async () => trusted.forEach((_, id) => trusted.set(id, true)),
    updateStatus: async () => ({ ...update }),
    onUpdateStatus: async () => () => {},
    checkForUpdate: async () => ({ ...update }),
    setAutoUpdate: async (enabled) => {
      update.autoUpdate = enabled;
      return { ...update };
    },
    installUpdate: async () => {},

    collectorStatus: async () => collector(),
    installCollector: () => switchSource("detailed"),
    uninstallCollector: () => switchSource("basic"),
    onCollectorStatus: async (listener) => {
      collectorListeners.add(listener);
      return () => void collectorListeners.delete(listener);
    },

    exportLive: async (what) => {
      await wait(350);
      const stamp = new Date().toISOString().slice(0, 16).replace(/[-:T]/g, "");
      const name = what === "flow" ? "connection-history" : what;
      return `C:\\Users\\you\\Documents\\Portlight\\${name}-${stamp}.csv`;
    },

    appIcon: async (appId) => {
      await wait(120 + random() * 260);
      return demoIcon(APPS.find((app) => app.id === appId)?.icon ?? null);
    },
    ...history,

    getSettings: async () => ({ ...settings, autoUpdate: update.autoUpdate, assistantAccess: assistantEnabled() }),
    setSettings: async (change) => {
      await wait(120);
      Object.assign(settings, change);
      if (change.autoUpdate !== undefined) update.autoUpdate = change.autoUpdate;
      return { ...settings, autoUpdate: update.autoUpdate, assistantAccess: assistantEnabled() };
    },
    learnAgain: async () => {
      trusted.forEach((_, id) => trusted.set(id, true));
      learningUntil = Date.now() + 120_000;
    },
    forgetKnownApps: async () => {
      // What is online now stays trusted; the rest would alert again.
      const online = new Set(connections.map((c) => c.appId));
      trusted.forEach((_, id) => trusted.set(id, online.has(id)));
      learningUntil = Date.now() + 120_000;
    },
    about: async () => ({
      version: update.current,
      authors: "PercyGFX",
      repository: "https://github.com/PercyGFX/portlight",
      license: "MIT",
      ownersBuiltAt: new Date(2026, 10, 12).getTime(),
      ownerRanges: 500_337,
      dataFolder: String.raw`C:\Users\you\AppData\Roaming\com.portlight.app`,
    }),
    openLink: async () => {},

    ...capture,
    ...assistant,
  };
}
