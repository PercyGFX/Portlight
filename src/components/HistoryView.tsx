// One app's history in the details column: a row per day, tap a day to see its servers.

import { useState } from "react";

import { useAppHistory, type History } from "../hooks/useHistory";
import { bytes, dayLabel, hourMinute, plural, shorten } from "../lib/format";
import { ownerLine, ownerTitle } from "../lib/owners";
import type { AppView, HistoryDay } from "../lib/types";
import { ChevronRightIcon, HistoryIcon } from "./icons";

/** Servers shown for a day before "Show all". */
const FIRST_SERVERS = 8;

export function HistoryView({ app, history }: { app: AppView; history: History }) {
  const [days, setDays] = useState(7);
  const { settings, version } = history;
  const enabled = settings?.enabled ?? false;
  const result = useAppHistory(app.id, settings && enabled ? days : null, version);
  const [open, setOpen] = useState<string | null>(null);

  if (!settings) return <p className="empty">Loading…</p>;

  if (!enabled) {
    return (
      <div className="history-off step-in">
        <HistoryIcon />
        <div>
          <p>
            <strong>History is off.</strong> Turn it on to keep a daily record of which servers each app talked to. Only
            counts are kept, never what was sent.
          </p>
          <button className="button button-solid" onClick={() => history.setEnabled(true)}>
            Turn on
          </button>
        </div>
      </div>
    );
  }

  const list = result.days;
  if (!list) {
    return <p className="empty">{result.error ?? "Loading…"}</p>;
  }
  if (list.length === 0) {
    return <p className="empty history-empty">No history yet — Portlight starts recording today.</p>;
  }

  const busiest = Math.max(1, ...list.map((day) => day.connections));
  const total = list.reduce((sum, day) => sum + day.connections, 0);
  const data = list.reduce((sum, day) => sum + day.bytesSent + day.bytesReceived, 0);
  const canSeeMore = days < 30 && settings.keepDays > days;

  return (
    <div className="history step-in">
      <p className="history-total">
        <span>Last {days} days</span>
        <span>
          {plural(total, "connection")}
          {data > 0 && ` · ${bytes(data)}`}
        </span>
      </p>
      <ul className="history-days">
        {list.map((day) => (
          <DayRow
            key={day.day}
            day={day}
            share={day.connections / busiest}
            open={open === day.day}
            onToggle={() => setOpen((current) => (current === day.day ? null : day.day))}
          />
        ))}
      </ul>
      {canSeeMore && (
        <button className="button button-quiet history-more" disabled={result.loading} onClick={() => setDays(30)}>
          {result.loading ? "Loading…" : "See 30 days"}
        </button>
      )}
    </div>
  );
}

function DayRow({ day, share, open, onToggle }: { day: HistoryDay; share: number; open: boolean; onToggle(): void }) {
  const [all, setAll] = useState(false);
  const data = day.bytesSent + day.bytesReceived;
  const servers = day.servers.length >= 200 ? "200+ servers" : plural(day.servers.length, "server");
  const shown = all ? day.servers : day.servers.slice(0, FIRST_SERVERS);

  return (
    <li className={`history-day ${open ? "is-open" : ""}`}>
      <button className="history-day-row" aria-expanded={open} onClick={onToggle}>
        <span className="history-day-text">
          <strong>{dayLabel(day.day)}</strong>
          <span>
            {plural(day.connections, "connection")}
            {data > 0 && ` · ${bytes(data)}`} · {servers}
          </span>
          <span className="history-bar" aria-hidden="true">
            <i style={{ width: `${Math.max(4, share * 100)}%` }} />
          </span>
        </span>
        <ChevronRightIcon />
      </button>
      {open && (
        <div className="history-servers step-in">
          <ul>
            {shown.map((server) => {
              const where = ownerLine(server, server.ip);
              const serverData = server.bytesSent + server.bytesReceived;
              return (
                <li key={`${server.ip}|${server.port}|${server.protocol}`}>
                  <span className="history-server-line">
                    <span className="history-server-name" title={server.name === server.ip ? server.ip : `${server.name} (${server.ip})`}>
                      {server.name}
                    </span>
                    {server.protocol === "udp" && <span className="tag tag-udp">UDP</span>}
                    <span className="history-server-side">{plural(server.connections, "connection")}</span>
                  </span>
                  <span className="history-server-line is-sub">
                    <span className="history-server-owner" title={ownerTitle(server, server.ip) ?? undefined}>
                      {where ? shorten(where, 34) : "Network owner unknown"}
                    </span>
                    <span className="history-server-side">
                      {hourMinute(server.firstSeen)}–{hourMinute(server.lastSeen)}
                      {serverData > 0 && ` · ${bytes(serverData)}`}
                    </span>
                  </span>
                </li>
              );
            })}
          </ul>
          {day.servers.length > FIRST_SERVERS && (
            <button className="link history-all" onClick={() => setAll((v) => !v)}>
              {all ? "Show fewer" : `Show all ${servers}`}
            </button>
          )}
        </div>
      )}
    </li>
  );
}
