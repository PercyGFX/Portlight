// The packet capture page: a full page of its own, "Wireshark, but friendly".
// Header (what is captured, Start/Stop, Save), the conversation strip with live
// counters, then the packet list and, under it, the chosen packet's layers and bytes.

import { useCallback, useEffect, useMemo, useRef, useState, type CSSProperties, type PointerEvent as ReactPointerEvent, type RefObject } from "react";

import type { Capture } from "../../hooks/useCapture";
import { useDismiss } from "../../hooks/useDismiss";
import { backend } from "../../lib/backend";
import { bytes, count, elapsed } from "../../lib/format";
import { sameTarget, targetDescription, targetLabel, targetTitle } from "../../lib/targets";
import type { AppView, CaptureStartOptions, CaptureTarget, ConnView, PacketDetail, PacketRow } from "../../lib/types";
import { ownerLine, ownerTitle, type Owner } from "../../lib/owners";
import { AppIcon } from "../AppIcon";
import { CopyButton } from "../CopyButton";
import { BackIcon, CheckIcon, ChevronDownIcon, CloseIcon, LockIcon, RecordIcon, StopIcon } from "../icons";
import { CaptureTargetPicker } from "./CaptureTargetPicker";
import { ConversationStrip } from "./ConversationStrip";
import { FilterBar } from "./FilterBar";
import { HexView } from "./HexView";
import { NpcapMissing } from "./NpcapMissing";
import { PacketList, type PacketListHandle } from "./PacketList";
import { breakdownText, PacketDetails, rangeAt, type ByteRange } from "./PacketDetails";
import { SaveMenu } from "./SaveMenu";
import { Ticker } from "./Ticker";

interface Props {
  capture: Capture;
  /** What pressing Start will capture. */
  target: CaptureTarget;
  apps: AppView[];
  connections: ConnView[];
  hosts: ReadonlyMap<string, string>;
  owners: ReadonlyMap<string, Owner>;
  leaving: boolean;
  /** An entry point asked to start recording straight away. */
  autoStart: boolean;
  /** The request to start straight away has been handled. */
  onAutoStarted(): void;
  onTarget(target: CaptureTarget): void;
  onBack(): void;
}

const PLAIN_KEY = "portlight.capture.plain";
const SPLIT_KEY = "portlight.capture.detailsHeight";
const STOP_UNITS = [
  ["seconds", "seconds"],
  ["packets", "packets"],
  ["megabytes", "MB"],
] as const;
const DNS_NOTE =
  "Name lookups (DNS) are made by Windows on the app's behalf, so they appear under Windows system services, not this app.";

export function CapturePage({ capture, target, apps, connections, hosts, owners, leaving, autoStart, onAutoStarted, onTarget, onBack }: Props) {
  const { support, status } = capture;
  const [card, setCard] = useState("");
  const [captureFilter, setCaptureFilter] = useState("");
  const [stopAfter, setStopAfter] = useState({ seconds: "", packets: "", megabytes: "" });
  const [advanced, setAdvanced] = useState(false);
  const [plain, setPlain] = useState(() => localStorage.getItem(PLAIN_KEY) !== "0");
  const [confirming, setConfirming] = useState<CaptureStartOptions | null>(null);

  const [selectedRow, setSelectedRow] = useState<PacketRow | null>(null);
  const [detail, setDetail] = useState<PacketDetail | null>(null);
  const [collapsed, setCollapsed] = useState<ReadonlySet<string>>(() => new Set(["frame", "eth"]));
  const [hovered, setHovered] = useState<ByteRange | null>(null);
  const [picked, setPicked] = useState<ByteRange | null>(null);

  const filterRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<PacketListHandle>(null);

  const available = support?.available ?? false;
  const running = status.state === "running";
  const recorded = status.state !== "idle" ? status.target : null;
  const shownTarget = recorded ?? target;
  const listKey = `${status.startedAt}|${status.filter}`;
  const filtered = status.filter !== "";
  const cardName = card ? (support?.interfaces.find((i) => i.id === card)?.name ?? "One network card") : "All active network cards";
  const targetApp = useMemo(() => appOf(shownTarget, apps), [shownTarget, apps]);
  const serverIp = useMemo(() => serverIpOf(shownTarget, connections, hosts), [shownTarget, connections, hosts]);
  const serverOwner = useMemo(() => {
    if (!serverIp) return null;
    const conn = connections.find((c) => c.remoteIp === serverIp && (c.owner || c.country));
    return conn ? { owner: conn.owner, country: conn.country } : (owners.get(serverIp) ?? null);
  }, [serverIp, connections, owners]);

  // A new capture forgets the chosen packet.
  useEffect(() => {
    setSelectedRow(null);
    setDetail(null);
    setPicked(null);
  }, [status.startedAt]);

  // Fetch the chosen packet's details.
  const selectedNumber = selectedRow?.number ?? null;
  useEffect(() => {
    if (selectedNumber === null) return;
    let active = true;
    backend
      .capturePacket(selectedNumber)
      .then((next) => {
        if (!active) return;
        setDetail(next);
        setPicked(null);
      })
      .catch(console.error);
    return () => {
      active = false;
    };
  }, [selectedNumber]);

  const startNow = useCallback(
    (options: CaptureStartOptions) => {
      setConfirming(null);
      capture.start(options);
    },
    [capture],
  );

  const requestStart = useCallback(() => {
    const limit = (text: string) => (Number(text) > 0 ? Number(text) : null);
    const options: CaptureStartOptions = {
      target,
      interfaces: card ? [card] : [],
      captureFilter: captureFilter.trim(),
      stopAfter: { seconds: limit(stopAfter.seconds), packets: limit(stopAfter.packets), megabytes: limit(stopAfter.megabytes) },
    };
    if (capture.unsaved) setConfirming(options);
    else startNow(options);
  }, [target, card, captureFilter, stopAfter, capture.unsaved, startNow]);

  // One click from the live view: start as soon as Npcap is known to be there,
  // unless that very capture is already running.
  const startedOnce = useRef(false);
  useEffect(() => {
    if (!autoStart || !support || startedOnce.current) return;
    startedOnce.current = true;
    onAutoStarted();
    if (!support.available) return;
    if (status.state === "running" && sameTarget(status.target, target)) return;
    requestStart();
  }, [autoStart, support, status.state, status.target, target, requestStart, onAutoStarted]);

  const togglePlain = (next: boolean) => {
    localStorage.setItem(PLAIN_KEY, next ? "1" : "0");
    setPlain(next);
  };

  // Keys: Esc goes back to the live view, Ctrl+F searches, arrows move through packets.
  const keys = useRef({ onBack, confirming, cancel: () => setConfirming(null) });
  keys.current = { onBack, confirming, cancel: () => setConfirming(null) };
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement | null;
      const typing = target?.tagName === "INPUT" || target?.tagName === "TEXTAREA" || target?.tagName === "SELECT";
      const menuOpen = document.querySelector(".capture-page .menu") !== null;
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "f") {
        event.preventDefault();
        filterRef.current?.focus();
        filterRef.current?.select();
      } else if (event.key === "Escape" && !typing && !menuOpen) {
        if (keys.current.confirming) keys.current.cancel();
        else keys.current.onBack();
      } else if ((event.key === "ArrowDown" || event.key === "ArrowUp") && !typing && !menuOpen) {
        event.preventDefault();
        listRef.current?.move(event.key === "ArrowDown" ? 1 : -1);
      } else if ((event.key === "PageDown" || event.key === "PageUp") && !typing && !menuOpen) {
        event.preventDefault();
        listRef.current?.move(event.key === "PageDown" ? 15 : -15);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const duration = status.startedAt ? ((status.stoppedAt ?? (running ? Date.now() : status.startedAt)) - status.startedAt) / 1000 : 0;
  const active = hovered ?? picked;
  const stopSummary = STOP_UNITS.filter(([key]) => Number(stopAfter[key]) > 0)
    .map(([key, unit]) => `${count(Number(stopAfter[key]))} ${unit}`)
    .join(" or ");
  // Before the first capture, the big button in the middle is the one thing to press.
  const readyCard = available && status.state === "idle" && status.packets === 0;
  const statePill = running ? (
    <span className="pill pill-recording">
      <span className="live-dot" /> Recording
    </span>
  ) : status.state === "stopped" ? (
    <span className="pill pill-stopped">Stopped</span>
  ) : (
    <span className="pill pill-ready">Ready</span>
  );

  return (
    <div className={`capture-page ${leaving ? "is-leaving" : "is-entering"}`}>
      <header className="capture-head">
        <button className="button capture-back" onClick={onBack} title="Back to the live view (Esc)">
          <BackIcon /> Live view
        </button>

        <div className="capture-heading">
          <div className="capture-title">
            {targetApp && <AppIcon appId={targetApp.id} name={targetApp.name} size={28} />}
            <h1>{targetTitle(shownTarget)}</h1>
          </div>
          <div className="capture-setup">
            <span className="setup-label">Capture</span>
            <CaptureTargetPicker target={target} apps={apps} connections={connections} hosts={hosts} onChange={onTarget} />
            {available && (
              <button
                className={`button button-quiet disclosure ${advanced ? "is-open" : ""}`}
                aria-expanded={advanced}
                onClick={() => setAdvanced((v) => !v)}
              >
                More options <ChevronDownIcon />
              </button>
            )}
            {!advanced && stopSummary && <span className="setup-hint">Stops by itself after {stopSummary}</span>}
            {shownTarget.kind === "app" && (
              <span className="setup-hint" title={DNS_NOTE}>
                Name lookups show under Windows system services
              </span>
            )}
          </div>
        </div>

        <div className="capture-actions">
          {available && statePill}
          {status.packets > 0 && (
            <SaveMenu
              packets={status.packets}
              shown={status.shown}
              filtered={filtered}
              selected={selectedRow?.number ?? null}
              busy={capture.busy === "saving"}
              onSave={capture.save}
            />
          )}
          {running ? (
            <button className="button button-solid button-big button-stop" disabled={capture.busy !== null} onClick={capture.stop}>
              <StopIcon /> Stop
            </button>
          ) : readyCard || !available ? null : (
            <button className="button button-solid button-big" disabled={!available || capture.busy !== null} onClick={requestStart}>
              <RecordIcon /> {status.state === "stopped" ? "Start new capture" : "Start recording"}
            </button>
          )}
        </div>
      </header>

      {advanced && available && (
        <div className="capture-advanced step-in">
          <label className="form-field">
            <span>Network card</span>
            <span className="select">
              <select value={card} onChange={(event) => setCard(event.target.value)}>
                <option value="">All active network cards</option>
                {support?.interfaces.map((item) => (
                  <option key={item.id} value={item.id} disabled={!item.active} title={item.description}>
                    {item.name}
                    {item.active ? "" : " (not connected)"}
                  </option>
                ))}
              </select>
              <ChevronDownIcon />
            </span>
            <small>Usually leave this as it is.</small>
          </label>
          <fieldset className="form-field stop-after">
            <legend>Stop automatically after</legend>
            <div>
              {STOP_UNITS.map(([key, unit]) => (
                <label key={key}>
                  <input
                    type="text"
                    inputMode="numeric"
                    placeholder="Off"
                    aria-label={`Stop after this many ${unit}`}
                    value={stopAfter[key]}
                    onChange={(event) => {
                      const digits = event.target.value.replace(/\D/g, "").slice(0, 9);
                      setStopAfter((current) => ({ ...current, [key]: digits }));
                    }}
                  />
                  {unit}
                </label>
              ))}
            </div>
            <small>Leave empty to record until you press Stop.</small>
          </fieldset>
          <label className="form-field capture-filter">
            <span>Capture filter</span>
            <input
              value={captureFilter}
              spellCheck={false}
              placeholder="e.g. port 443"
              onChange={(event) => setCaptureFilter(event.target.value)}
            />
            <small>Wireshark's capture-filter language. Most people leave this empty.</small>
          </label>
        </div>
      )}

      {recorded && !sameTarget(recorded, target) && (
        <div className="capture-note step-in" role="status">
          <span>
            Next capture: <strong>{targetLabel(target)}</strong>. {running ? "This one keeps recording until you switch." : "Press Start new capture when ready."}
          </span>
          {running && (
            <button className="button" onClick={requestStart}>
              Switch now
            </button>
          )}
          <button className="button button-quiet" onClick={() => onTarget(recorded)}>
            Keep {targetLabel(recorded)}
          </button>
        </div>
      )}

      {confirming && (
        <div className="callout capture-confirm step-in" role="alertdialog" aria-label="Start a new capture?">
          <p>
            {capture.unsaved ? (
              <>
                <strong>Start a new capture?</strong> The {count(status.packets)} packets you have now are not saved and will
                be thrown away.
              </>
            ) : (
              <>
                <strong>Saved.</strong> Ready to start the new capture.
              </>
            )}
          </p>
          <div className="capture-confirm-actions">
            {capture.unsaved && (
              <button className="button" disabled={capture.busy === "saving" || running} onClick={() => capture.save("pcapng", { kind: "all" })} title={running ? "Stop the capture first, or start anyway" : undefined}>
                Save for Wireshark first
              </button>
            )}
            <button className="button button-solid" onClick={() => startNow(confirming)}>
              {capture.unsaved ? "Start anyway" : "Start the new capture"}
            </button>
            <button className="button button-quiet" onClick={() => setConfirming(null)}>
              Cancel
            </button>
          </div>
        </div>
      )}

      <Messages capture={capture} />

      {available && (
        <section className="panel capture-overview" aria-label="What is being captured">
          <ConversationStrip
            target={shownTarget}
            cards={cardName}
            running={running}
            shown={status.shown}
            listKey={listKey}
            appId={targetApp?.id ?? null}
            owner={ownerLine(serverOwner, serverIp ?? undefined)}
            ownerAbout={ownerTitle(serverOwner, serverIp ?? undefined)}
          />
          <dl className="counters" aria-live="off">
            <div>
              <dt>Packets</dt>
              <dd><Ticker value={status.packets} format={count} /></dd>
            </div>
            {filtered && (
              <div>
                <dt>Shown</dt>
                <dd><Ticker value={status.shown} format={count} /></dd>
              </div>
            )}
            <div>
              <dt>Size</dt>
              <dd><Ticker value={status.bytes} format={bytes} /></dd>
            </div>
            <div>
              <dt>Time</dt>
              <dd>{elapsed(duration)}</dd>
            </div>
            {status.dropped > 0 && (
              <div className="is-alert" title="Packets the capture driver could not keep up with, or that did not fit">
                <dt>Missed</dt>
                <dd>{count(status.dropped)}</dd>
              </div>
            )}
          </dl>
        </section>
      )}

      {support === null ? (
        <p className="empty capture-checking">Checking for the capture driver…</p>
      ) : !available ? (
        <NpcapMissing
          support={support}
          checking={capture.checking}
          onGet={() => void backend.openNpcapDownload().catch(console.error)}
          onCheck={capture.checkSupport}
        />
      ) : (
        <Workspace
          capture={capture}
          shownTarget={shownTarget}
          listKey={listKey}
          plain={plain}
          filterRef={filterRef}
          listRef={listRef}
          selectedRow={selectedRow}
          detail={detail}
          collapsed={collapsed}
          active={active?.key ?? null}
          range={active}
          onPlain={togglePlain}
          onStart={requestStart}
          onSelect={setSelectedRow}
          onToggle={(id) =>
            setCollapsed((current) => {
              const next = new Set(current);
              if (next.has(id)) next.delete(id);
              else next.add(id);
              return next;
            })
          }
          onHover={setHovered}
          onPick={(range) => setPicked((current) => (current?.key === range.key ? null : range))}
        />
      )}
    </div>
  );
}

/** The app being captured, for its icon: the app itself, or the app of one connection. */
function appOf(target: CaptureTarget, apps: AppView[]): AppView | null {
  if (target.kind === "app") return apps.find((app) => app.id === target.appId) ?? null;
  if (target.kind === "connection") return apps.find((app) => app.name === target.appName) ?? null;
  return null;
}

/** One address of the server being captured, to say who runs its network. */
function serverIpOf(target: CaptureTarget, connections: ConnView[], hosts: ReadonlyMap<string, string>): string | null {
  if (target.kind === "host") return target.ip;
  if (target.kind === "connection") return target.remoteIp;
  if (target.kind !== "domain") return null;
  const conn = connections.find((c) => (c.host ?? hosts.get(c.remoteIp)) === target.name);
  if (conn) return conn.remoteIp;
  for (const [ip, host] of hosts) if (host === target.name) return ip;
  return null;
}

/** Problems and notes about the capture, each in plain words. */
function Messages({ capture }: { capture: Capture }) {
  const { status, error, saved } = capture;
  return (
    <>
      {error && (
        <div className="capture-message is-error step-in" role="alert">
          <span>{error}</span>
          <button className="button button-icon button-quiet" aria-label="Dismiss" onClick={capture.dismissError}>
            <CloseIcon />
          </button>
        </div>
      )}
      {status.error && (
        <div className="capture-message is-error step-in" role="alert">
          <span>{status.error}</span>
        </div>
      )}
      {status.stopReason && status.state === "stopped" && (
        <div className="capture-message step-in" role="status">
          <span>{status.stopReason}</span>
        </div>
      )}
      {status.limitReached && (
        <div className="capture-message step-in" role="status">
          <span>
            The capture stopped itself because it is full (200,000 packets or 512 MB). Save it, then start a new one to keep
            going.
          </span>
        </div>
      )}
      {saved && (
        <div className="capture-message is-saved step-in" role="status">
          <span>
            {saved.format === "pcapng" ? "Saved. Open it in Wireshark: " : "Exported: "}
            <span className="mono path">{saved.path}</span>
          </span>
          <CopyButton value={saved.path} what="file path" />
        </div>
      )}
    </>
  );
}

interface WorkspaceProps {
  capture: Capture;
  shownTarget: CaptureTarget;
  listKey: string;
  plain: boolean;
  filterRef: RefObject<HTMLInputElement | null>;
  listRef: RefObject<PacketListHandle | null>;
  selectedRow: PacketRow | null;
  detail: PacketDetail | null;
  collapsed: ReadonlySet<string>;
  active: string | null;
  range: ByteRange | null;
  onPlain(plain: boolean): void;
  onStart(): void;
  onSelect(row: PacketRow): void;
  onToggle(layerId: string): void;
  onHover(range: ByteRange | null): void;
  onPick(range: ByteRange): void;
}

/** The list over the details, with a handle between them to change the split. */
function Workspace(props: WorkspaceProps) {
  const { capture, shownTarget, listKey, plain, filterRef, listRef, selectedRow, detail, onPlain, onSelect } = props;
  const { status } = capture;
  const running = status.state === "running";
  const [detailsHeight, setDetailsHeight] = useState(() => Number(localStorage.getItem(SPLIT_KEY)) || 300);
  const area = useRef<HTMLDivElement>(null);

  const startDrag = (event: ReactPointerEvent) => {
    const element = area.current;
    if (!element) return;
    event.preventDefault();
    const box = element.getBoundingClientRect();
    const move = (e: PointerEvent) => {
      const height = Math.round(Math.min(box.height - 180, Math.max(150, box.bottom - e.clientY)));
      setDetailsHeight(height);
      localStorage.setItem(SPLIT_KEY, String(height));
    };
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  };

  return (
    <div className={`workspace ${selectedRow && detail ? "" : "is-empty"} ${status.shown > 0 ? "" : "is-bare"}`} ref={area} style={{ "--details-height": `${detailsHeight}px` } as CSSProperties}>
      <section className="panel list-panel" aria-label="Packets">
        {status.packets > 0 && (
          <header className="list-head">
            <FilterBar inputRef={filterRef} applied={status.filter} error={status.filterError} onApply={capture.setFilter} />
            <div className="list-tools">
              <ListMenu plain={plain} filter={status.filter} onPlain={onPlain} />
            </div>
          </header>
        )}
        {status.shown > 0 ? (
          <PacketList
            ref={listRef}
            listKey={listKey}
            shown={status.shown}
            running={running}
            plain={plain}
            showApp={shownTarget.kind === "all" || shownTarget.kind === "host" || shownTarget.kind === "domain"}
            selected={selectedRow?.number ?? null}
            onSelect={onSelect}
          />
        ) : (
          <EmptyList capture={capture} target={shownTarget} onStart={props.onStart} />
        )}
      </section>

      {status.shown > 0 && (
        <>
          <div
            className="split-handle"
            role="separator"
            aria-orientation="horizontal"
            aria-label="Drag to resize the packet details"
            onPointerDown={startDrag}
          />

          <section className="panel details-split" aria-label="Packet details">
            {selectedRow && detail ? (
              <PacketView key={detail.number} row={selectedRow} {...props} detail={detail} />
            ) : (
              <p className="empty details-hint">
                Pick a packet to see what is inside it, layer by layer, in plain words. Arrow keys move through the list.
              </p>
            )}
          </section>
        </>
      )}
    </div>
  );
}

function PacketView({ row, detail, collapsed, active, range, onToggle, onHover, onPick }: WorkspaceProps & { row: PacketRow; detail: PacketDetail }) {
  const describe = (name: string | null, address: string, port: number | null) =>
    `${address}${port !== null ? `:${port}` : ""}${name && name !== address ? ` (${name})` : ""}`;
  const text = useMemo(() => breakdownText(detail, `Packet ${detail.number}: ${row.summary}`), [detail, row.summary]);
  const hoverByte = (offset: number | null) => onHover(offset === null ? null : rangeAt(detail, offset));
  const pickByte = (offset: number) => {
    const found = rangeAt(detail, offset);
    if (found) onPick(found);
  };

  return (
    <div className="packet-view">
      <header className="packet-view-head">
        <div className="packet-view-title">
          <h2 className="panel-title">
            Packet {detail.number}
            <span className="packet-view-meta">
              {detail.time.toFixed(3)} s · {count(detail.length)} bytes · {detail.interfaceName}
              {detail.app ? ` · ${detail.app}` : ""}
            </span>
          </h2>
          <p className="packet-view-summary">
            <span>{row.summary}</span>
            <CopyButton value={row.summary} what="summary" />
          </p>
        </div>
        <div className="packet-view-copy">
          <span className="copy-pair" title={describe(row.sourceName, row.source, row.sourcePort)}>
            From <span className="mono">{row.sourceName ?? row.source}</span>
            <CopyButton value={row.sourcePort !== null ? `${row.source}:${row.sourcePort}` : row.source} what="source address" />
          </span>
          <span className="copy-pair" title={describe(row.destinationName, row.destination, row.destinationPort)}>
            To <span className="mono">{row.destinationName ?? row.destination}</span>
            <CopyButton value={row.destinationPort !== null ? `${row.destination}:${row.destinationPort}` : row.destination} what="destination address" />
          </span>
          <CopyButton value={text} what="the whole packet as text">Copy packet</CopyButton>
        </div>
      </header>
      <div className="packet-view-body">
        <PacketDetails detail={detail} collapsed={collapsed} active={active} onToggle={onToggle} onHover={onHover} onPick={onPick} />
        <HexView bytes={detail.bytes} start={range?.start ?? 0} length={range?.length ?? 0} onHoverByte={hoverByte} onPickByte={pickByte} />
      </div>
    </div>
  );
}

/** Before the first packet, or when the filter hides everything. */
function EmptyList({ capture, target, onStart }: { capture: Capture; target: CaptureTarget; onStart(): void }) {
  const { status } = capture;
  if (status.packets > 0) {
    return (
      <div className="list-empty">
        <h2>No packets match this search</h2>
        <p>
          {count(status.packets)} packets are hidden by <code>{status.filter}</code>. Clear the search to see them all.
        </p>
        <button className="button" onClick={() => capture.setFilter("")}>Show all packets</button>
      </div>
    );
  }
  if (status.state === "running") {
    return (
      <div className="list-empty">
        <h2>Listening…</h2>
        <p>Packets appear here as soon as {targetLabel(target) === "Everything" ? "anything" : targetLabel(target)} sends or receives something.</p>
        {target.kind === "app" && <p className="muted">{DNS_NOTE}</p>}
      </div>
    );
  }
  return (
    <div className="list-empty ready">
      <h2>Ready to record</h2>
      <p>{targetDescription(target)}</p>
      <p className="privacy">
        <LockIcon />
        <span>
          Encrypted traffic (HTTPS, most of the web) stays private: you will see who talked to whom, when and how much, but
          not what was said.
        </span>
      </p>
      {target.kind === "app" && <p className="muted">{DNS_NOTE}</p>}
      <button className="button button-solid button-big" disabled={capture.busy !== null} onClick={onStart}>
        <RecordIcon /> Start recording
      </button>
    </div>
  );
}

/** The quiet "…" beside the search box: how packets are described, and copying the search. */
function ListMenu({ plain, filter, onPlain }: { plain: boolean; filter: string; onPlain(plain: boolean): void }) {
  const [open, setOpen] = useState(false);
  const root = useRef<HTMLDivElement>(null);
  const close = useCallback(() => setOpen(false), []);
  useDismiss(root, open, close);

  const option = (value: boolean, title: string, sub: string) => (
    <li>
      <button
        className={`picker-option ${plain === value ? "is-selected" : ""}`}
        aria-pressed={plain === value}
        onClick={() => {
          onPlain(value);
          setOpen(false);
        }}
      >
        <span className="picker-option-text">
          <strong>{title}</strong>
          <span>{sub}</span>
        </span>
        {plain === value && <CheckIcon />}
      </button>
    </li>
  );

  return (
    <div className="popover" ref={root}>
      <button className="button button-icon" aria-label="List options" title="List options" aria-expanded={open} onClick={() => setOpen((v) => !v)}>
        …
      </button>
      {open && (
        <div className="menu list-menu" role="dialog" aria-label="List options">
          <h2 className="section-title">Describe each packet</h2>
          <ul className="picker-list">
            {option(true, "In plain words", "This PC asks github.com to open a connection")}
            {option(false, "Technically, like Wireshark", "51234 → 443 [SYN] Seq=0 Win=64240 Len=0")}
          </ul>
          {filter && (
            <div className="list-menu-copy">
              <span className="mono">{filter}</span>
              <CopyButton value={filter} what="search" className="has-text">
                Copy search
              </CopyButton>
            </div>
          )}
        </div>
      )}
    </div>
  );
}
