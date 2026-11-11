// The live map: apps on the left, this PC in the middle, the internet and the
// servers on the right. A dot travels along the lanes whenever a connection
// opens (outwards) or closes (back home).

import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { plural, shorten } from "../lib/format";
import { serviceOf } from "../lib/services";
import type { AppView, ConnView, EventKind, FlowEvent } from "../lib/types";
import { AppTile, Cloud, Laptop, Server } from "./iso";

const VIEW = { width: 840, height: 500 };
const MID_Y = 258;
const APP_X = 200;
const PC = { x: 368, y: MID_Y };
const CLOUD = { x: 518, y: MID_Y - 4 };
const SERVER_X = 638;
/** Longest label that fits beside a node. */
const LABEL_CHARS = 21;
const MAX_APPS = 6;
const MAX_SERVERS = 5;
/** Most dots launched from one update, so a burst stays readable. */
const MAX_BURST = 10;

type Point = [x: number, y: number];

interface ServerGroup {
  ip: string;
  host: string | null;
  count: number;
  ports: number[];
}

interface PacketSpec {
  id: number;
  kind: EventKind;
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
}

/** Y positions for `count` items stacked around the middle of the map. */
function stack(count: number, spacing: number): number[] {
  return Array.from({ length: count }, (_, i) => MID_Y + (i - (count - 1) / 2) * spacing);
}

/** Splits a list into the items to draw and how many are left over. */
function fit<T>(items: T[], max: number): { shown: T[]; more: number } {
  if (items.length <= max) return { shown: items, more: 0 };
  return { shown: items.slice(0, max - 1), more: items.length - (max - 1) };
}

function groupServers(connections: ConnView[], hosts: ReadonlyMap<string, string>): ServerGroup[] {
  const groups = new Map<string, ServerGroup>();
  for (const conn of connections) {
    const group = groups.get(conn.remoteIp) ?? { ip: conn.remoteIp, host: null, count: 0, ports: [] };
    group.count += 1;
    group.host = conn.host ?? hosts.get(conn.remoteIp) ?? group.host;
    if (!group.ports.includes(conn.remotePort)) group.ports.push(conn.remotePort);
    groups.set(conn.remoteIp, group);
  }
  return [...groups.values()].sort((a, b) => b.count - a.count || a.ip.localeCompare(b.ip));
}

export function LiveMap({ ready, apps, connections, events, hosts, selectedId, paused, onSelect }: Props) {
  const selected = apps.find((app) => app.id === selectedId) ?? null;

  // Apps: the busiest ones, always including the selected one.
  const appSlots = useMemo(() => {
    const online = apps.filter((app) => app.connections > 0 || app.id === selectedId);
    const { shown, more } = fit(online, MAX_APPS);
    if (selected && !shown.includes(selected)) shown[shown.length - 1] = selected;
    const ys = stack(shown.length + (more ? 1 : 0), 70);
    return { nodes: shown.map((app, i) => ({ app, y: ys[i] })), more, moreY: ys[shown.length] };
  }, [apps, selected, selectedId]);

  // Servers: the selected app's, or the busiest overall.
  const serverSlots = useMemo(() => {
    const relevant = selectedId ? connections.filter((c) => c.appId === selectedId) : connections;
    const { shown, more } = fit(groupServers(relevant, hosts), MAX_SERVERS);
    const ys = stack(shown.length + (more ? 1 : 0), 86);
    return { nodes: shown.map((server, i) => ({ server, y: ys[i] })), more, moreY: ys[shown.length] };
  }, [connections, hosts, selectedId]);

  // Where a dot for an event should travel, using the layout on screen now.
  const layout = useRef({ appSlots, serverSlots });
  layout.current = { appSlots, serverSlots };

  const route = useCallback((event: FlowEvent): Point[] => {
    const { appSlots, serverSlots } = layout.current;
    const appY = appSlots.nodes.find((n) => n.app.id === event.appId)?.y ?? (appSlots.more ? appSlots.moreY : null);
    const serverY =
      serverSlots.nodes.find((n) => n.server.ip === event.remoteIp)?.y ?? (serverSlots.more ? serverSlots.moreY : null);

    const points: Point[] = [];
    if (appY !== null) points.push([APP_X + 22, appY]);
    points.push([PC.x, PC.y], [CLOUD.x, CLOUD.y + 4]);
    if (serverY !== null) points.push([SERVER_X - 24, serverY]);
    return event.kind === "opened" ? points : points.reverse();
  }, []);

  const [packets, setPackets] = useState<PacketSpec[]>([]);
  const lastSeq = useRef<number | null>(null);

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
      [...current, ...fresh.map((e, i) => ({ id: e.seq, kind: e.kind, points: route(e), delay: i * 110 }))].slice(-40),
    );
  }, [events, paused, ready, route]);

  const removePacket = useCallback((id: number) => {
    setPackets((current) => current.filter((p) => p.id !== id));
  }, []);

  const anyOnline = connections.length > 0;
  const serverCaption = selected
    ? serverSlots.nodes.length
      ? `${shorten(selected.name, 22)} is talking to`
      : `${shorten(selected.name, 22)} is not connected`
    : "Busiest servers";

  return (
    <section className="panel map-panel" aria-label="Live map">
      <div className="map-badge">
        <span className={`live-dot ${paused ? "is-paused" : ""}`} />
        {paused ? "PAUSED" : "LIVE MAP"}
      </div>

      <svg className="map" viewBox={`0 0 ${VIEW.width} ${VIEW.height}`} role="img" aria-label="Apps, this PC and the servers they are connected to">
        <defs>
          <pattern id="iso-grid" width="55.4" height="32" patternUnits="userSpaceOnUse">
            <path className="grid-line" d="M0 16 27.7 0 55.4 16 27.7 32Z" />
          </pattern>
        </defs>
        <rect width={VIEW.width} height={VIEW.height} fill="url(#iso-grid)" />

        <text className="map-caption" x={APP_X} y="30" textAnchor="middle">Apps online</text>
        <text className="map-caption" x={SERVER_X + 30} y="30" textAnchor="middle">{serverCaption}</text>

        {/* Lanes go under the nodes. */}
        {appSlots.nodes.map(({ app, y }) => (
          <line
            key={app.id}
            className={laneClass(app.connections > 0, selectedId ? app.id === selectedId : null)}
            x1={APP_X + 26} y1={y} x2={PC.x - 54} y2={PC.y + 6}
          />
        ))}
        {appSlots.more > 0 && (
          <line className={laneClass(true, selectedId ? false : null)} x1={APP_X + 26} y1={appSlots.moreY} x2={PC.x - 54} y2={PC.y + 6} />
        )}
        <line className={laneClass(anyOnline, null)} x1={PC.x + 52} y1={PC.y + 2} x2={CLOUD.x - 48} y2={CLOUD.y + 6} />
        {serverSlots.nodes.map(({ server, y }) => (
          <line key={server.ip} className={laneClass(true, null)} x1={CLOUD.x + 50} y1={CLOUD.y + 6} x2={SERVER_X - 28} y2={y} />
        ))}
        {serverSlots.more > 0 && (
          <line className={laneClass(true, null)} x1={CLOUD.x + 50} y1={CLOUD.y + 6} x2={SERVER_X - 28} y2={serverSlots.moreY} />
        )}

        {appSlots.nodes.map(({ app, y }) => {
          const state = app.id === selectedId ? "is-selected" : !app.trusted ? "is-new" : app.connections > 0 ? "is-online" : "is-idle";
          const dim = selectedId && app.id !== selectedId ? "is-dim" : "";
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
              <rect className="node-hit" x="8" y={y - 30} width={APP_X + 22} height="60" />
              <AppTile cx={APP_X} cy={y} />
              <text className="node-name" x={APP_X - 38} y={y - 2} textAnchor="end">{shorten(app.name, LABEL_CHARS)}</text>
              <text className="node-sub" x={APP_X - 38} y={y + 13} textAnchor="end">
                {app.trusted ? plural(app.connections, "connection") : "NEW · not seen before"}
              </text>
            </g>
          );
        })}
        {appSlots.more > 0 && (
          <g className={`node more-node ${selectedId ? "is-dim" : ""}`}>
            <circle cx={APP_X} cy={appSlots.moreY} r="15" />
            <text className="more-count" x={APP_X} y={appSlots.moreY + 4} textAnchor="middle">+{appSlots.more}</text>
            <text className="node-sub" x={APP_X - 38} y={appSlots.moreY + 4} textAnchor="end">more apps in the list</text>
          </g>
        )}

        <g className="node">
          <Laptop cx={PC.x} cy={PC.y} />
          <text className="node-name" x={PC.x} y={PC.y + 62} textAnchor="middle">This PC</text>
          <text className="node-sub" x={PC.x} y={PC.y + 77} textAnchor="middle">{plural(connections.length, "connection")}</text>
        </g>

        <g className="node">
          <Cloud cx={CLOUD.x} cy={CLOUD.y} />
          <text className="node-name" x={CLOUD.x} y={CLOUD.y + 44} textAnchor="middle">Internet</text>
        </g>

        {serverSlots.nodes.map(({ server, y }) => (
          <g key={server.ip} className="node server-node">
            <Server cx={SERVER_X} cy={y} />
            <text className="node-name" x={SERVER_X + 38} y={y - 2}>{shorten(server.host ?? server.ip, LABEL_CHARS)}</text>
            <text className="node-sub" x={SERVER_X + 38} y={y + 13}>
              {server.ports.slice(0, 2).map((p) => serviceOf(p).label).join(", ")} · {plural(server.count, "connection")}
            </text>
          </g>
        ))}
        {serverSlots.more > 0 && (
          <g className="node more-node">
            <circle cx={SERVER_X} cy={serverSlots.moreY} r="15" />
            <text className="more-count" x={SERVER_X} y={serverSlots.moreY + 4} textAnchor="middle">+{serverSlots.more}</text>
            <text className="node-sub" x={SERVER_X + 38} y={serverSlots.moreY + 4}>more servers</text>
          </g>
        )}

        {ready && !anyOnline && (
          <text className="map-empty" x={VIEW.width / 2} y={VIEW.height - 40} textAnchor="middle">
            Nothing is online right now.
          </text>
        )}

        {packets.map((spec) => (
          <Packet key={spec.id} spec={spec} onDone={removePacket} />
        ))}
      </svg>
    </section>
  );
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
      <circle className="packet-halo" r="8" />
      <circle className="packet-core" r="3.6" />
    </g>
  );
}
