// The live map: apps on the left, this PC in the middle, the internet and the
// servers on the right, drawn over sonar rings that spread from this PC.
// A dot travels along the lanes whenever a connection opens (outwards) or
// closes (back home). Servers are grouped by site name, so one site with many
// addresses is one node. Devices on the home network are reached directly,
// not through the internet. In detailed mode, busy lanes are thicker and show
// how fast data is flowing.

import { useCallback, useEffect, useMemo, useRef, useState, type KeyboardEvent, type PointerEvent } from "react";

import { useAppIcon } from "../hooks/useAppIcon";
import { useDismiss } from "../hooks/useDismiss";
import { isLocalAddress } from "../lib/addresses";
import { plural, rate, shorten } from "../lib/format";
import { ownerLine, ownerTitle, type Owner } from "../lib/owners";
import { serviceOf } from "../lib/services";
import { serverTarget } from "../lib/targets";
import type { AppView, CaptureTarget, ConnView, EventKind, FlowEvent } from "../lib/types";
import { AppIcon } from "./AppIcon";
import { RecordIcon } from "./icons";
import { AppTile, Cloud, Laptop, Server } from "./iso";

const VIEW = { width: 840, height: 470 };
const MID_Y = 240;
const APP_X = 200;
const PC = { x: 368, y: MID_Y };
const CLOUD = { x: 518, y: MID_Y - 4 };
const SERVER_X = 638;
const MAX_APPS = 6;
const MAX_SERVERS = 5;
/** Longest label that fits beside a node. */
const LABEL_CHARS = 21;
/** The owner line is in smaller type, so a little more fits. */
const OWNER_CHARS = 27;
/** Most dots launched from one update, so a burst stays readable. */
const MAX_BURST = 10;
/** Most "data is flowing" dots added per second on busy lanes. */
const MAX_FLOW_DOTS = 3;
/** Below this, a lane is not called busy (bytes per second). */
const BUSY_RATE = 20_000;
/** Sonar rings around this PC. */
const RINGS = [70, 140, 210, 280, 350, 420];

type Point = [x: number, y: number];

// Where lanes meet the drawings.
const PC_IN: Point = [PC.x - 54, PC.y + 6];
const PC_OUT: Point = [PC.x + 52, PC.y + 2];
const PC_LOCAL: Point = [PC.x + 44, PC.y + 22];
const CLOUD_IN: Point = [CLOUD.x - 48, CLOUD.y + 6];
const CLOUD_OUT: Point = [CLOUD.x + 50, CLOUD.y + 6];
const appAnchor = (y: number): Point => [APP_X + 28, y];
const serverAnchor = (y: number): Point => [SERVER_X - 30, y];

interface ServerGroup {
  /** The site name when known, else the address. */
  key: string;
  host: string | null;
  ips: string[];
  count: number;
  ports: number[];
  /** On the home network, not the internet. */
  local: boolean;
  /** Who runs the network, when known. */
  owner: Owner | null;
}

interface PacketSpec {
  id: number;
  kind: EventKind | "data";
  points: Point[];
  delay: number;
}

interface Props {
  ready: boolean;
  apps: AppView[];
  connections: ConnView[];
  events: FlowEvent[];
  hosts: ReadonlyMap<string, string>;
  selectedId: string | null;
  paused: boolean;
  onSelect(id: string | null): void;
  onCapture(target: CaptureTarget): void;
}

/** Y positions for `count` items stacked around the middle of the map. */
function stack(count: number, spacing: number): number[] {
  return Array.from({ length: count }, (_, i) => MID_Y + (i - (count - 1) / 2) * spacing);
}

/** Splits a list into the items to draw and the ones left over. */
function fit<T>(items: T[], max: number): { shown: T[]; hidden: T[] } {
  if (items.length <= max) return { shown: items, hidden: [] };
  return { shown: items.slice(0, max - 1), hidden: items.slice(max - 1) };
}

/** An S-shaped lane: leaves and arrives level, bends in between. */
function curve([x1, y1]: Point, [x2, y2]: Point): string {
  const mid = (x1 + x2) / 2;
  return `M${x1} ${y1} C${mid} ${y1} ${mid} ${y2} ${x2} ${y2}`;
}

/** Points along a cubic curve, for a dot to follow. */
function sample(p0: Point, p1: Point, p2: Point, p3: Point, steps = 12): Point[] {
  return Array.from({ length: steps + 1 }, (_, i) => {
    const t = i / steps;
    const u = 1 - t;
    const a = u * u * u;
    const b = 3 * u * u * t;
    const c = 3 * u * t * t;
    const d = t * t * t;
    return [a * p0[0] + b * p1[0] + c * p2[0] + d * p3[0], a * p0[1] + b * p1[1] + c * p2[1] + d * p3[1]];
  });
}

function alongCurve(from: Point, to: Point): Point[] {
  const mid = (from[0] + to[0]) / 2;
  return sample(from, [mid, from[1]], [mid, to[1]], to, 10);
}

/** The lane to a home-network device: it dips under the internet cloud instead of going through it. */
function localControls(to: Point): [Point, Point] {
  return [
    [CLOUD.x - 40, PC.y + 96],
    [SERVER_X - 120, to[1] + 10],
  ];
}

function localPath(to: Point): string {
  const [c1, c2] = localControls(to);
  return `M${PC_LOCAL[0]} ${PC_LOCAL[1]} C${c1[0]} ${c1[1]} ${c2[0]} ${c2[1]} ${to[0]} ${to[1]}`;
}

function alongLocal(to: Point): Point[] {
  const [c1, c2] = localControls(to);
  return sample(PC_LOCAL, c1, c2, to, 14);
}

function groupServers(connections: ConnView[], keyOf: (conn: ConnView) => string, hosts: ReadonlyMap<string, string>): ServerGroup[] {
  const groups = new Map<string, ServerGroup>();
  for (const conn of connections) {
    const key = keyOf(conn);
    const group = groups.get(key) ?? { key, host: null, ips: [], count: 0, ports: [], local: true, owner: null };
    group.count += 1;
    group.host = conn.host ?? hosts.get(conn.remoteIp) ?? group.host;
    if (!group.ips.includes(conn.remoteIp)) group.ips.push(conn.remoteIp);
    if (!group.ports.includes(conn.remotePort)) group.ports.push(conn.remotePort);
    group.local = group.local && isLocalAddress(conn.remoteIp);
    if (!group.owner && (conn.owner || conn.country)) group.owner = { owner: conn.owner, country: conn.country };
    groups.set(key, group);
  }
  return [...groups.values()].sort((a, b) => b.count - a.count || a.key.localeCompare(b.key));
}

interface Rates {
  /** Bytes per second down, and in total, by app id and by server group. */
  appDown: Map<string, number>;
  appTotal: Map<string, number>;
  groupDown: Map<string, number>;
  groupTotal: Map<string, number>;
  /** Per connection, for the "data is flowing" dots. */
  byConn: Map<number, number>;
}

/** Data rates from the change in byte counts between snapshots, smoothed so bursts stay readable. */
function useRates(connections: ConnView[], keyOf: (conn: ConnView) => string): Rates {
  const memory = useRef(new Map<number, { total: number; down: number; time: number; rate: number; downRate: number }>());
  return useMemo(() => {
    const now = performance.now();
    const next = new Map<number, { total: number; down: number; time: number; rate: number; downRate: number }>();
    const rates: Rates = { appDown: new Map(), appTotal: new Map(), groupDown: new Map(), groupTotal: new Map(), byConn: new Map() };
    const add = (map: Map<string, number>, key: string, value: number) => map.set(key, (map.get(key) ?? 0) + value);
    for (const conn of connections) {
      const total = conn.bytesSent + conn.bytesReceived;
      const prev = memory.current.get(conn.id);
      let value = 0;
      let downValue = 0;
      if (prev) {
        const seconds = (now - prev.time) / 1000;
        // Two calls in a row (React may render twice) keep the last figure.
        const raw = seconds > 0.05 ? Math.max(0, total - prev.total) / seconds : prev.rate;
        const rawDown = seconds > 0.05 ? Math.max(0, conn.bytesReceived - prev.down) / seconds : prev.downRate;
        value = prev.rate + (raw - prev.rate) * 0.35;
        downValue = prev.downRate + (rawDown - prev.downRate) * 0.35;
      }
      const keep = prev && (now - prev.time) / 1000 <= 0.05;
      next.set(conn.id, keep && prev ? { ...prev, rate: value, downRate: downValue } : { total, down: conn.bytesReceived, time: now, rate: value, downRate: downValue });
      rates.byConn.set(conn.id, value);
      add(rates.appTotal, conn.appId, value);
      add(rates.appDown, conn.appId, downValue);
      add(rates.groupTotal, keyOf(conn), value);
      add(rates.groupDown, keyOf(conn), downValue);
    }
    memory.current = next;
    return rates;
  }, [connections, keyOf]);
}

/** Lane thickness for a data rate. */
function laneWidth(bytesPerSecond: number): number | undefined {
  if (bytesPerSecond < 2000) return undefined;
  return 2.4 + Math.min(3.2, Math.log10(1 + bytesPerSecond / 2000) * 1.4);
}

export function LiveMap({ ready, apps, connections, events, hosts, selectedId, paused, onSelect, onCapture }: Props) {
  const selected = apps.find((app) => app.id === selectedId) ?? null;
  const [more, setMore] = useState<"apps" | "servers" | null>(null);
  const popover = useRef<HTMLDivElement>(null);
  useDismiss(popover, more !== null, useCallback(() => setMore(null), []));

  // One site with many addresses is one server; unnamed ones go by address.
  const hostsRef = useRef(hosts);
  hostsRef.current = hosts;
  const keyOf = useCallback((conn: { remoteIp: string; host: string | null }) => conn.host ?? hostsRef.current.get(conn.remoteIp) ?? conn.remoteIp, []);
  const rates = useRates(connections, keyOf);

  // Apps: the busiest ones, always including the selected one.
  const appSlots = useMemo(() => {
    const online = apps.filter((app) => app.connections > 0 || app.id === selectedId);
    const { shown, hidden } = fit(online, MAX_APPS);
    if (selected && !shown.includes(selected)) {
      hidden.unshift(shown[shown.length - 1]);
      shown[shown.length - 1] = selected;
      hidden.splice(hidden.indexOf(selected), 1);
    }
    const ys = stack(shown.length + (hidden.length ? 1 : 0), 66);
    return { nodes: shown.map((app, i) => ({ app, y: ys[i] })), hidden, moreY: ys[shown.length] };
  }, [apps, selected, selectedId]);

  // Servers: the selected app's, or the busiest overall. Home-network devices go last, under the cloud.
  const serverSlots = useMemo(() => {
    const relevant = selectedId ? connections.filter((c) => c.appId === selectedId) : connections;
    const { shown, hidden } = fit(groupServers(relevant, keyOf, hosts), MAX_SERVERS);
    shown.sort((a, b) => Number(a.local) - Number(b.local));
    const ys = stack(shown.length + (hidden.length ? 1 : 0), 82);
    return { nodes: shown.map((server, i) => ({ server, y: ys[i] })), hidden, moreY: ys[shown.length] };
  }, [connections, hosts, selectedId, keyOf]);

  // Where a dot should travel, using the layout on screen now.
  const layout = useRef({ appSlots, serverSlots });
  layout.current = { appSlots, serverSlots };

  const route = useCallback(
    (appId: string, remoteIp: string, host: string | null, outwards: boolean): Point[] => {
      const { appSlots, serverSlots } = layout.current;
      const key = keyOf({ remoteIp, host });
      const appY = appSlots.nodes.find((n) => n.app.id === appId)?.y ?? (appSlots.hidden.length ? appSlots.moreY : null);
      const serverY = serverSlots.nodes.find((n) => n.server.key === key)?.y ?? (serverSlots.hidden.length ? serverSlots.moreY : null);

      const points: Point[] = [];
      if (appY !== null) points.push(...alongCurve(appAnchor(appY), PC_IN));
      if (isLocalAddress(remoteIp) && serverY !== null) {
        points.push(...alongLocal(serverAnchor(serverY)));
      } else {
        points.push(PC_OUT, CLOUD_IN, CLOUD_OUT);
        if (serverY !== null) points.push(...alongCurve(CLOUD_OUT, serverAnchor(serverY)).slice(1));
      }
      return outwards ? points : points.reverse();
    },
    [keyOf],
  );

  const [packets, setPackets] = useState<PacketSpec[]>([]);
  const lastSeq = useRef<number | null>(null);
  const nextFlowDot = useRef(-1);
  const lastFlowDots = useRef(0);

  useEffect(() => {
    if (!ready) return;
    const newest = events[0]?.seq ?? 0;
    const previous = lastSeq.current;
    lastSeq.current = newest;
    // Events from before the window opened are history, not something to replay.
    if (previous === null || paused) return;
    if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) return;

    const fresh = events.filter((e) => e.seq > previous).slice(0, MAX_BURST).reverse();
    if (!fresh.length) return;
    setPackets((current) =>
      [
        ...current,
        ...fresh.map((e, i) => ({
          id: e.seq,
          kind: e.kind,
          points: route(e.appId, e.remoteIp, e.host, e.kind === "opened"),
          delay: i * 110,
        })),
      ].slice(-40),
    );
  }, [events, paused, ready, route]);

  // Now and then, a dot comes home along the busiest lanes to show data is flowing.
  useEffect(() => {
    if (!ready || paused) return;
    const now = performance.now();
    if (now - lastFlowDots.current < 1000) return;
    if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) return;
    lastFlowDots.current = now;
    const busy = connections
      .filter((conn) => (rates.byConn.get(conn.id) ?? 0) > BUSY_RATE && (!selectedId || conn.appId === selectedId))
      .sort((a, b) => (rates.byConn.get(b.id) ?? 0) - (rates.byConn.get(a.id) ?? 0))
      .slice(0, MAX_FLOW_DOTS)
      .filter(() => Math.random() < 0.5);
    if (!busy.length) return;
    setPackets((current) =>
      [
        ...current,
        ...busy.map((conn, i) => ({
          id: nextFlowDot.current--,
          kind: "data" as const,
          points: route(conn.appId, conn.remoteIp, conn.host, false),
          delay: 150 + i * 260,
        })),
      ].slice(-40),
    );
  }, [connections, rates, paused, ready, route, selectedId]);

  const removePacket = useCallback((id: number) => {
    setPackets((current) => current.filter((p) => p.id !== id));
  }, []);

  const anyOnline = connections.length > 0;
  const anyInternet = serverSlots.nodes.some((n) => !n.server.local) || serverSlots.hidden.length > 0;
  const localNodes = serverSlots.nodes.filter((n) => n.server.local);
  const internetRate = [...rates.groupTotal.entries()].reduce((sum, [key, value]) => {
    const node = serverSlots.nodes.find((n) => n.server.key === key);
    return node?.server.local ? sum : sum + value;
  }, 0);
  const serverCaption = selected
    ? serverSlots.nodes.length
      ? `${shorten(selected.name, 22)} is talking to`
      : `${shorten(selected.name, 22)} is not connected`
    : "Busiest servers";

  const toggleMore = (kind: "apps" | "servers") => setMore((current) => (current === kind ? null : kind));
  const moreProps = (kind: "apps" | "servers", label: string) => ({
    role: "button",
    tabIndex: 0,
    "aria-label": label,
    "aria-expanded": more === kind,
    // Keep the menu's own outside-click check from closing it just before this click reopens it.
    onPointerDown: (e: PointerEvent) => e.stopPropagation(),
    onClick: () => toggleMore(kind),
    onKeyDown: (e: KeyboardEvent) => {
      if (e.key === "Enter" || e.key === " ") {
        e.preventDefault();
        toggleMore(kind);
      }
    },
  });

  return (
    <section className="panel map-panel" aria-label="Live map">
      <svg className="map" viewBox={`0 0 ${VIEW.width} ${VIEW.height}`} role="img" aria-label="Apps, this PC and the servers they are connected to">
        {/* Sonar rings spreading from this PC, and one that pulses while live. */}
        <g className="rings">
          {RINGS.map((r) => (
            <circle key={r} cx={PC.x} cy={PC.y + 8} r={r} />
          ))}
          {anyOnline && !paused && <circle className="ring-pulse" cx={PC.x} cy={PC.y + 8} r="70" />}
        </g>

        <text className="map-caption" x={APP_X} y="26" textAnchor="middle">Apps online</text>
        <text className="map-caption" x={SERVER_X + 30} y="26" textAnchor="middle">{serverCaption}</text>

        {/* Lanes go under the nodes. */}
        {appSlots.nodes.map(({ app, y }) => (
          <path
            key={app.id}
            className={laneClass(app.connections > 0, selectedId ? app.id === selectedId : null)}
            style={{ strokeWidth: laneWidth(rates.appTotal.get(app.id) ?? 0) }}
            d={curve(appAnchor(y), PC_IN)}
          />
        ))}
        {appSlots.hidden.length > 0 && (
          <path className={laneClass(true, selectedId ? false : null)} d={curve(appAnchor(appSlots.moreY), PC_IN)} />
        )}
        {anyInternet && (
          <path
            className={laneClass(anyOnline, null)}
            style={{ strokeWidth: laneWidth(internetRate) }}
            d={`M${PC_OUT[0]} ${PC_OUT[1]} L${CLOUD_IN[0]} ${CLOUD_IN[1]}`}
          />
        )}
        {serverSlots.nodes.map(({ server, y }) => (
          <path
            key={server.key}
            className={`${laneClass(true, null)} ${server.local ? "lane-local" : ""}`}
            style={{ strokeWidth: laneWidth(rates.groupTotal.get(server.key) ?? 0) }}
            d={server.local ? localPath(serverAnchor(y)) : curve(CLOUD_OUT, serverAnchor(y))}
          />
        ))}
        {serverSlots.hidden.length > 0 && (
          <path className={laneClass(true, null)} d={curve(CLOUD_OUT, serverAnchor(serverSlots.moreY))} />
        )}
        {localNodes.length > 0 && (
          <text className="map-caption map-local" x={CLOUD.x + 4} y={PC.y + 106} textAnchor="middle">home network</text>
        )}

        {appSlots.nodes.map(({ app, y }) => {
          const state = app.id === selectedId ? "is-selected" : !app.trusted ? "is-new" : app.connections > 0 ? "is-online" : "is-idle";
          const dim = selectedId && app.id !== selectedId ? "is-dim" : "";
          const down = rates.appDown.get(app.id) ?? 0;
          return (
            <g
              key={app.id}
              className={`node app-node ${state} ${dim}`}
              role="button"
              tabIndex={0}
              aria-label={`${app.name}, ${plural(app.connections, "connection")}`}
              onClick={() => onSelect(app.id === selectedId ? null : app.id)}
              onKeyDown={(e) => {
                if (e.key === "Enter" || e.key === " ") {
                  e.preventDefault();
                  onSelect(app.id === selectedId ? null : app.id);
                }
              }}
            >
              <rect className="node-hit" x="8" y={y - 30} width={APP_X + 22} height="60" rx="10" />
              <MapAppTile appId={app.id} cx={APP_X} cy={y} />
              <text className="node-name" x={APP_X - 38} y={y - 2} textAnchor="end">{shorten(app.name, LABEL_CHARS)}</text>
              <text className="node-sub" x={APP_X - 38} y={y + 13} textAnchor="end">
                {app.trusted ? plural(app.connections, "connection") : down > BUSY_RATE ? "New app" : "New · not seen before"}
                {down > BUSY_RATE && <tspan className="node-rate"> · ↓ {rate(down)}</tspan>}
              </text>
            </g>
          );
        })}
        {appSlots.hidden.length > 0 && (
          <g className={`node more-node is-clickable ${selectedId ? "is-dim" : ""}`} {...moreProps("apps", `${appSlots.hidden.length} more apps`)}>
            <rect className="node-hit" x="40" y={appSlots.moreY - 20} width={APP_X - 20} height="40" rx="10" />
            <circle cx={APP_X} cy={appSlots.moreY} r="15" />
            <text className="more-count" x={APP_X} y={appSlots.moreY + 4} textAnchor="middle">+{appSlots.hidden.length}</text>
            <text className="node-sub" x={APP_X - 38} y={appSlots.moreY + 4} textAnchor="end">more apps</text>
          </g>
        )}

        <g className="node">
          <Laptop cx={PC.x} cy={PC.y} />
          <text className="node-name" x={PC.x} y={PC.y + 62} textAnchor="middle">This PC</text>
          <text className="node-sub" x={PC.x} y={PC.y + 77} textAnchor="middle">{plural(connections.length, "connection")}</text>
        </g>

        {anyInternet && (
          <g className="node">
            <Cloud cx={CLOUD.x} cy={CLOUD.y} />
            <text className="node-name" x={CLOUD.x} y={CLOUD.y + 44} textAnchor="middle">Internet</text>
          </g>
        )}

        {serverSlots.nodes.map(({ server, y }) => {
          const down = rates.groupDown.get(server.key) ?? 0;
          const where = server.local ? "Home network" : server.ports.slice(0, 2).map((p) => serviceOf(p).label).join(", ");
          // When owner and country do not both fit, the owner alone (the tooltip has the rest).
          const fullOwner = server.local ? null : ownerLine(server.owner, server.ips[0]);
          const owner =
            fullOwner && fullOwner.length > OWNER_CHARS && server.owner?.owner ? ownerLine({ owner: server.owner.owner, country: null }) : fullOwner;
          const about = ownerTitle(server.owner, server.ips[0]);
          // With an owner line the labels move up a little, so all of them fit beside the rack.
          const top = owner ? y - 8 : y - 2;
          return (
            <g key={server.key} className={`node server-node ${server.local ? "is-local" : ""}`}>
              <title>{`${server.host ?? server.ips[0]}${server.host ? `\n${server.ips.join(", ")}` : ""}${about ? `\n${about}` : ""}`}</title>
              <Server cx={SERVER_X} cy={y} />
              <text className="node-name node-host" x={SERVER_X + 38} y={top}>{shorten(server.host ?? server.key, LABEL_CHARS)}</text>
              {owner && (
                <text className="node-sub node-owner" x={SERVER_X + 38} y={top + 14}>{shorten(owner, OWNER_CHARS)}</text>
              )}
              <text className="node-sub" x={SERVER_X + 38} y={top + (owner ? 28 : 15)}>
                {where}
                {server.ips.length > 1 && ` · ${server.ips.length} addresses`} · {plural(server.count, "connection")}
              </text>
              {down > BUSY_RATE && (
                <text className="node-sub node-rate" x={SERVER_X + 38} y={top + (owner ? 42 : 29)}>↓ {rate(down)}</text>
              )}
            </g>
          );
        })}
        {serverSlots.hidden.length > 0 && (
          <g className="node more-node is-clickable" {...moreProps("servers", `${serverSlots.hidden.length} more servers`)}>
            <rect className="node-hit" x={SERVER_X - 22} y={serverSlots.moreY - 20} width="150" height="40" rx="10" />
            <circle cx={SERVER_X} cy={serverSlots.moreY} r="15" />
            <text className="more-count" x={SERVER_X} y={serverSlots.moreY + 4} textAnchor="middle">+{serverSlots.hidden.length}</text>
            <text className="node-sub" x={SERVER_X + 38} y={serverSlots.moreY + 4}>more servers</text>
          </g>
        )}

        {ready && !anyOnline && (
          <text className="map-empty" x={VIEW.width / 2} y={VIEW.height - 30} textAnchor="middle">
            Nothing is online right now.
          </text>
        )}

        {packets.map((spec) => (
          <Packet key={spec.id} spec={spec} onDone={removePacket} />
        ))}
      </svg>

      {more === "apps" && (
        <div className="menu map-more is-apps" ref={popover} role="dialog" aria-label="More apps">
          <h2 className="section-title">More apps online</h2>
          <ul className="picker-list">
            {appSlots.hidden.map((app) => (
              <li key={app.id}>
                <button
                  className="picker-option"
                  onClick={() => {
                    onSelect(app.id);
                    setMore(null);
                  }}
                >
                  <AppIcon appId={app.id} name={app.name} size={24} />
                  <span className="picker-option-text">
                    <strong>{app.name}</strong>
                    <span>{plural(app.connections, "connection")}</span>
                  </span>
                </button>
              </li>
            ))}
          </ul>
        </div>
      )}
      {more === "servers" && (
        <div className="menu map-more is-servers" ref={popover} role="dialog" aria-label="More servers">
          <h2 className="section-title">More servers</h2>
          <p className="menu-text">Click one to capture its packets.</p>
          <ul className="picker-list">
            {serverSlots.hidden.map((server) => (
              <li key={server.key}>
                <button
                  className="picker-option"
                  title={`Capture packets to and from ${server.host ?? server.ips[0]}`}
                  onClick={() => {
                    setMore(null);
                    onCapture(serverTarget(server.ips[0], server.host));
                  }}
                >
                  <span className="picker-option-text">
                    <strong>{server.host ?? server.key}</strong>
                    {!server.local && ownerLine(server.owner) && (
                      <span className="picker-owner" title={ownerTitle(server.owner) ?? undefined}>{ownerLine(server.owner)}</span>
                    )}
                    <span>
                      {server.local ? "Home network · " : ""}
                      {server.ips.length > 1 ? `${server.ips.length} addresses · ` : server.host ? `${server.ips[0]} · ` : ""}
                      {plural(server.count, "connection")}
                    </span>
                  </span>
                  <RecordIcon />
                </button>
              </li>
            ))}
          </ul>
        </div>
      )}
    </section>
  );
}

/** An app's tile with its icon standing on it. */
function MapAppTile({ appId, cx, cy }: { appId: string; cx: number; cy: number }) {
  return <AppTile cx={cx} cy={cy} icon={useAppIcon(appId)} />;
}

/** `focus` is null when no app is selected, else whether this lane belongs to it. */
function laneClass(active: boolean, focus: boolean | null): string {
  return ["lane", active ? "is-active" : "", focus === true ? "is-focus" : "", focus === false ? "is-dim" : ""].join(" ");
}

/** One dot travelling along a route, removed when it arrives. */
function Packet({ spec, onDone }: { spec: PacketSpec; onDone(id: number): void }) {
  const ref = useRef<SVGGElement>(null);

  useEffect(() => {
    const element = ref.current;
    if (!element) return;

    // Place each waypoint in time by how far along the route it is.
    const steps = spec.points.map((point, i) =>
      i === 0 ? 0 : Math.hypot(point[0] - spec.points[i - 1][0], point[1] - spec.points[i - 1][1]),
    );
    const total = steps.reduce((sum, step) => sum + step, 0) || 1;
    let travelled = 0;
    const frames = spec.points.map(([x, y], i) => {
      travelled += steps[i];
      return { transform: `translate(${x}px, ${y}px)`, offset: travelled / total };
    });

    const timing = { duration: 600 + total * 1.9, delay: spec.delay, fill: "both" as const };
    const move = element.animate(frames, { ...timing, easing: "cubic-bezier(.45, 0, .25, 1)" });
    const fade = element.animate(
      [{ opacity: 0 }, { opacity: 1, offset: 0.1 }, { opacity: 1, offset: 0.85 }, { opacity: 0 }],
      timing,
    );
    move.onfinish = () => onDone(spec.id);

    return () => {
      move.cancel();
      fade.cancel();
    };
  }, [spec, onDone]);

  return (
    <g ref={ref} className={`packet is-${spec.kind}`}>
      <circle className="packet-halo" r={spec.kind === "data" ? 6 : 8} />
      <circle className="packet-core" r={spec.kind === "data" ? 2.8 : 3.6} />
    </g>
  );
}
