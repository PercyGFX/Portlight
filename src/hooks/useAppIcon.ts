// App icons, fetched once per app and shared by every place that shows one.

import { useEffect, useReducer } from "react";

import { backend } from "../lib/backend";

const cache = new Map<string, string | null>();
const pending = new Map<string, Promise<string | null>>();

function load(appId: string): Promise<string | null> {
  let request = pending.get(appId);
  if (!request) {
    request = backend
      .appIcon(appId)
      .catch(() => null)
      .then((icon) => {
        cache.set(appId, icon);
        pending.delete(appId);
        return icon;
      });
    pending.set(appId, request);
  }
  return request;
}

/** The app's icon as a data URL; null while loading or when it has none. */
export function useAppIcon(appId: string | null | undefined): string | null {
  const [, refresh] = useReducer((n: number) => n + 1, 0);

  useEffect(() => {
    if (!appId || cache.has(appId)) return;
    let active = true;
    load(appId).then(() => active && refresh());
    return () => {
      active = false;
    };
  }, [appId]);

  return appId ? (cache.get(appId) ?? null) : null;
}
