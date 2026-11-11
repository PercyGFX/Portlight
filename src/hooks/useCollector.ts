// The Portlight Collector: is detailed monitoring on, and the turn on / turn off flow.

import { useCallback, useEffect, useState } from "react";

import { backend } from "../lib/backend";
import type { CollectorStatus } from "../lib/types";

export interface Collector {
  status: CollectorStatus | null;
  /** What is happening right now, or null when idle. */
  busy: "installing" | "removing" | null;
  /** The last thing that went wrong, in words. */
  error: string | null;
  install(): void;
  uninstall(): void;
}

export function useCollector(): Collector {
  const [status, setStatus] = useState<CollectorStatus | null>(null);
  const [busy, setBusy] = useState<Collector["busy"]>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let active = true;
    const apply = (next: CollectorStatus) => {
      if (active) setStatus(next);
    };
    backend.collectorStatus().then(apply).catch(console.error);
    const stop = backend.onCollectorStatus(apply);
    return () => {
      active = false;
      stop.then((unlisten) => unlisten()).catch(console.error);
    };
  }, []);

  const run = useCallback((kind: NonNullable<Collector["busy"]>, work: () => Promise<CollectorStatus>) => {
    setBusy(kind);
    setError(null);
    work()
      .then((next) => {
        setStatus(next);
        if (next.error) setError(next.error);
      })
      .catch((reason) => setError(String(reason)))
      .finally(() => setBusy(null));
  }, []);

  const install = useCallback(() => run("installing", () => backend.installCollector()), [run]);
  const uninstall = useCallback(() => run("removing", () => backend.uninstallCollector()), [run]);

  return { status, busy, error: error ?? status?.error ?? null, install, uninstall };
}
