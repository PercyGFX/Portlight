// A pretend PC for running the UI in a plain browser (`npm run dev`).
// Every app, name and address here is invented; the addresses come from the
// ranges reserved for documentation, so nothing real is shown or contacted.

import type { Backend } from "./backend";
import type { AppView, ConnView, EventKind, FlowEvent, Snapshot, UpdateStatus } from "./types";

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
  targets: Array<[host: string | null, port: number]>;
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
    ],
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
    ],
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
    ],
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
    ],
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
  },
];

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

/** A stable made-up address for a host name. */
function addressFor(key: string): string {
  let hash = 0;
  for (const ch of key) hash = (hash * 31 + ch.charCodeAt(0)) >>> 0;
  return `${DOC_RANGES[hash % DOC_RANGES.length]}.${(hash >>> 8) % 250 + 2}`;
}

export function createDemoBackend(): Backend {
  const random = seeded(7);
  const started = Date.now();
  const firstSeen = new Map<string, number>();
  const trusted = new Map<string, boolean>(APPS.map((app) => [app.id, app.trusted]));
  const totalOpened = new Map<string, number>();
  const events: FlowEvent[] = [];
  let connections: ConnView[] = [];
  let nextConn = 1;
  let nextEvent = 1;

  const pick = <T,>(items: T[]): T => items[Math.floor(random() * items.length)];
  const online = (app: DemoApp, now: number) => now - started >= app.appearsAt * 1000;

  function open(app: DemoApp, now: number, quiet = false) {
    const [host, port] = pick(app.targets);
    const remoteIp = addressFor(host ?? `${app.id}:${port}`);
    if (!firstSeen.has(app.id)) firstSeen.set(app.id, quiet ? now - 86_400_000 * 9 : now);
    connections.push({
      id: nextConn++,
      appId: app.id,
      pid: 1000 + APPS.indexOf(app) * 412,
      remoteIp,
      remotePort: port,
      localPort: 49152 + Math.floor(random() * 16000),
      host,
      openedAt: now,
    });
    totalOpened.set(app.id, (totalOpened.get(app.id) ?? 0) + 1);
    if (!quiet) record("opened", app, remoteIp, port, host, now);
  }

  function record(kind: EventKind, app: DemoApp, remoteIp: string, remotePort: number, host: string | null, now: number) {
    events.push({ seq: nextEvent++, time: now, kind, appId: app.id, appName: app.name, remoteIp, remotePort, host });
    if (events.length > 300) events.shift();
  }

  function step(now: number) {
    for (const app of APPS) {
      if (!online(app, now)) continue;
      const mine = connections.filter((c) => c.appId === app.id);
      if (!firstSeen.has(app.id)) {
        open(app, now);
      } else if (mine.length < app.limit && random() < 0.16) {
        open(app, now);
      } else if (mine.length > 1 && random() < 0.1) {
        const closing = pick(mine);
        connections = connections.filter((c) => c !== closing);
        record("closed", app, closing.remoteIp, closing.remotePort, closing.host, now);
      }
    }
  }

  function snapshot(now: number, eventsAfter: number): Snapshot {
    const apps: AppView[] = APPS.filter((app) => firstSeen.has(app.id)).map((app) => {
      const mine = connections.filter((c) => c.appId === app.id);
      return {
        id: app.id,
        name: app.name,
        exe: app.exe,
        path: app.path,
        firstSeen: firstSeen.get(app.id) ?? now,
        trusted: trusted.get(app.id) ?? true,
        connections: mine.length,
        servers: new Set(mine.map((c) => c.remoteIp)).size,
        totalOpened: totalOpened.get(app.id) ?? 0,
      };
    });
    apps.sort((a, b) => b.connections - a.connections || a.name.localeCompare(b.name));
    return {
      now,
      learningSecondsLeft: 0,
      apps,
      connections: [...connections],
      events: events.filter((e) => e.seq > eventsAfter),
    };
  }

  const update: UpdateStatus = {
    current: "0.2.0",
    autoUpdate: true,
    canInstall: false,
    availableVersion: null,
    releasePage: null,
  };

  // The starting picture: every app that is online from the start, already busy.
  for (const app of APPS.filter((a) => a.appearsAt === 0)) {
    const count = Math.max(1, Math.round(app.limit * 0.6));
    for (let i = 0; i < count; i++) open(app, started, true);
  }

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
  };
}
