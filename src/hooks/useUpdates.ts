import { useCallback, useEffect, useState } from "react";

import { backend } from "../lib/backend";
import type { UpdateStatus } from "../lib/types";

export interface Updates {
  status: UpdateStatus | null;
  /** "checking", "installing", or null when idle. */
  busy: "checking" | "installing" | null;
  /** The last thing that went wrong, in words. */
  error: string | null;
  /** True after a manual check found nothing newer. */
  upToDate: boolean;
  check(): void;
  install(): void;
  setAutoUpdate(enabled: boolean): void;
}

/** Version, update setting and the "Update to x.y.z" flow. */
export function useUpdates(): Updates {
  const [status, setStatus] = useState<UpdateStatus | null>(null);
  const [busy, setBusy] = useState<Updates["busy"]>(null);
  const [error, setError] = useState<string | null>(null);
  const [upToDate, setUpToDate] = useState(false);

  useEffect(() => {
    let active = true;
    const apply = (next: UpdateStatus) => {
      if (active) setStatus(next);
    };
    backend.updateStatus().then(apply).catch(console.error);
    const stop = backend.onUpdateStatus(apply);
    return () => {
      active = false;
      stop.then((unlisten) => unlisten()).catch(console.error);
    };
  }, []);

  const run = useCallback((kind: NonNullable<Updates["busy"]>, work: () => Promise<UpdateStatus | void>) => {
    setBusy(kind);
    setError(null);
    setUpToDate(false);
    work()
      .then((next) => {
        if (!next) return;
        setStatus(next);
        if (kind === "checking") setUpToDate(next.availableVersion === null);
      })
      .catch((reason) => setError(String(reason)))
      .finally(() => setBusy(null));
  }, []);

  const check = useCallback(() => run("checking", () => backend.checkForUpdate()), [run]);
  const install = useCallback(() => run("installing", () => backend.installUpdate()), [run]);
  const setAutoUpdate = useCallback((enabled: boolean) => {
    backend.setAutoUpdate(enabled).then(setStatus).catch((reason) => setError(String(reason)));
  }, []);

  return { status, busy, error, upToDate, check, install, setAutoUpdate };
}
