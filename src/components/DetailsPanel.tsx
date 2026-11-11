// Right panel: the big picture, or one app's details when an app is selected.

import { ago, plural, shorten } from "../lib/format";
import { serviceOf } from "../lib/services";
import type { AppView, ConnView } from "../lib/types";
import { BackIcon, CheckIcon } from "./icons";

interface Props {
  app: AppView | null;
  apps: AppView[];
  connections: ConnView[];
  hosts: ReadonlyMap<string, string>;
  now: number;
  onTrust(id: string): void;
  onBack(): void;
}

export function DetailsPanel({ app, apps, connections, hosts, now, onTrust, onBack }: Props) {
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
        <AppDetails key={app.id} app={app} connections={connections} hosts={hosts} now={now} onTrust={onTrust} />
      ) : (
        <Overview apps={apps} connections={connections} hosts={hosts} />
      )}
    </section>
  );
}

function Overview({ apps, connections, hosts }: Pick<Props, "apps" | "connections" | "hosts">) {
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
        {busiest.map(([ip, count]) => (
          <li key={ip}>
            <span className="kv-main" title={hosts.get(ip) ?? ip}>{shorten(hosts.get(ip) ?? ip, 26)}</span>
            <span className="kv-side">{plural(count, "connection")}</span>
          </li>
        ))}
        {busiest.length === 0 && <li className="empty">Nothing is connected.</li>}
      </ul>
    </div>
  );
}

function AppDetails({ app, connections, hosts, now, onTrust }: Pick<Props, "connections" | "hosts" | "now" | "onTrust"> & { app: AppView }) {
  const mine = connections.filter((c) => c.appId === app.id);
  const badge = !app.trusted ? ["New", "badge-new"] : app.connections > 0 ? ["Online", "badge-online"] : ["Idle", "badge-idle"];

  return (
    <div className="details-body step-in">
      <div className="details-title">
        <h3>{app.name}</h3>
        <span className={`badge ${badge[1]}`}>{badge[0]}</span>
      </div>
      <p className="details-exe">{app.exe || "Processes Windows will not name"}</p>

      <p className="details-text">
        {app.connections > 0
          ? `${app.name} has ${plural(app.connections, "open connection")} to ${plural(app.servers, "server")}.`
          : `${app.name} is not connected to anything right now.`}
        {app.path === null && " Run Portlight as administrator to see these processes by name."}
      </p>

      {!app.trusted && (
        <div className="callout">
          <p>This is the first time Portlight has seen this app go online. If you know it, mark it as trusted.</p>
          <button className="button button-solid" onClick={() => onTrust(app.id)}>
            <CheckIcon /> Mark as trusted
          </button>
        </div>
      )}

      <dl className="facts">
        <dt>Program</dt>
        <dd className="is-path">{app.path ?? "Hidden by Windows"}</dd>
        <dt>First seen</dt>
        <dd>{ago(app.firstSeen, now)}</dd>
        <dt>Open now</dt>
        <dd>{plural(app.connections, "connection")}</dd>
        <dt>Opened</dt>
        <dd>{app.totalOpened} since Portlight started</dd>
      </dl>

      <h3 className="section-title">Connected to</h3>
      <ul className="kv-list">
        {mine.map((conn) => {
          const host = conn.host ?? hosts.get(conn.remoteIp);
          return (
            <li key={conn.id}>
              <span className="kv-main" title={host ? `${host} (${conn.remoteIp})` : conn.remoteIp}>
                {shorten(host ?? conn.remoteIp, 24)}
              </span>
              <span className="kv-side">{serviceOf(conn.remotePort).label}</span>
            </li>
          );
        })}
        {mine.length === 0 && <li className="empty">No open connections.</li>}
      </ul>
    </div>
  );
}

function Stat({ label, value, alert = false }: { label: string; value: number; alert?: boolean }) {
  return (
    <div className={`stat ${alert ? "is-alert" : ""}`}>
      <span className="stat-value">{value}</span>
      <span className="stat-label">{label}</span>
    </div>
  );
}
