// Bottom panel: connections opening and closing, newest first, in plain words.

import { clock, shorten } from "../lib/format";
import { describe, serviceOf } from "../lib/services";
import type { FlowEvent } from "../lib/types";
import { ClearIcon, PauseIcon, PlayIcon } from "./icons";

interface Props {
  events: FlowEvent[];
  hosts: ReadonlyMap<string, string>;
  /** Name of the app the table is narrowed to, if any. */
  scope: string | null;
  paused: boolean;
  onTogglePause(): void;
  onClear(): void;
  onSelectApp(id: string): void;
}

export function FlowTable({ events, hosts, scope, paused, onTogglePause, onClear, onSelectApp }: Props) {
  return (
    <section className="panel flow-panel" aria-label="Connection flow">
      <header className="flow-head">
        <h2 className="panel-title">Connection flow</h2>
        <ul className="legend">
          <li><span className="dot dot-opened" /> Opened</li>
          <li><span className="dot dot-closed" /> Closed</li>
        </ul>
        {scope && <span className="scope">Only {shorten(scope, 24)}</span>}
        <div className="flow-controls">
          <button className={`button ${paused ? "button-solid" : ""}`} onClick={onTogglePause} title="Space">
            {paused ? <PlayIcon /> : <PauseIcon />} {paused ? "Resume" : "Pause"}
          </button>
          <button className="button" onClick={onClear} title="Clear the list">
            <ClearIcon /> Clear
          </button>
        </div>
      </header>

      <div className="flow-scroll">
        <table className="flow-table">
          <thead>
            <tr>
              <th className="col-time">Time</th>
              <th className="col-app">App</th>
              <th className="col-event">Event</th>
              <th className="col-dest">Destination</th>
              <th className="col-port">Type</th>
              <th>What happened</th>
            </tr>
          </thead>
          <tbody>
            {events.map((event) => {
              const destination = hosts.get(event.remoteIp) ?? event.host ?? event.remoteIp;
              return (
                <tr key={event.seq} className={`is-${event.kind}`}>
                  <td className="col-time">{clock(event.time)}</td>
                  <td className="col-app">
                    <button className="link" onClick={() => onSelectApp(event.appId)}>{shorten(event.appName, 28)}</button>
                  </td>
                  <td className="col-event">
                    <span className={`dot dot-${event.kind}`} /> {event.kind}
                  </td>
                  <td className="col-dest" title={`${destination} (${event.remoteIp}:${event.remotePort})`}>{shorten(destination, 34)}</td>
                  <td className="col-port">{serviceOf(event.remotePort).label}</td>
                  <td className="col-info">{describe(event, shorten(destination, 34))}</td>
                </tr>
              );
            })}
          </tbody>
        </table>
        {events.length === 0 && (
          <p className="empty flow-empty">
            {scope ? "Nothing new from this app yet." : "Waiting for the next connection to open or close."}
          </p>
        )}
      </div>
    </section>
  );
}
