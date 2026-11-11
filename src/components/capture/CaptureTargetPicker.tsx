// Chooses what to capture: everything, one app, one server, or a site typed in.

import { useCallback, useMemo, useRef, useState } from "react";

import { useDismiss } from "../../hooks/useDismiss";
import { plural, shorten } from "../../lib/format";
import { EVERYTHING, sameTarget, serverTarget, targetLabel, typedTarget } from "../../lib/targets";
import type { AppView, CaptureTarget, ConnView } from "../../lib/types";
import { CheckIcon, ChevronDownIcon } from "../icons";

interface Props {
  target: CaptureTarget;
  apps: AppView[];
  connections: ConnView[];
  hosts: ReadonlyMap<string, string>;
  disabled?: boolean;
  onChange(target: CaptureTarget): void;
}

const MAX_SERVERS = 8;

export function CaptureTargetPicker({ target, apps, connections, hosts, disabled, onChange }: Props) {
  const [open, setOpen] = useState(false);
  const [typed, setTyped] = useState("");
  const root = useRef<HTMLDivElement>(null);
  const close = useCallback(() => setOpen(false), []);
  useDismiss(root, open, close);

  const onlineApps = useMemo(() => apps.filter((app) => app.connections > 0), [apps]);
  const servers = useMemo(() => {
    const counts = new Map<string, { ip: string; host: string | null; count: number }>();
    for (const conn of connections) {
      const entry = counts.get(conn.remoteIp) ?? { ip: conn.remoteIp, host: null, count: 0 };
      entry.count += 1;
      entry.host = conn.host ?? hosts.get(conn.remoteIp) ?? entry.host;
      counts.set(conn.remoteIp, entry);
    }
    return [...counts.values()].sort((a, b) => b.count - a.count || a.ip.localeCompare(b.ip)).slice(0, MAX_SERVERS);
  }, [connections, hosts]);

  const choose = (next: CaptureTarget) => {
    onChange(next);
    setOpen(false);
    setTyped("");
  };

  const option = (key: string, next: CaptureTarget, title: string, sub?: string) => {
    const selected = sameTarget(next, target);
    return (
      <li key={key}>
        <button className={`picker-option ${selected ? "is-selected" : ""}`} aria-pressed={selected} onClick={() => choose(next)}>
          <span className="picker-option-text">
            <strong>{title}</strong>
            {sub && <span>{sub}</span>}
          </span>
          {selected && <CheckIcon />}
        </button>
      </li>
    );
  };

  const typedChoice = typedTarget(typed);

  return (
    <div className="popover" ref={root}>
      <button className="button picker-button" aria-expanded={open} disabled={disabled} onClick={() => setOpen((v) => !v)}>
        <span className="picker-value">{shorten(targetLabel(target), 34)}</span> <ChevronDownIcon />
      </button>
      {open && (
        <div className="menu menu-left picker-menu" role="dialog" aria-label="What to capture">
          <ul className="picker-list">
            {target.kind === "connection" && option("current", target, "This connection", targetLabel(target))}
            {option("all", EVERYTHING, "Everything", "Every app, plus your home network")}
          </ul>

          <h2 className="section-title">An app</h2>
          <ul className="picker-list">
            {onlineApps.map((app) =>
              option(app.id, { kind: "app", appId: app.id, name: app.name }, app.name, plural(app.connections, "connection")),
            )}
            {onlineApps.length === 0 && <li className="empty">No app is online right now.</li>}
          </ul>

          <h2 className="section-title">A server</h2>
          <ul className="picker-list">
            {servers.map((server) =>
              option(
                server.ip,
                serverTarget(server.ip, server.host),
                server.host ?? server.ip,
                `${server.host ? `${server.ip} · ` : ""}${plural(server.count, "connection")}`,
              ),
            )}
          </ul>

          <form
            className="picker-typed"
            onSubmit={(event) => {
              event.preventDefault();
              if (typedChoice) choose(typedChoice);
            }}
          >
            <label className="form-field">
              <span>Or type a site name or address</span>
              <input
                value={typed}
                placeholder="github.com or 203.0.113.9"
                spellCheck={false}
                onChange={(event) => setTyped(event.target.value)}
              />
            </label>
            <button className="button" type="submit" disabled={!typedChoice}>Use</button>
          </form>
        </div>
      )}
    </div>
  );
}
