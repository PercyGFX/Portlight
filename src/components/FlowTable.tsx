// Under the map: connections opening and closing, newest first, in plain words.

import { bytes, clock, shorten } from "../lib/format";
import { ownerLine, ownerTitle, type Owner } from "../lib/owners";
import { describe, serviceOf } from "../lib/services";
import type { FlowEvent } from "../lib/types";
import { AppIcon } from "./AppIcon";
import { CopyButton } from "./CopyButton";
import { ExportMenu } from "./ExportMenu";
import { ClearIcon, PauseIcon, PlayIcon } from "./icons";

interface Props {
  events: FlowEvent[];
  hosts: ReadonlyMap<string, string>;
  owners: ReadonlyMap<string, Owner>;
  /** Name of the app the table is narrowed to, if any. */
  scope: string | null;
  paused: boolean;
  onTogglePause(): void;
  onClear(): void;
  onSelectApp(id: string): void;
}

export function FlowTable({ events, hosts, owners, scope, paused, onTogglePause, onClear, onSelectApp }: Props) {
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
          <ExportMenu />
        </div>
      </header>

      <div className="flow-scroll">
        <table className="flow-table">
          <thead>
            <tr>
              <th className="col-time">Time</th>
              <th className="col-app">App</th>
              <th>What happened</th>
              <th className="col-dest">With</th>
              <th className="col-owner" title="Who runs the server's network, and where that network is registered">Network</th>
              <th className="col-port">Type</th>
            </tr>
          </thead>
          <tbody>
            {events.map((event) => {
              const destination = hosts.get(event.remoteIp) ?? event.host ?? event.remoteIp;
              const sentence = describe(event);
              const total = event.bytesSent + event.bytesReceived;
              const label = event.protocol === "udp" && event.remotePort === 443 ? "QUIC" : serviceOf(event.remotePort).label;
              const owner = owners.get(event.remoteIp);
              const where = ownerLine(owner, event.remoteIp);
              const about = ownerTitle(owner, event.remoteIp);
              return (
                <tr key={event.seq} className={`is-${event.kind}`}>
                  <td className="col-time">{clock(event.time)}</td>
                  <td className="col-app">
                    <button className="link flow-app" onClick={() => onSelectApp(event.appId)}>
                      <AppIcon appId={event.appId} name={event.appName} size={16} />
                      <span className="cell-ellipsis">{shorten(event.appName, 26)}</span>
                    </button>
                  </td>
                  <td className="col-what" title={`${sentence} · ${label}${event.protocol === "udp" ? " over UDP" : ""}`}>
                    <span className={`dot dot-${event.kind}`} /> {sentence}
                    {event.kind === "closed" && total > 0 && (
                      <span className="flow-bytes" title={`${bytes(event.bytesReceived)} down, ${bytes(event.bytesSent)} up`}>
                        {" "}· {bytes(total)}
                      </span>
                    )}
                  </td>
                  <td className="col-dest" title={`${destination} (${event.remoteIp}:${event.remotePort})${about ? `\n${about}` : ""}`}>
                    <span className="copyable has-copy">
                      <span className="cell-ellipsis">{destination}</span>
                      <CopyButton value={destination === event.remoteIp ? `${event.remoteIp}:${event.remotePort}` : destination} what="server" />
                    </span>
                  </td>
                  <td className="col-owner" title={about ?? undefined}>{where ?? ""}</td>
                  <td className="col-port" title={event.protocol === "udp" ? `${label}, over UDP` : label}>
                    <span className="port-cell">
                      {event.protocol === "udp" && <span className="tag tag-udp">UDP</span>}
                      <span className="cell-ellipsis">{label}</span>
                    </span>
                  </td>
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
