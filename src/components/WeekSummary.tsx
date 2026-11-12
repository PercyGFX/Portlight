// Overview card: which apps were busiest over the last seven days, from history.

import { useMemo } from "react";

import { useHistorySummary, type History } from "../hooks/useHistory";
import { bytes, plural } from "../lib/format";
import type { AppView } from "../lib/types";
import { AppIcon } from "./AppIcon";

const SHOWN = 5;

export function WeekSummary({ apps, history, onSelect }: { apps: AppView[]; history: History; onSelect(id: string): void }) {
  const enabled = history.settings?.enabled ?? false;
  const rows = useHistorySummary(7, enabled, history.version);

  const top = useMemo(() => {
    const totals = new Map<string, { appId: string; name: string; connections: number; data: number }>();
    for (const row of rows ?? []) {
      const entry = totals.get(row.appId) ?? { appId: row.appId, name: row.appName, connections: 0, data: 0 };
      entry.connections += row.connections;
      entry.data += row.bytesSent + row.bytesReceived;
      totals.set(row.appId, entry);
    }
    return [...totals.values()].sort((a, b) => b.connections - a.connections || b.data - a.data).slice(0, SHOWN);
  }, [rows]);

  if (!enabled || top.length === 0) return null;
  const busiest = top[0].connections || 1;

  return (
    <>
      <h3 className="section-title">Last 7 days</h3>
      <ul className="week-list">
        {top.map((entry) => {
          const known = apps.some((app) => app.id === entry.appId);
          const body = (
            <>
              <AppIcon appId={entry.appId} name={entry.name} size={24} />
              <span className="week-text">
                <span className="week-name">{entry.name}</span>
                <span className="week-sub">
                  {plural(entry.connections, "connection")}
                  {entry.data > 0 && ` · ${bytes(entry.data)}`}
                </span>
                <span className="meter">
                  <i style={{ width: `${(entry.connections / busiest) * 100}%` }} />
                </span>
              </span>
            </>
          );
          return (
            <li key={entry.appId}>
              {known ? (
                <button className="week-row" onClick={() => onSelect(entry.appId)} title={`See ${entry.name}`}>
                  {body}
                </button>
              ) : (
                <div className="week-row">{body}</div>
              )}
            </li>
          );
        })}
      </ul>
    </>
  );
}
