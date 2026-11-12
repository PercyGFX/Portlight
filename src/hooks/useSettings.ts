// The app's settings and About details, for the Settings page.

import { useCallback, useEffect, useState } from "react";

import { backend } from "../lib/backend";
import type { About, AppSettings, SettingsChange } from "../lib/types";

export interface Settings {
  /** Null until known. */
  settings: AppSettings | null;
  about: About | null;
  /** The last thing that went wrong, in words. */
  error: string | null;
  change(change: SettingsChange): void;
}

export function useSettings(): Settings {
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [about, setAbout] = useState<About | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let active = true;
    backend.getSettings().then((next) => active && setSettings(next)).catch((reason) => active && setError(String(reason)));
    backend.about().then((next) => active && setAbout(next)).catch(console.error);
    return () => {
      active = false;
    };
  }, []);

  const change = useCallback((next: SettingsChange) => {
    setError(null);
    // Show the switch moving at once; the answer confirms it, or puts it back.
    let before: AppSettings | null = null;
    setSettings((current) => {
      before = current;
      return current ? { ...current, ...next } : current;
    });
    backend
      .setSettings(next)
      .then(setSettings)
      .catch((reason) => {
        setError(String(reason));
        setSettings(before);
      });
  }, []);

  return { settings, about, error, change };
}
