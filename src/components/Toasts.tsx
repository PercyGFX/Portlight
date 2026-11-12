// Pop-ups inside the window for apps that just went online for the first time.
// (Windows also shows a notification, which works while the window is hidden.)

import { useEffect, useRef, useState } from "react";

import { TOASTS_PREF, usePref } from "../hooks/usePref";
import { signatureLine } from "../lib/owners";
import type { AppView } from "../lib/types";
import { AppIcon } from "./AppIcon";
import { CloseIcon } from "./icons";

const SHOW_FOR_MS = 8000;

interface Props {
  ready: boolean;
  newApps: AppView[];
  onSelect(id: string): void;
}

export function Toasts({ ready, newApps, onSelect }: Props) {
  const [toasts, setToasts] = useState<AppView[]>([]);
  // Apps already announced, so each one pops up once. `null` until the first
  // snapshot: apps that were new before the window opened only show in the bell.
  const announced = useRef<Set<string> | null>(null);
  const [enabled] = usePref(TOASTS_PREF, true);

  useEffect(() => {
    if (!ready) return;
    if (announced.current === null) {
      announced.current = new Set(newApps.map((app) => app.id));
      return;
    }
    const seen = announced.current;
    const fresh = newApps.filter((app) => !seen.has(app.id));
    if (!fresh.length) return;
    fresh.forEach((app) => seen.add(app.id));
    // Turned off in Settings: still marked as announced, so turning it on later does not replay them.
    if (!enabled) return;
    setToasts((current) => [...current, ...fresh].slice(-3));

    const freshIds = new Set(fresh.map((app) => app.id));
    window.setTimeout(() => {
      setToasts((current) => current.filter((t) => !freshIds.has(t.id)));
    }, SHOW_FOR_MS);
  }, [ready, newApps, enabled]);

  const dismiss = (id: string) => setToasts((current) => current.filter((t) => t.id !== id));

  return (
    <div className="toasts" aria-live="polite">
      {toasts.map((announcedApp) => {
        // The signature is checked a moment after the app appears, so read the latest.
        const app = newApps.find((a) => a.id === announcedApp.id) ?? announcedApp;
        const maker =
          signatureLine(app.signature) ?? (app.path ? "Checking who made it…" : "Part of Windows or hidden by it");
        return (
          <div key={app.id} className="toast">
            <button
              className="toast-body"
              onClick={() => {
                onSelect(app.id);
                dismiss(app.id);
              }}
            >
              <span className="toast-title">New app online</span>
              <span className="toast-app">
                <AppIcon appId={app.id} name={app.name} size={28} />
                <strong>{app.name}</strong>
              </span>
              <span className={app.signature.state === "unsigned" ? "toast-unsigned" : undefined}>
                {maker.endsWith("…") ? maker : `${maker}.`} Click to look.
              </span>
            </button>
            <button className="button button-icon button-quiet" aria-label="Dismiss" onClick={() => dismiss(app.id)}>
              <CloseIcon />
            </button>
          </div>
        );
      })}
    </div>
  );
}
