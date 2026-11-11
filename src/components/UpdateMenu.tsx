// The version button: shows this version, offers a newer one, and holds the
// "install updates automatically" switch.

import { useCallback, useRef, useState } from "react";

import { useDismiss } from "../hooks/useDismiss";
import type { Updates } from "../hooks/useUpdates";

export function UpdateMenu({ updates }: { updates: Updates }) {
  const [open, setOpen] = useState(false);
  const root = useRef<HTMLDivElement>(null);
  useDismiss(root, open, useCallback(() => setOpen(false), []));

  const { status, busy, error, upToDate } = updates;
  if (!status) return null;
  const available = status.availableVersion;

  return (
    <div className="alerts" ref={root}>
      <button
        className={`button ${available ? "button-solid" : ""}`}
        aria-expanded={open}
        onClick={() => setOpen((value) => !value)}
      >
        {available ? `Update to ${available}` : `v${status.current}`}
      </button>

      {open && (
        <div className="menu" role="dialog" aria-label="Version and updates">
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
