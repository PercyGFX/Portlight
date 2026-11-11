// Capture targets in words: titles, short labels and what will be captured.

import { serviceOf } from "./services";
import type { CaptureTarget, ConnView } from "./types";

export const EVERYTHING: CaptureTarget = { kind: "all" };

/** The page title, e.g. "Packets from Firefox". */
export function targetTitle(target: CaptureTarget): string {
  switch (target.kind) {
    case "all":
      return "Everything on this PC's network";
    case "app":
      return `Packets from ${target.name}`;
    case "host":
      return `Packets to and from ${target.name ?? target.ip}`;
    case "domain":
      return `Packets to and from ${target.name}`;
    case "connection":
      return `One connection: ${target.appName} ↔ ${target.host ?? target.remoteIp}:${target.remotePort}`;
  }
}

/** A few words for buttons and pills, e.g. "github.com". */
export function targetLabel(target: CaptureTarget): string {
  switch (target.kind) {
    case "all":
      return "Everything";
    case "app":
      return target.name;
    case "host":
      return target.name ?? target.ip;
    case "domain":
      return target.name;
    case "connection":
      return `${target.appName} ↔ ${target.host ?? target.remoteIp}`;
  }
}

/** What pressing Start will record, in a sentence. */
export function targetDescription(target: CaptureTarget): string {
  switch (target.kind) {
    case "all":
      return "Every packet on the chosen network cards: every app on this PC, plus the chatter of your home network (the router, phones, TVs).";
    case "app":
      return `Every packet ${target.name} sends or receives, on every connection it opens while Portlight is recording.`;
    case "host":
      return `Every packet between this PC and ${target.name ? `${target.name} (${target.ip})` : target.ip}, from any app.`;
    case "domain":
      return `Every packet between this PC and ${target.name}, from any app. Portlight follows every address ${target.name} uses.`;
    case "connection": {
      const service = serviceOf(target.remotePort).label;
      return `Only this one conversation: ${target.appName} (port ${target.localPort} on this PC) with ${target.host ?? target.remoteIp} on port ${target.remotePort} (${service}, ${target.protocol.toUpperCase()}).`;
    }
  }
}

/** The best target for a server: its name when known, else its address. */
export function serverTarget(ip: string, host: string | null | undefined): CaptureTarget {
  return host ? { kind: "domain", name: host } : { kind: "host", ip, name: null };
}

export function connectionTarget(conn: ConnView, appName: string, host: string | null | undefined): CaptureTarget {
  return {
    kind: "connection",
    protocol: conn.protocol,
    localPort: conn.localPort,
    remoteIp: conn.remoteIp,
    remotePort: conn.remotePort,
    appName,
    host: host ?? null,
  };
}

/** Same target, whatever order the fields arrived in. */
export function sameTarget(a: CaptureTarget | null, b: CaptureTarget | null): boolean {
  if (!a || !b) return a === b;
  const keys = new Set([...Object.keys(a), ...Object.keys(b)]);
  const left = a as unknown as Record<string, unknown>;
  const right = b as unknown as Record<string, unknown>;
  return [...keys].every((key) => (left[key] ?? null) === (right[key] ?? null));
}

/** A typed site name or address as a target: digits and dots (or colons) are an address. */
export function typedTarget(text: string): CaptureTarget | null {
  const value = text.trim().toLowerCase();
  if (!value) return null;
  if (/^[\d.]+$/.test(value) || value.includes(":")) return { kind: "host", ip: value, name: null };
  return { kind: "domain", name: value };
}
