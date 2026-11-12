// A slim picture across the top of the capture page: this PC on the left, what is
// being captured on the right, and a dot along the lane for packets as they pass
// (outgoing on the upper lane, incoming on the lower), coloured by kind.

import { useCallback, useEffect, useRef, useState } from "react";

import { useAppIcon } from "../../hooks/useAppIcon";
import { backend } from "../../lib/backend";
import { shorten } from "../../lib/format";
import { FAMILY_LABELS, familyOf, type Family } from "../../lib/protocols";
import type { CaptureTarget, PacketRow } from "../../lib/types";
import { AppTile, Cloud, Laptop, Server } from "../iso";

const HEIGHT = 96;
const NODE_X = 64;
const LABEL_GAP = 46;
const LANE_INSET = 210;
const OUT_Y = 34;
const IN_Y = 52;
/** Most dots started from one status update, so a burst stays readable. */
const MAX_BURST = 8;
const MAX_DOTS = 40;

interface Dot {
  id: number;
  family: Family;
  out: boolean;
  delay: number;
}

interface Props {
  target: CaptureTarget;
  /** Words for the network cards, e.g. "Wi-Fi" or "All active network cards". */
  cards: string;
  running: boolean;
  /** Packets in the shown list; a rise means new packets to draw. */
  shown: number;
  /** Changes when the shown list is a different list. */
  listKey: string;
  /** The app on the left, for its icon; null when it is this PC. */
  appId: string | null;
  /** "Cloudflare · United States" for a server on the right, and the longer words for its tooltip. */
  owner: string | null;
  ownerAbout: string | null;
}

export function ConversationStrip({ target, cards, running, shown, listKey, appId, owner, ownerAbout }: Props) {
  const icon = useAppIcon(appId);
  const box = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(900);
  const [dots, setDots] = useState<Dot[]>([]);
  const lastShown = useRef<{ key: string; shown: number } | null>(null);

  useEffect(() => {
    const element = box.current;
    if (!element) return;
    const observer = new ResizeObserver(() => setWidth(Math.max(560, element.clientWidth)));
    observer.observe(element);
    return () => observer.disconnect();
  }, []);

  // New packets: fetch the newest few and send a dot for each.
  useEffect(() => {
    const previous = lastShown.current;
    lastShown.current = { key: listKey, shown };
    if (!running || !previous || previous.key !== listKey || shown <= previous.shown) return;
    if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) return;
    const fresh = Math.min(MAX_BURST, shown - previous.shown);
    let active = true;
    backend
      .captureRows(shown - fresh, fresh)
      .then((rows: PacketRow[]) => {
        if (!active) return;
        const spacing = 240 / Math.max(1, rows.length);
        setDots((current) =>
          [
            ...current,
            ...rows.map((row, i) => ({
              id: row.number,
              family: familyOf(row.protocol),
              out: row.direction === "out",
              delay: i * spacing,
            })),
          ].slice(-MAX_DOTS),
        );
      })
      .catch(console.error);
    return () => {
      active = false;
    };
  }, [shown, running, listKey]);

  useEffect(() => {
    if (!running) setDots([]);
  }, [running]);

  const removeDot = useCallback((id: number) => setDots((current) => current.filter((dot) => dot.id !== id)), []);

  const left = NODE_X;
  const right = width - NODE_X;
  const laneStart = left + LANE_INSET;
  const laneEnd = right - LANE_INSET;
  const ends = describeEnds(target, cards);
  const rightOwner = ends.right.kind === "server" ? owner : null;

  return (
    <div className={`strip ${running ? "is-running" : ""}`} ref={box}>
      <svg className="strip-svg" width={width} height={HEIGHT} viewBox={`0 0 ${width} ${HEIGHT}`} role="img" aria-label={`${ends.left.title} and ${ends.right.title}`}>
        <g className="rings">
          {[48, 110, 190, 290, 410, 560, 740, 960].map((r) => (
            <circle key={r} cx={left} cy={HEIGHT / 2} r={r} />
          ))}
          {running && <circle className="ring-pulse" cx={left} cy={HEIGHT / 2} r="48" />}
        </g>

        <path className={`lane ${running ? "is-active" : ""}`} d={`M${laneStart} ${OUT_Y} L${laneEnd} ${OUT_Y}`} />
        <path className={`lane lane-in ${running ? "is-active" : ""}`} d={`M${laneEnd} ${IN_Y} L${laneStart} ${IN_Y}`} />
        <text className="strip-lane-label" x={(laneStart + laneEnd) / 2} y={OUT_Y - 9} textAnchor="middle">sent →</text>
        <text className="strip-lane-label" x={(laneStart + laneEnd) / 2} y={IN_Y + 15} textAnchor="middle">← received</text>

        <g className="node" transform={`translate(${left} ${HEIGHT / 2 - 2}) scale(${ends.left.kind === "pc" ? 0.6 : 0.85})`}>
          {ends.left.kind === "pc" ? <Laptop cx={0} cy={0} /> : <AppTile cx={0} cy={0} icon={icon} />}
        </g>
        <text className="node-name" x={left + LABEL_GAP} y={HEIGHT / 2 - 2}>{shorten(ends.left.title, 22)}</text>
        <text className="node-sub" x={left + LABEL_GAP} y={HEIGHT / 2 + 14}>{shorten(ends.left.sub, 28)}</text>

        <g className="node" transform={`translate(${right} ${HEIGHT / 2 - (ends.right.kind === "server" ? 6 : 0)}) scale(${ends.right.kind === "server" ? 0.62 : 0.78})`}>
          {ends.right.kind === "server" ? <Server cx={0} cy={0} /> : <Cloud cx={0} cy={0} />}
        </g>
        <text className={`node-name ${ends.right.kind === "server" ? "node-host" : ""}`} x={right - LABEL_GAP - 4} y={HEIGHT / 2 - (rightOwner ? 9 : 2)} textAnchor="end">
          {shorten(ends.right.title, 24)}
        </text>
        {rightOwner && (
          <text className="node-sub node-owner" x={right - LABEL_GAP - 4} y={HEIGHT / 2 + 6} textAnchor="end">
            {ownerAbout && <title>{ownerAbout}</title>}
            {shorten(rightOwner, 36)}
          </text>
        )}
        <text className="node-sub" x={right - LABEL_GAP - 4} y={HEIGHT / 2 + (rightOwner ? 21 : 14)} textAnchor="end">{shorten(ends.right.sub, 30)}</text>

        {dots.map((dot) => (
          <StripDot key={dot.id} dot={dot} from={dot.out ? laneStart : laneEnd} to={dot.out ? laneEnd : laneStart} y={dot.out ? OUT_Y : IN_Y} onDone={removeDot} />
        ))}
      </svg>
      <ul className="strip-legend" aria-label="Dot colours">
        {FAMILY_LABELS.map(([family, label]) => (
          <li key={family}>
            <span className={`swatch fam-${family}`} /> {label}
          </li>
        ))}
      </ul>
    </div>
  );
}

interface End {
  kind: "pc" | "app" | "server" | "cloud";
  title: string;
  sub: string;
}

function describeEnds(target: CaptureTarget, cards: string): { left: End; right: End } {
  const pc: End = { kind: "pc", title: "This PC", sub: cards };
  switch (target.kind) {
    case "all":
      return { left: pc, right: { kind: "cloud", title: "The internet", sub: "and your home network" } };
    case "app":
      return {
        left: { kind: "app", title: target.name, sub: "on this PC" },
        right: { kind: "cloud", title: "Any server", sub: `wherever ${target.name} connects` },
      };
    case "host":
      return {
        left: { ...pc, sub: "any app" },
        right: { kind: "server", title: target.name ?? target.ip, sub: target.name ? target.ip : "one address" },
      };
    case "domain":
      return { left: { ...pc, sub: "any app" }, right: { kind: "server", title: target.name, sub: "every address it uses" } };
    case "connection":
      return {
        left: { kind: "app", title: target.appName, sub: `port ${target.localPort} on this PC` },
        right: {
          kind: "server",
          title: target.host ?? target.remoteIp,
          sub: `port ${target.remotePort} · ${target.protocol.toUpperCase()}`,
        },
      };
  }
}

/** One dot gliding along a lane, moved by the browser so React draws it once. */
function StripDot({ dot, from, to, y, onDone }: { dot: Dot; from: number; to: number; y: number; onDone(id: number): void }) {
  const ref = useRef<SVGGElement>(null);

  useEffect(() => {
    const element = ref.current;
    if (!element) return;
    const timing = { duration: 900 + Math.abs(to - from) * 1.1, delay: dot.delay, fill: "both" as const };
    const move = element.animate(
      [{ transform: `translate(${from}px, ${y}px)` }, { transform: `translate(${to}px, ${y}px)` }],
      { ...timing, easing: "cubic-bezier(.45, 0, .25, 1)" },
    );
    const fade = element.animate([{ opacity: 0 }, { opacity: 1, offset: 0.12 }, { opacity: 1, offset: 0.85 }, { opacity: 0 }], timing);
    move.onfinish = () => onDone(dot.id);
    return () => {
      move.cancel();
      fade.cancel();
    };
  }, [dot, from, to, y, onDone]);

  return (
    <g ref={ref} className={`strip-dot fam-${dot.family}`}>
      <circle className="strip-dot-halo" r="7" />
      <circle className="strip-dot-core" r="3.4" />
    </g>
  );
}
