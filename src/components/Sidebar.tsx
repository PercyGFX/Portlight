// Left rail: the name, search, the app list (passed in), and at the bottom
// the version menu and the theme switch.

import type { ReactNode, RefObject } from "react";

import type { Theme } from "../hooks/useTheme";
import type { Updates } from "../hooks/useUpdates";
import { Logo, MoonIcon, SearchIcon, SunIcon } from "./icons";
import { UpdateMenu } from "./UpdateMenu";

interface Props {
  query: string;
  searchRef: RefObject<HTMLInputElement | null>;
  theme: Theme;
  updates: Updates;
  onQuery(query: string): void;
  onTheme(theme: Theme): void;
  children: ReactNode;
}

export function Sidebar({ query, searchRef, theme, updates, onQuery, onTheme, children }: Props) {
  const next: Theme = theme === "dark" ? "light" : "dark";

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

      <footer className="sidebar-foot">
        <UpdateMenu updates={updates} />
        <button className="button button-icon" aria-label={`Switch to ${next} mode`} title={`Switch to ${next} mode`} onClick={() => onTheme(next)}>
          {theme === "dark" ? <SunIcon /> : <MoonIcon />}
        </button>
      </footer>
    </aside>
  );
}
