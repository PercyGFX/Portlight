// The packet list: one row per packet, only the rows on screen are drawn, so
// 200,000 packets scroll as smoothly as 20. It follows the newest packet while
// scrolled to the bottom, like Wireshark's auto-scroll.

import { forwardRef, useCallback, useEffect, useImperativeHandle, useLayoutEffect, useRef, useState, type CSSProperties } from "react";

import { usePacketRows } from "../../hooks/usePacketRows";
import { count } from "../../lib/format";
import { aboutProtocol, familyOf } from "../../lib/protocols";
import type { PacketRow } from "../../lib/types";
import { DownIcon } from "../icons";

export const ROW_HEIGHT = 27;
const HEAD_HEIGHT = 30;
const OVERSCAN = 12;
/** How long a newly arrived row stays highlighted. */
const FRESH_MS = 900;

interface Props {
  /** Changes when the list is a different list: new capture or new filter. */
  listKey: string;
  shown: number;
  running: boolean;
  /** Show the plain sentence (true) or Wireshark's technical line (false). */
  plain: boolean;
  /** Add an App column (when capturing everything). */
  showApp: boolean;
  selected: number | null;
  onSelect(row: PacketRow): void;
}

export interface PacketListHandle {
  /** Moves the selection up or down by `delta` rows. */
  move(delta: number): void;
}

export const PacketList = forwardRef<PacketListHandle, Props>(function PacketList(props, ref) {
  const { listKey, shown, running, plain, showApp, selected, onSelect } = props;
  const rows = usePacketRows(listKey, shown);
  const scroller = useRef<HTMLDivElement>(null);
  const [view, setView] = useState({ top: 0, height: 400 });
  const [following, setFollowing] = useState(true);
  const followingRef = useRef(true);
  followingRef.current = following;

  // Where the selection is in the list. Recomputed when the list changes.
  const [selectedPos, setSelectedPos] = useState<number | null>(null);
  useEffect(() => setSelectedPos(null), [listKey]);
  const position = selectedPos ?? (selected !== null ? rows.positionOf(selected) : null);

  // Newly arrived rows glow briefly, only while following the newest.
  const fresh = useRef({ from: Infinity, until: 0 });
  const previousShown = useRef(shown);
  if (shown !== previousShown.current) {
    if (shown > previousShown.current && followingRef.current && running) {
      fresh.current = { from: previousShown.current, until: Date.now() + FRESH_MS };
    }
    previousShown.current = shown;
  }
  useEffect(() => {
    fresh.current = { from: Infinity, until: 0 };
  }, [listKey]);

  // Keep the newest row in view while following.
  useLayoutEffect(() => {
    const element = scroller.current;
    if (element && following) element.scrollTop = element.scrollHeight;
  }, [shown, following, listKey]);

  useEffect(() => {
    const element = scroller.current;
    if (!element) return;
    const observer = new ResizeObserver(() => setView({ top: element.scrollTop, height: element.clientHeight }));
    observer.observe(element);
    return () => observer.disconnect();
  }, []);

  const onScroll = useCallback(() => {
    const element = scroller.current;
    if (!element) return;
    setView({ top: element.scrollTop, height: element.clientHeight });
    const atBottom = element.scrollTop + element.clientHeight >= element.scrollHeight - 4;
    if (atBottom !== followingRef.current) setFollowing(atBottom);
  }, []);

  const bodyTop = Math.max(0, view.top - HEAD_HEIGHT);
  const first = Math.max(0, Math.floor(bodyTop / ROW_HEIGHT) - OVERSCAN);
  const last = Math.min(shown - 1, Math.ceil((bodyTop + view.height) / ROW_HEIGHT) + OVERSCAN);

  useEffect(() => {
    rows.ensure(first, last);
  });

  const scrollTo = (pos: number) => {
    const element = scroller.current;
    if (!element) return;
    const top = HEAD_HEIGHT + pos * ROW_HEIGHT;
    if (top - HEAD_HEIGHT < element.scrollTop) element.scrollTop = top - HEAD_HEIGHT;
    else if (top + ROW_HEIGHT > element.scrollTop + element.clientHeight) element.scrollTop = top + ROW_HEIGHT - element.clientHeight;
  };

  const jumpToNewest = () => {
    setFollowing(true);
    const element = scroller.current;
    if (element) element.scrollTop = element.scrollHeight;
  };

  useImperativeHandle(ref, () => ({
    move(delta: number) {
      if (shown === 0) return;
      const start = position ?? (delta > 0 ? first - 1 : Math.min(shown, last + 1));
      const next = Math.max(0, Math.min(shown - 1, start + delta));
      setSelectedPos(next);
      if (next < shown - 1) setFollowing(false);
      scrollTo(next);
      const row = rows.rowAt(next);
      if (row) onSelect(row);
      else {
        // Not fetched yet: ask for it, then select it.
        rows.ensure(next, next);
        window.setTimeout(() => {
          const late = rows.rowAt(next);
          if (late) onSelect(late);
        }, 120);
      }
    },
  }));

  const now = Date.now();
  const glowFrom = fresh.current.until > now ? fresh.current.from : Infinity;
  const items = [];
  for (let pos = first; pos <= last; pos++) {
    const row = rows.rowAt(pos);
    const style = { transform: `translateY(${pos * ROW_HEIGHT}px)` };
    if (!row) {
      items.push(<div key={`wait-${pos}`} className="packet-row is-waiting" style={style} aria-hidden="true" />);
      continue;
    }
    items.push(
      <Row
        key={row.number}
        row={row}
        style={style}
        plain={plain}
        showApp={showApp}
        selected={row.number === selected}
        fresh={pos >= glowFrom}
        onSelect={() => {
          setSelectedPos(pos);
          onSelect(row);
        }}
      />,
    );
  }

  return (
    <div className={`packet-list ${showApp ? "has-app" : ""}`}>
      <div className="packet-scroll" ref={scroller} onScroll={onScroll} role="grid" aria-rowcount={shown} aria-label="Packets">
        <div className="packet-row packet-head" role="row">
          <span role="columnheader">No.</span>
          <span role="columnheader">Time</span>
          <span role="columnheader" aria-label="Direction" />
          <span role="columnheader">From</span>
          <span role="columnheader">To</span>
          {showApp && <span role="columnheader">App</span>}
          <span role="columnheader">Protocol</span>
          <span role="columnheader" className="num">Length</span>
          <span role="columnheader">{plain ? "What happened" : "Info"}</span>
        </div>
        <div className="packet-body" style={{ height: shown * ROW_HEIGHT }}>
          {items}
        </div>
      </div>
      {!following && shown > 0 && (
        <button className="button jump-newest" onClick={jumpToNewest}>
          <DownIcon /> {running ? "Jump to newest" : "Go to the end"}
          {running && <span className="button-count">{count(shown)}</span>}
        </button>
      )}
    </div>
  );
});

interface RowProps {
  row: PacketRow;
  style: CSSProperties;
  plain: boolean;
  showApp: boolean;
  selected: boolean;
  fresh: boolean;
  onSelect(): void;
}

function Row({ row, style, plain, showApp, selected, fresh, onSelect }: RowProps) {
  const family = familyOf(row.protocol);
  const text = plain ? row.summary : row.info;
  return (
    <div
      className={`packet-row ${selected ? "is-selected" : ""} ${fresh ? "is-fresh" : ""}`}
      style={style}
      role="row"
      aria-selected={selected}
      onClick={onSelect}
    >
      <span className="mono muted">{row.number}</span>
      <span className="mono muted">{row.time.toFixed(3)}</span>
      <span className={`direction is-${row.direction ?? "none"}`} title={row.direction === "out" ? "Sent by this PC" : row.direction === "in" ? "Received by this PC" : "Seen on the network"}>
        {row.direction === "out" ? "↑" : row.direction === "in" ? "↓" : "·"}
      </span>
      <Endpoint name={row.sourceName} address={row.source} port={row.sourcePort} />
      <Endpoint name={row.destinationName} address={row.destination} port={row.destinationPort} />
      {showApp && <span className="cell-app" title={row.app ?? undefined}>{row.app ?? "—"}</span>}
      <span>
        <span className={`proto proto-${family}`} title={aboutProtocol(row.protocol)}>{row.protocol}</span>
      </span>
      <span className="mono num">{row.length}</span>
      <span className={plain ? "cell-text" : "cell-text mono"} title={plain ? row.info : row.summary}>{text}</span>
    </div>
  );
}

function Endpoint({ name, address, port }: { name: string | null; address: string; port: number | null }) {
  const title = `${name ? `${name} · ` : ""}${address}${port !== null ? ` port ${port}` : ""}`;
  return (
    <span className="endpoint" title={title}>
      <span className={name ? "endpoint-name" : "endpoint-name mono"}>{name ?? address}</span>
      {port !== null && <span className="endpoint-port">:{port}</span>}
    </span>
  );
}
