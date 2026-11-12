// Small on/off preferences that only the window cares about, kept in localStorage and shared live.

import { useCallback, useSyncExternalStore } from "react";

const listeners = new Set<() => void>();

function subscribe(listener: () => void) {
  listeners.add(listener);
  return () => void listeners.delete(listener);
}

/** Pop-ups inside the window for new apps. */
export const TOASTS_PREF = "portlight.toasts";

export function usePref(key: string, fallback: boolean): [boolean, (value: boolean) => void] {
  const value = useSyncExternalStore(subscribe, () => {
    const saved = localStorage.getItem(key);
    return saved === null ? fallback : saved === "1";
  });
  const set = useCallback(
    (next: boolean) => {
      localStorage.setItem(key, next ? "1" : "0");
      listeners.forEach((listener) => listener());
    },
    [key],
  );
  return [value, set];
}
