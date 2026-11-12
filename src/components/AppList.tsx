// The app list in the sidebar: every app seen online, busiest first.

import { useState } from "react";

import { plural } from "../lib/format";
import type { AppView } from "../lib/types";
import { AppIcon } from "./AppIcon";
import { ShieldIcon } from "./icons";

type Tab = "all" | "online" | "new";

interface Props {
  apps: AppView[];
  selectedId: string | null;
  /** True when the search box is hiding some apps. */
  filtered: boolean;
  onSelect(id: string | null): void;
}

export function AppList({ apps, selectedId, filtered, onSelect }: Props) {
  const [tab, setTab] = useState<Tab>("all");

  const online = apps.filter((app) => app.connections > 0);
  const fresh = apps.filter((app) => !app.trusted);
  const shown = tab === "online" ? online : tab === "new" ? fresh : apps;
  const busiest = Math.max(1, ...apps.map((app) => app.connections));

  const tabs: Array<[Tab, string, number]> = [
    ["all", "All", apps.length],
    ["online", "Online", online.length],
    ["new", "New", fresh.length],
  ];

  return (
    <section className="app-section" aria-label="Apps">
      <div className="tabs" role="tablist">
        {tabs.map(([id, label, count]) => (
          <button
            key={id}
            role="tab"
            aria-selected={tab === id}
            className={`tab ${tab === id ? "is-active" : ""} ${id === "new" && count > 0 ? "has-alert" : ""}`}
            onClick={() => setTab(id)}
          >
            {label} <span className="tab-count">{count}</span>
          </button>
        ))}
      </div>

      <ul className="app-list">
        {shown.map((app) => (
          <li key={app.id}>
            <button
              className={`app-row ${app.id === selectedId ? "is-selected" : ""} ${app.trusted ? "" : "is-new"}`}
              aria-pressed={app.id === selectedId}
              onClick={() => onSelect(app.id === selectedId ? null : app.id)}
            >
              <AppIcon appId={app.id} name={app.name} online={app.connections > 0} />
              <span className="app-row-text">
                <span className="app-row-name">
                  <span>{app.name}</span>
                  {app.signature.state === "unsigned" && (
                    <span className="unsigned-mark" title="Not signed by its maker">
                      <ShieldIcon />
                    </span>
                  )}
                  {!app.trusted && <span className="tag tag-new">New</span>}
                </span>
                <span className="app-row-sub">
                  {app.connections > 0
                    ? `${plural(app.connections, "connection")} · ${plural(app.servers, "server")}`
                    : "Not connected right now"}
                </span>
                <span className="meter">
                  <i style={{ width: `${(app.connections / busiest) * 100}%` }} />
                </span>
              </span>
            </button>
          </li>
        ))}
        {shown.length === 0 && (
          <li className="empty">
            {filtered
              ? "No app matches your search."
              : tab === "new"
                ? "No new apps. You will be told when one goes online."
                : "No apps yet."}
          </li>
        )}
      </ul>
    </section>
  );
}
