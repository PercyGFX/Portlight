// Thirty days of made-up history for the demo, the same on every run of the same day.

import type { HistoryAppDay, HistoryDay, HistoryServer, HistorySettings, Protocol } from "./types";

export interface HistoryApp {
  id: string;
  name: string;
  targets: Array<[host: string | null, port: number, protocol?: Protocol]>;
  /** Typical connections on a weekday. */
  daily: number;
  /** Weekend activity compared to a weekday. */
  weekend: number;
  /** Days back the app has been around; 0 = only today. */
  since: number;
  /** Typical bytes received per connection. */
  perConnection: number;
  /** Extra servers on a busy day, beyond the usual ones. */
  extra: number;
}

export interface ServerInfo {
  ip: string;
  owner: string | null;
  country: string | null;
}

const DAY_MS = 86_400_000;
const KEEP_CHOICES = [7, 30, 90, 365];

/** Extra sites a browser visits now and then. */
const EXTRA_SITES = [
  "static.example.net",
  "fonts.example.com",
  "img.news.example.org",
  "shop.example.com",
  "maps.example.com",
  "docs.example.org",
  "forum.example.net",
  "weather.example.com",
  "bank.example.com",
  "mail.example.org",
  "recipes.example.net",
  "travel.example.com",
  "analytics.example.net",
  "ads.example.com",
  "cdn2.example.net",
  "stream.example.org",
  "blog.example.com",
  "api.example.org",
];

/** Small repeatable random-number generator. */
function seeded(seed: number): () => number {
  let state = seed >>> 0;
  return () => {
    state = (state * 1664525 + 1013904223) >>> 0;
    return state / 0x100000000;
  };
}

function hash(text: string): number {
  let value = 2166136261;
  for (const ch of text) value = Math.imul(value ^ ch.charCodeAt(0), 16777619) >>> 0;
  return value;
}

const pad = (n: number) => String(n).padStart(2, "0");

/** "2026-11-12" in local time. */
function dayKey(date: Date): string {
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`;
}

/** Local midnight `offset` days ago. */
function midnight(offset: number): Date {
  const date = new Date();
  date.setHours(0, 0, 0, 0);
  date.setDate(date.getDate() - offset);
  return date;
}

export function createDemoHistory(apps: HistoryApp[], serverInfo: (host: string | null, key: string) => ServerInfo) {
  let enabled = true;
  let keepDays = 90;
  /** Oldest day offset still kept; -1 after clearing. */
  let oldest = 29;

  function build(app: HistoryApp, offset: number): HistoryDay | null {
    if (offset > app.since || offset > oldest) return null;
    const start = midnight(offset);
    const epochDay = Math.floor((start.getTime() - start.getTimezoneOffset() * 60_000) / DAY_MS);
    const random = seeded(hash(app.id) ^ (epochDay * 2654435761));
    const weekend = start.getDay() === 0 || start.getDay() === 6;
    const factor = weekend ? app.weekend : 1;
    // Quiet days: weekday-only apps rest at weekends, and everything has an odd day off.
    if (factor === 0 || random() < 0.08) return null;

    const now = Date.now();
    const today = offset === 0;
    const sofar = today ? Math.max(0.12, (now - start.getTime()) / DAY_MS) : 1;
    const total = Math.max(1, Math.round(app.daily * factor * (0.55 + random() * 0.9) * sofar));

    const hosts: Array<[string | null, number, Protocol]> = app.targets.map(([host, port, protocol = "tcp"]) => [host, port, protocol]);
    const extras = Math.round(app.extra * (0.4 + random() * 0.6) * Math.min(1, sofar * 1.6));
    for (let i = 0; i < extras; i++) hosts.push([EXTRA_SITES[(hash(app.id) + epochDay * 7 + i * 5) % EXTRA_SITES.length], 443, "tcp"]);
    const unique = hosts.filter((h, i) => hosts.findIndex((o) => o[0] === h[0] && o[1] === h[1] && o[2] === h[2]) === i);

    // Earlier servers are the usual ones and get the most connections.
    const weights = unique.map((_, i) => (i < app.targets.length ? 6 / (1 + i * 0.6) : 0.6) * (0.5 + random()));
    const weightSum = weights.reduce((a, b) => a + b, 0);
    const firstHour = 7 + random() * 3;
    const lastHour = Math.min(23.5, 17 + random() * 6);
    const end = today ? now : start.getTime() + lastHour * 3_600_000;

    const servers: HistoryServer[] = unique.map(([host, port, protocol], i) => {
      const connections = Math.max(1, Math.round((total * weights[i]) / weightSum));
      const received = Math.round(connections * app.perConnection * (0.4 + random() * 1.2));
      const info = serverInfo(host, `${app.id}:${port}`);
      const first = Math.min(end - 60_000, start.getTime() + (firstHour + random() * 2) * 3_600_000);
      const last = Math.max(first + 60_000, end - random() * 3 * 3_600_000);
      return {
        name: host ?? info.ip,
        ip: info.ip,
        port,
        protocol,
        connections,
        bytesSent: Math.round(received * (0.05 + random() * 0.08)),
        bytesReceived: received,
        owner: info.owner,
        country: info.country,
        firstSeen: first,
        lastSeen: Math.min(last, end),
      };
    });
    servers.sort((a, b) => b.connections - a.connections || b.bytesReceived - a.bytesReceived);

    return {
      day: dayKey(start),
      connections: servers.reduce((sum, s) => sum + s.connections, 0),
      bytesSent: servers.reduce((sum, s) => sum + s.bytesSent, 0),
      bytesReceived: servers.reduce((sum, s) => sum + s.bytesReceived, 0),
      servers: servers.slice(0, 200),
    };
  }

  function days(app: HistoryApp, count: number): HistoryDay[] {
    const result: HistoryDay[] = [];
    for (let offset = 0; offset < count; offset++) {
      const day = build(app, offset);
      if (day) result.push(day);
    }
    return result;
  }

  const settings = (): HistorySettings => ({
    enabled,
    keepDays,
    sizeBytes: oldest < 0 ? 24_576 : 24_576 + (oldest + 1) * 118_000,
    oldestDay: oldest < 0 ? null : dayKey(midnight(oldest)),
  });

  return {
    appHistory: async (appId: string, count: number): Promise<HistoryDay[]> => {
      const app = apps.find((a) => a.id === appId);
      return app ? days(app, Math.min(count, keepDays)) : [];
    },
    historySummary: async (count: number): Promise<HistoryAppDay[]> =>
      apps.flatMap((app) =>
        days(app, Math.min(count, keepDays)).map((day) => ({
          day: day.day,
          appId: app.id,
          appName: app.name,
          connections: day.connections,
          servers: day.servers.length,
          bytesSent: day.bytesSent,
          bytesReceived: day.bytesReceived,
        })),
      ),
    historySettings: async () => settings(),
    setHistorySettings: async (on: boolean, keep: number) => {
      enabled = on;
      keepDays = KEEP_CHOICES.includes(keep) ? keep : 90;
      oldest = Math.min(oldest, keepDays - 1);
      return settings();
    },
    clearHistory: async () => {
      oldest = -1;
      return settings();
    },
  };
}
