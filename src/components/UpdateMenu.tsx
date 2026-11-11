// The version button: shows this version, offers a newer one, and holds the
// "install updates automatically" and "detailed monitoring" switches. It sits at the bottom of the
// sidebar, so its menu opens upwards.

import { useCallback, useRef, useState } from "react";

import type { Collector } from "../hooks/useCollector";
import { useDismiss } from "../hooks/useDismiss";
import type { Updates } from "../hooks/useUpdates";
import { CopyButton } from "./CopyButton";

export function UpdateMenu({ updates, collector }: { updates: Updates; collector: Collector }) {
  const [open, setOpen] = useState(false);
  const root = useRef<HTMLDivElement>(null);
  useDismiss(root, open, useCallback(() => setOpen(false), []));

  const { status, busy, error, upToDate } = updates;
  if (!status) return null;
  const available = status.availableVersion;

  return (
    <div className="popover" ref={root}>
      <button
        className={`button ${available ? "button-solid" : "button-quiet"}`}
        aria-expanded={open}
        onClick={() => setOpen((value) => !value)}
      >
        {available ? `Update to ${available}` : `Version ${status.current}`}
      </button>

      {open && (
        <div className="menu is-up" role="dialog" aria-label="Version and updates">
          <div className="menu-head">
            <h2 className="section-title">Portlight {status.current}</h2>
          </div>

          {available && (
            <div className="callout">
              <p>Version {available} is ready.</p>
              {status.canInstall ? (
                <button className="button button-solid" disabled={busy !== null} onClick={updates.install}>
                  {busy === "installing" ? "Downloading…" : `Update to ${available}`}
                </button>
              ) : (
                <p className="menu-url">{status.releasePage}</p>
              )}
            </div>
          )}

          <label className="check">
            <input
              type="checkbox"
              checked={status.autoUpdate}
              onChange={(event) => updates.setAutoUpdate(event.target.checked)}
            />
            <span>
              <strong>Install updates automatically</strong>
              Portlight checks GitHub twice a day and updates itself quietly.
            </span>
          </label>

          <CollectorSwitch collector={collector} />

          <div className="menu-foot">
            <button className="button" disabled={busy !== null} onClick={updates.check}>
              {busy === "checking" ? "Checking…" : "Check for updates"}
            </button>
            <span className="menu-note" role="status">
              {error ?? (upToDate ? "You have the latest version." : "")}
            </span>
          </div>

          {!status.canInstall && (
            <p className="menu-text">
              This copy was not set up by the installer, so it will not update itself.
            </p>
          )}
        </div>
      )}
    </div>
  );
}

/** Turns the Portlight Collector service on or off. Each change shows one Windows administrator prompt. */
function CollectorSwitch({ collector }: { collector: Collector }) {
  const { status, busy, error } = collector;
  if (!status) return null;
  const on = status.installed;

  return (
    <div className="check-block">
      <label className="check">
        <input
          type="checkbox"
          checked={busy === "installing" || (on && busy !== "removing")}
          disabled={busy !== null || (!on && !status.canInstall)}
          onChange={(event) => (event.target.checked ? collector.install() : collector.uninstall())}
        />
        <span>
          <strong>Detailed monitoring</strong>
          {busy === "installing"
            ? "Waiting for Windows to allow it…"
            : busy === "removing"
              ? "Turning off…"
              : on
                ? `On${status.version ? ` (background service ${status.version})` : ""}. Sees UDP, very short connections and data used. Turning it off asks Windows for permission once.`
                : status.canInstall
                  ? "Off. Turning it on installs a small background service and asks Windows for permission once."
                  : "Not available in this copy: the background service is missing."}
        </span>
      </label>
      {on && status.outdated && busy === null && (
        <button className="button" onClick={collector.install}>Update the background service</button>
      )}
      {error && (
        <p className="menu-error has-copy" role="alert">
          <span>{error}</span>
          <CopyButton value={error} what="message" />
        </p>
      )}
    </div>
  );
}
