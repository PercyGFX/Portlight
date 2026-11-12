// Left rail: the name, search, the app list (passed in), the way into packet
// capture, and at the bottom the version, Settings and the theme switch.

import type { ReactNode, RefObject } from "react";

import type { Theme } from "../hooks/useTheme";
import type { Updates } from "../hooks/useUpdates";
import { count } from "../lib/format";
import type { CaptureStatus } from "../lib/types";
import { GearIcon, Logo, MoonIcon, RecordIcon, SearchIcon, SunIcon } from "./icons";

interface Props {
  query: string;
  searchRef: RefObject<HTMLInputElement | null>;
  theme: Theme;
  updates: Updates;
  capture: CaptureStatus;
  onOpenCapture(): void;
  /** Opens Settings: at Updates when one is waiting, else at About. */
  onOpenSettings(section?: "updates" | "about"): void;
  onQuery(query: string): void;
  onTheme(theme: Theme): void;
  children: ReactNode;
}

export function Sidebar({ query, searchRef, theme, updates, capture, onOpenCapture, onOpenSettings, onQuery, onTheme, children }: Props) {
  const next: Theme = theme === "dark" ? "light" : "dark";
  const status = updates.status;
  const available = status?.availableVersion ?? null;

  return (
    <aside className="sidebar">
      <div className="brand">
        <Logo />
        <div>
          <h1>Portlight</h1>
          <p>See which apps are online</p>
        </div>
      </div>

      <label className="search">
        <SearchIcon />
        <input
          ref={searchRef}
          type="search"
          placeholder="Search apps and servers"
          value={query}
          onChange={(e) => onQuery(e.target.value)}
          aria-label="Search apps and servers"
        />
        <kbd>Ctrl K</kbd>
      </label>

      {children}

      {capture.state === "running" ? (
        <button className="pill pill-recording capture-entry" onClick={onOpenCapture} title="Back to the packet capture">
          <span className="live-dot" />
          Recording · {count(capture.packets)} {capture.packets === 1 ? "packet" : "packets"}
        </button>
      ) : (
        <button className="button capture-entry" onClick={onOpenCapture} title="Record the packets on this PC's network, like Wireshark">
          <RecordIcon /> Capture packets
        </button>
      )}

      <footer className="sidebar-foot">
        {status && (
          <button
            className={`button ${available ? "button-solid" : "button-quiet"} version-button`}
            title={available ? "See the update" : "About Portlight and what's new"}
            onClick={() => onOpenSettings(available ? "updates" : "about")}
          >
            {available ? `Update to ${available}` : `Version ${status.current}`}
          </button>
        )}
        <span className="sidebar-foot-tools">
          <button className="button button-icon" aria-label="Settings" title="Settings" onClick={() => onOpenSettings()}>
            <GearIcon />
          </button>
          <button className="button button-icon" aria-label={`Switch to ${next} mode`} title={`Switch to ${next} mode`} onClick={() => onTheme(next)}>
            {theme === "dark" ? <SunIcon /> : <MoonIcon />}
          </button>
        </span>
      </footer>
    </aside>
  );
}
