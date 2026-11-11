// Strip above the map: live totals on the left, the new-app bell on the right.

import { useCallback, useRef, useState } from "react";

import { useDismiss } from "../hooks/useDismiss";
import { plural } from "../lib/format";
import type { AppView } from "../lib/types";
import { BellIcon, CheckIcon } from "./icons";

interface Props {
  appsOnline: number;
  connections: number;
  servers: number;
  paused: boolean;
  newApps: AppView[];
  demo: boolean;
  onSelect(id: string): void;
  onTrust(id: string): void;
  onTrustAll(): void;
}

export function Topbar({ appsOnline, connections, servers, paused, newApps, demo, onSelect, onTrust, onTrustAll }: Props) {
  return (
    <header className="topbar">
      <span className={`pill pill-live ${paused ? "is-paused" : ""}`}>
        <span className="live-dot" />
        {paused ? "Paused" : "Live"}
      </span>

      <dl className="totals">
        <div><dt>Apps online</dt><dd>{appsOnline}</dd></div>
        <div><dt>Connections</dt><dd>{connections}</dd></div>
        <div><dt>Servers</dt><dd>{servers}</dd></div>
      </dl>

      {demo && (
        <span className="pill pill-demo" title="This is sample data, not your PC. Run the desktop app to see real connections.">
          Demo data
        </span>
      )}

      <Alerts newApps={newApps} onSelect={onSelect} onTrust={onTrust} onTrustAll={onTrustAll} />
    </header>
  );
}

type AlertsProps = Pick<Props, "newApps" | "onSelect" | "onTrust" | "onTrustAll">;

/** The bell: a count of new apps and a menu to review them. */
function Alerts({ newApps, onSelect, onTrust, onTrustAll }: AlertsProps) {
  const [open, setOpen] = useState(false);
  const root = useRef<HTMLDivElement>(null);
  useDismiss(root, open, useCallback(() => setOpen(false), []));

  return (
    <div className="popover alerts" ref={root}>
      <button
        className={`button ${newApps.length ? "has-alert" : ""}`}
        aria-expanded={open}
        aria-label={`New apps: ${newApps.length}`}
        onClick={() => setOpen((v) => !v)}
      >
        <BellIcon />
        {newApps.length ? plural(newApps.length, "new app") : "No new apps"}
      </button>

      {open && (
        <div className="menu" role="dialog" aria-label="New apps">
          <div className="menu-head">
            <h2 className="section-title">{newApps.length ? plural(newApps.length, "new app") : "No new apps"}</h2>
            {newApps.length > 1 && (
              <button className="button button-quiet" onClick={onTrustAll}>Trust all</button>
            )}
          </div>
          {newApps.length === 0 && (
            <p className="menu-text">
              Portlight tells you here, and with a Windows notification, the first time an app goes online.
            </p>
          )}
          <ul className="menu-list">
            {newApps.map((app) => (
              <li key={app.id}>
                <button
                  className="link menu-app"
                  onClick={() => {
                    onSelect(app.id);
                    setOpen(false);
                  }}
                >
                  <strong>{app.name}</strong>
                  <span>{app.path ?? "System process"}</span>
                </button>
                <button className="button" onClick={() => onTrust(app.id)}>
                  <CheckIcon /> Trust
                </button>
              </li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}
