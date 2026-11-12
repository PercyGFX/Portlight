// Right panel: the big picture, or one app's details when an app is selected.

import { useRef, useState } from "react";

import { useHistorySettings, type History } from "../hooks/useHistory";
import { ago, bytes, plural, rate, shorten } from "../lib/format";
import { ownerLine, ownerTitle, type Owner } from "../lib/owners";
import { serviceOf } from "../lib/services";
import { connectionTarget, serverTarget } from "../lib/targets";
import type { AppView, CaptureTarget, ConnView, Signature, Source } from "../lib/types";
import { AppIcon } from "./AppIcon";
import { CopyButton } from "./CopyButton";
import { HistoryView } from "./HistoryView";
import { BackIcon, CheckIcon, RecordIcon, ShieldCheckIcon, ShieldIcon } from "./icons";
import { WeekSummary } from "./WeekSummary";

type Tab = "now" | "history";

interface Props {
  app: AppView | null;
  apps: AppView[];
  connections: ConnView[];
  hosts: ReadonlyMap<string, string>;
  owners: ReadonlyMap<string, Owner>;
  now: number;
  source: Source | null;
  onTrust(id: string): void;
  onBack(): void;
  onSelect(id: string): void;
  onCapture(target: CaptureTarget): void;
}

export function DetailsPanel({ app, apps, connections, hosts, owners, now, source, onTrust, onBack, onSelect, onCapture }: Props) {
  const history = useHistorySettings();
  // Kept here, so moving from app to app stays on the same tab.
  const [tab, setTab] = useState<Tab>("now");

  return (
    <section className="panel details-panel" aria-label={app ? "App details" : "Overview"}>
      <header className="panel-head">
        <h2 className="panel-title">{app ? "App details" : "Overview"}</h2>
        {app && (
          <button className="button button-quiet" onClick={onBack}>
            <BackIcon /> Overview
          </button>
        )}
      </header>
      {app ? (
        <AppDetails
          key={app.id}
          app={app}
          connections={connections}
          hosts={hosts}
          now={now}
          detailed={source === "detailed"}
          tab={tab}
          history={history}
          onTab={setTab}
          onTrust={onTrust}
          onCapture={onCapture}
        />
      ) : (
        <Overview
          apps={apps}
          connections={connections}
          hosts={hosts}
          owners={owners}
          history={history}
          onSelect={onSelect}
          onCapture={onCapture}
        />
      )}
    </section>
  );
}

type OverviewProps = Pick<Props, "apps" | "connections" | "hosts" | "owners" | "onSelect" | "onCapture"> & { history: History };

function Overview({ apps, connections, hosts, owners, history, onSelect, onCapture }: OverviewProps) {
  const online = apps.filter((a) => a.connections > 0).length;
  const fresh = apps.filter((a) => !a.trusted).length;

  const counts = new Map<string, number>();
  for (const conn of connections) counts.set(conn.remoteIp, (counts.get(conn.remoteIp) ?? 0) + 1);
  const busiest = [...counts.entries()].sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0])).slice(0, 6);

  return (
    <div className="details-body">
      <div className="stat-grid">
        <Stat label="Apps online" value={online} />
        <Stat label="Connections" value={connections.length} />
        <Stat label="Servers" value={counts.size} />
        <Stat label="New apps" value={fresh} alert={fresh > 0} />
      </div>

      <p className="details-text">
        Every app that is talking to the internet right now. Pick one to see exactly who it is connected to.
      </p>

      <h3 className="section-title">Busiest servers</h3>
      <ul className="kv-list">
        {busiest.map(([ip, count]) => {
          const host = hosts.get(ip);
          const owner = owners.get(ip);
          const where = ownerLine(owner, ip);
          return (
            <li key={ip} className="has-action">
              <span className="kv-stack">
                <span className="kv-main" title={host ? `${host} (${ip})` : ip}>{shorten(host ?? ip, 24)}</span>
                {where && <span className="kv-owner" title={ownerTitle(owner, ip) ?? undefined}>{where}</span>}
              </span>
              <span className="kv-side">{plural(count, "connection")}</span>
              <span className="kv-copy">
                {host && <CopyButton value={host} what="site name" />}
                <CopyButton value={ip} what="address" />
              </span>
              <CaptureButton label={`Capture packets to and from ${host ?? ip}`} onClick={() => onCapture(serverTarget(ip, host))} />
            </li>
          );
        })}
        {busiest.length === 0 && <li className="empty">Nothing is connected.</li>}
      </ul>

      <WeekSummary apps={apps} history={history} onSelect={onSelect} />
    </div>
  );
}

interface AppDetailsProps {
  app: AppView;
  connections: ConnView[];
  hosts: ReadonlyMap<string, string>;
  now: number;
  detailed: boolean;
  tab: Tab;
  history: History;
  onTab(tab: Tab): void;
  onTrust(id: string): void;
  onCapture(target: CaptureTarget): void;
}

function AppDetails({ app, connections, hosts, now, detailed, tab, history, onTab, onTrust, onCapture }: AppDetailsProps) {
  const mine = connections.filter((c) => c.appId === app.id);
  const badge = !app.trusted ? ["New", "badge-new"] : app.connections > 0 ? ["Online", "badge-online"] : ["Idle", "badge-idle"];
  const speed = useRate(app, now);

  return (
    <div className="details-body step-in">
      <div className="details-title">
        <AppIcon appId={app.id} name={app.name} size={40} className="details-icon" />
        <div className="details-name">
          <h3>{app.name}</h3>
          <p className="details-exe has-copy">
            <span>{app.exe || "Processes Windows will not name"}</span>
            {app.exe && <CopyButton value={app.exe} what="program name" />}
          </p>
        </div>
        <span className={`badge ${badge[1]}`}>{badge[0]}</span>
      </div>
      {app.path !== null && <SignatureNote signature={app.signature} />}

      <p className="details-text">
        {app.connections > 0
          ? `${app.name} has ${plural(app.connections, "open connection")} to ${plural(app.servers, "server")}.`
          : `${app.name} is not connected to anything right now.`}
        {app.path === null && !detailed && " Turn on detailed monitoring to see these processes by name."}
      </p>

      {!app.trusted && (
        <div className="callout">
          <p>This is the first time Portlight has seen this app go online. If you know it, mark it as trusted.</p>
          <button className="button button-solid" onClick={() => onTrust(app.id)}>
            <CheckIcon /> Mark as trusted
          </button>
        </div>
      )}

      <button
        className="button button-solid button-wide"
        onClick={() => onCapture({ kind: "app", appId: app.id, name: app.name })}
      >
        <RecordIcon /> Capture this app's packets
      </button>

      <div className="segmented details-tabs" role="tablist" aria-label="Show">
        <button role="tab" aria-selected={tab === "now"} className={tab === "now" ? "is-active" : ""} onClick={() => onTab("now")}>
          Right now
        </button>
        <button role="tab" aria-selected={tab === "history"} className={tab === "history" ? "is-active" : ""} onClick={() => onTab("history")}>
          History
        </button>
      </div>

      {tab === "history" ? (
        <HistoryView app={app} history={history} />
      ) : (
        <div className="step-in">
          <dl className="facts">
            <dt>Program</dt>
            <dd className="is-path has-copy copyable">
              <span>{app.path ?? "Hidden by Windows"}</span>
              {app.path && <CopyButton value={app.path} what="program path" />}
            </dd>
            <dt>First seen</dt>
            <dd>{ago(app.firstSeen, now)}</dd>
            <dt>Open now</dt>
            <dd>{plural(app.connections, "connection")}</dd>
            <dt>Opened</dt>
            <dd>{app.totalOpened} since Portlight started</dd>
            <dt>Data used</dt>
            {detailed ? (
              <dd className="data-used">
                <span>
                  <strong>{bytes(app.bytesReceived)}</strong> down · <strong>{bytes(app.bytesSent)}</strong> up
                </span>
                <span className="data-rate" title="Over the last few seconds">
                  {speed ? `now ${rate(speed.down)} down · ${rate(speed.up)} up` : "measuring…"}
                </span>
              </dd>
            ) : (
              <dd className="muted">Turn on detailed monitoring to see data used.</dd>
            )}
          </dl>

          <h3 className="section-title">Connected to</h3>
          <ul className="kv-list">
            {mine.map((conn) => {
              const host = conn.host ?? hosts.get(conn.remoteIp);
              const service = conn.protocol === "udp" && conn.remotePort === 443 ? "QUIC" : serviceOf(conn.remotePort).label;
              const where = ownerLine(conn, conn.remoteIp);
              return (
                <li key={conn.id} className="has-action">
                  <span className="kv-stack">
                    <button
                      className="link kv-main"
                      title={`${host ? `${host} (${conn.remoteIp})` : conn.remoteIp}. Click to capture every packet to and from this server.`}
                      onClick={() => onCapture(serverTarget(conn.remoteIp, host))}
                    >
                      {shorten(host ?? conn.remoteIp, 22)}
                    </button>
                    {where && <span className="kv-owner" title={ownerTitle(conn, conn.remoteIp) ?? undefined}>{where}</span>}
                  </span>
                  {conn.protocol === "udp" && <span className="tag tag-udp" title="UDP: quick messages without a connection">UDP</span>}
                  <span className="kv-side" title={detailed ? `${bytes(conn.bytesReceived)} down, ${bytes(conn.bytesSent)} up` : undefined}>
                    {service}
                    {detailed && ` · ${bytes(conn.bytesReceived + conn.bytesSent)}`}
                  </span>
                  <span className="kv-copy">
                    {host && <CopyButton value={host} what="site name" />}
                    <CopyButton value={`${conn.remoteIp}:${conn.remotePort}`} what="address and port" />
                  </span>
                  <CaptureButton
                    label={`Capture this connection only (${app.name} ↔ ${host ?? conn.remoteIp}:${conn.remotePort})`}
                    onClick={() => onCapture(connectionTarget(conn, app.name, host))}
                  />
                </li>
              );
            })}
            {mine.length === 0 && <li className="empty">No open connections.</li>}
          </ul>
        </div>
      )}
    </div>
  );
}

const UNSIGNED_NOTE = "Most well-known apps are signed by their maker. Unsigned isn't always bad, but it's worth knowing who made it.";

/** Who made the program, from its digital signature. */
function SignatureNote({ signature }: { signature: Signature }) {
  if (signature.state === "signed") {
    return (
      <p className="signature is-signed" title="Windows checked the program's digital signature on this PC.">
        <ShieldCheckIcon />
        <span>
          Signed by <strong>{signature.publisher}</strong>
        </span>
      </p>
    );
  }
  if (signature.state === "unsigned") {
    return (
      <div className="signature is-unsigned">
        <ShieldIcon />
        <span>
          <strong>Not signed</strong>
          <span>{UNSIGNED_NOTE}</span>
        </span>
      </div>
    );
  }
  return <p className="signature is-checking">Checking the signature…</p>;
}

/** Small round "record" button at the end of a list row. */
function CaptureButton({ label, onClick }: { label: string; onClick(): void }) {
  return (
    <button className="button button-icon button-quiet row-action" aria-label={label} title={label} onClick={onClick}>
      <RecordIcon />
    </button>
  );
}

/** Bytes per second, smoothed over consecutive snapshots. Null until two snapshots have arrived. */
function useRate(app: AppView, now: number): { down: number; up: number } | null {
  const memory = useRef<{ now: number; sent: number; received: number; down: number; up: number; ready: boolean } | null>(null);
  const last = memory.current;
  if (!last) {
    memory.current = { now, sent: app.bytesSent, received: app.bytesReceived, down: 0, up: 0, ready: false };
  } else if (now > last.now) {
    const seconds = (now - last.now) / 1000;
    const down = Math.max(0, app.bytesReceived - last.received) / seconds;
    const up = Math.max(0, app.bytesSent - last.sent) / seconds;
    // Smooth the bursts so the figure is readable.
    const mix = last.ready ? 0.3 : 1;
    memory.current = {
      now,
      sent: app.bytesSent,
      received: app.bytesReceived,
      down: last.down + (down - last.down) * mix,
      up: last.up + (up - last.up) * mix,
      ready: true,
    };
  }
  const current = memory.current;
  return current && current.ready ? { down: current.down, up: current.up } : null;
}

function Stat({ label, value, alert = false }: { label: string; value: number; alert?: boolean }) {
  return (
    <div className={`stat ${alert ? "is-alert" : ""}`}>
      <span className="stat-value">{value}</span>
      <span className="stat-label">{label}</span>
    </div>
  );
}
