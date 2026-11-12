// History on disk: the shared settings (menu and app details stay in step) and one app's days.

import { useCallback, useEffect, useState, useSyncExternalStore } from "react";

import { backend } from "../lib/backend";
import type { HistoryAppDay, HistoryDay, HistorySettings } from "../lib/types";

let current: HistorySettings | null = null;
let requested = false;
/** Bumped on every change, so day lists reload after a clear or a shorter keep. */
let version = 0;
const listeners = new Set<() => void>();

function publish(next: HistorySettings) {
  current = next;
  version += 1;
  listeners.forEach((listener) => listener());
}

function subscribe(listener: () => void) {
  listeners.add(listener);
  if (!requested) {
    requested = true;
    backend.historySettings().then(publish).catch(console.error);
  }
  return () => void listeners.delete(listener);
}

export interface History {
  /** Null until known. */
  settings: HistorySettings | null;
  /** Changes whenever the stored history may have changed. */
  version: number;
  error: string | null;
  setEnabled(enabled: boolean): void;
  setKeepDays(days: number): void;
  clear(): Promise<void>;
}

export function useHistorySettings(): History {
  const settings = useSyncExternalStore(subscribe, () => current);
  const seen = useSyncExternalStore(subscribe, () => version);
  const [error, setError] = useState<string | null>(null);

  const apply = useCallback((work: Promise<HistorySettings>) => {
    setError(null);
    return work.then(publish).catch((reason) => setError(String(reason)));
  }, []);

  const setEnabled = useCallback(
    (enabled: boolean) => void apply(backend.setHistorySettings(enabled, current?.keepDays ?? 90)),
    [apply],
  );
  const setKeepDays = useCallback(
    (days: number) => void apply(backend.setHistorySettings(current?.enabled ?? true, days)),
    [apply],
  );
  const clear = useCallback(() => apply(backend.clearHistory()), [apply]);

  return { settings, version: seen, error, setEnabled, setKeepDays, clear };
}

/** One app's days, newest first. `days` null means not wanted yet. */
export function useAppHistory(appId: string, days: number | null, version: number) {
  const [state, setState] = useState<{ key: string; days: HistoryDay[] | null; error: string | null }>({ key: "", days: null, error: null });
  const key = `${appId}|${days}|${version}`;

  useEffect(() => {
    if (days === null) return;
    let active = true;
    backend
      .appHistory(appId, days)
      .then((result) => active && setState({ key, days: result, error: null }))
      .catch((reason) => active && setState({ key, days: null, error: String(reason) }));
    return () => {
      active = false;
    };
  }, [appId, days, key]);

  // Keep showing the last list while a longer one loads, so the page does not jump.
  const fresh = state.key === key;
  const sameApp = state.key.startsWith(`${appId}|`);
  return { days: fresh || sameApp ? state.days : null, loading: !fresh, error: fresh ? state.error : null };
}

/** Totals per app over the last `days` days, busiest first. */
export function useHistorySummary(days: number, enabled: boolean, version: number) {
  const [rows, setRows] = useState<HistoryAppDay[] | null>(null);

  useEffect(() => {
    if (!enabled) {
      setRows(null);
      return;
    }
    let active = true;
    backend
      .historySummary(days)
      .then((result) => active && setRows(result))
      .catch(() => active && setRows(null));
    // Today's figures grow, so look again now and then.
    const timer = window.setInterval(() => {
      backend
        .historySummary(days)
        .then((result) => active && setRows(result))
        .catch(() => {});
    }, 60_000);
    return () => {
      active = false;
      window.clearInterval(timer);
    };
  }, [days, enabled, version]);

  return rows;
}
