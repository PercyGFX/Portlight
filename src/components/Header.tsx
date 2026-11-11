// Top bar: name, live totals, search, alerts and the theme switch.

import { useCallback, useRef, useState, type RefObject } from "react";

import { useDismiss } from "../hooks/useDismiss";
import type { Theme } from "../hooks/useTheme";
import type { Updates } from "../hooks/useUpdates";
import { plural } from "../lib/format";
import type { AppView } from "../lib/types";
import { BellIcon, CheckIcon, Logo, MoonIcon, SearchIcon, SunIcon } from "./icons";
import { UpdateMenu } from "./UpdateMenu";

interface Props {
  appsOnline: number;
  connections: number;
  query: string;
  searchRef: RefObject<HTMLInputElement | null>;
  theme: Theme;
  newApps: AppView[];
  updates: Updates;
  demo: boolean;
  onQuery(query: string): void;
  onTheme(theme: Theme): void;
  onSelect(id: string): void;
  onTrust(id: string): void;
  onTrustAll(): void;
}

export function Header(props: Props) {
  const { appsOnline, connections, query, searchRef, theme, newApps, demo } = props;

  return (
    <header className="topbar">
      <div className="brand">
        <Logo />
        <div>
          <h1>Portlight</h1>
          <p>See which apps are online</p>
        </div>
      </div>

      <div className="totals">
        <div><strong>{appsOnline}</strong><span>Apps online</span></div>
        <div><strong>{connections}</strong><span>Connections</span></div>
      </div>

      {demo && (
        <span className="tag tag-demo" title="This is sample data, not your PC. Run the desktop app to see real connections.">
          Demo data
        </span>
      )}

      <label className="search">
        <SearchIcon />
        <input
          ref={searchRef}
          type="search"
          placeholder="Search apps and servers"
          value={query}
          onChange={(e) => props.onQuery(e.target.value)}
          aria-label="Search apps and servers"
        />
        <kbd>Ctrl K</kbd>
      </label>

      <Alerts newApps={newApps} onSelect={props.onSelect} onTrust={props.onTrust} onTrustAll={props.onTrustAll} />
      <UpdateMenu updates={props.updates} />

      <div className="segmented" role="group" aria-label="Theme">
        <button
          className={theme === "light" ? "is-active" : ""}
          aria-pressed={theme === "light"}
          aria-label="Light mode"
          onClick={() => props.onTheme("light")}
        >
          <SunIcon />
        </button>
        <button
          className={theme === "dark" ? "is-active" : ""}
          aria-pressed={theme === "dark"}
          aria-label="Dark mode"
          onClick={() => props.onTheme("dark")}
        >
          <MoonIcon />
        </button>
      </div>
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
    <div className="alerts" ref={root}>
      <button
        className={`button button-icon ${newApps.length ? "has-alert" : ""}`}
        aria-expanded={open}
        aria-label={`New apps: ${newApps.length}`}
        onClick={() => setOpen((v) => !v)}
      >
        <BellIcon />
        {newApps.length > 0 && <span className="count">{newApps.length}</span>}
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
