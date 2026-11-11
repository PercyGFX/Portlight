// The live map: apps on the left, this PC in the middle, the internet and the
// servers on the right, drawn over sonar rings that spread from this PC.
// A dot travels along the lanes whenever a connection opens (outwards) or
// closes (back home).

import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { plural, shorten } from "../lib/format";
import { serviceOf } from "../lib/services";
import type { AppView, ConnView, EventKind, FlowEvent } from "../lib/types";
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
/** Most dots launched from one update, so a burst stays readable. */
const MAX_BURST = 10;
/** Sonar rings around this PC. */
const RINGS = [70, 140, 210, 280, 350, 420];

type Point = [x: number, y: number];

// Where lanes meet the drawings.
const PC_IN: Point = [PC.x - 54, PC.y + 6];
const PC_OUT: Point = [PC.x + 52, PC.y + 2];
const CLOUD_IN: Point = [CLOUD.x - 48, CLOUD.y + 6];
const CLOUD_OUT: Point = [CLOUD.x + 50, CLOUD.y + 6];
const appAnchor = (y: number): Point => [APP_X + 28, y];
const serverAnchor = (y: number): Point => [SERVER_X - 30, y];

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

/** An S-shaped lane: leaves and arrives level, bends in between. */
function curve([x1, y1]: Point, [x2, y2]: Point): string {
  const mid = (x1 + x2) / 2;
  return `M${x1} ${y1} C${mid} ${y1} ${mid} ${y2} ${x2} ${y2}`;
}

/** Points along the same S-shape, for a dot to follow. */
function alongCurve([x1, y1]: Point, [x2, y2]: Point, steps = 10): Point[] {
  const mid = (x1 + x2) / 2;
  return Array.from({ length: steps + 1 }, (_, i) => {
    const t = i / steps;
    const u = 1 - t;
    return [
      u * u * u * x1 + 3 * u * u * t * mid + 3 * u * t * t * mid + t * t * t * x2,
      u * u * u * y1 + 3 * u * u * t * y1 + 3 * u * t * t * y2 + t * t * t * y2,
    ];
  });
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
    const ys = stack(shown.length + (more ? 1 : 0), 66);
    return { nodes: shown.map((app, i) => ({ app, y: ys[i] })), more, moreY: ys[shown.length] };
  }, [apps, selected, selectedId]);

  // Servers: the selected app's, or the busiest overall.
  const serverSlots = useMemo(() => {
    const relevant = selectedId ? connections.filter((c) => c.appId === selectedId) : connections;
    const { shown, more } = fit(groupServers(relevant, hosts), MAX_SERVERS);
    const ys = stack(shown.length + (more ? 1 : 0), 82);
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
    if (appY !== null) points.push(...alongCurve(appAnchor(appY), PC_IN));
    points.push(PC_OUT, CLOUD_IN, CLOUD_OUT);
    if (serverY !== null) points.push(...alongCurve(CLOUD_OUT, serverAnchor(serverY)).slice(1));
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
            d={curve(appAnchor(y), PC_IN)}
          />
        ))}
        {appSlots.more > 0 && (
          <path className={laneClass(true, selectedId ? false : null)} d={curve(appAnchor(appSlots.moreY), PC_IN)} />
        )}
        <path className={laneClass(anyOnline, null)} d={`M${PC_OUT[0]} ${PC_OUT[1]} L${CLOUD_IN[0]} ${CLOUD_IN[1]}`} />
        {serverSlots.nodes.map(({ server, y }) => (
          <path key={server.ip} className={laneClass(true, null)} d={curve(CLOUD_OUT, serverAnchor(y))} />
        ))}
        {serverSlots.more > 0 && (
          <path className={laneClass(true, null)} d={curve(CLOUD_OUT, serverAnchor(serverSlots.moreY))} />
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
              <rect className="node-hit" x="8" y={y - 30} width={APP_X + 22} height="60" rx="10" />
              <AppTile cx={APP_X} cy={y} />
              <text className="node-name" x={APP_X - 38} y={y - 2} textAnchor="end">{shorten(app.name, LABEL_CHARS)}</text>
              <text className="node-sub" x={APP_X - 38} y={y + 13} textAnchor="end">
                {app.trusted ? plural(app.connections, "connection") : "New · not seen before"}
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
            <text className="node-name node-host" x={SERVER_X + 38} y={y - 2}>{shorten(server.host ?? server.ip, LABEL_CHARS)}</text>
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
          <text className="map-empty" x={VIEW.width / 2} y={VIEW.height - 30} textAnchor="middle">
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
