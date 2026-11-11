import { useCallback, useEffect, useState } from "react";

import { backend } from "../lib/backend";
import type { AppView, ConnView, FlowEvent, Snapshot } from "../lib/types";

const MAX_EVENTS = 300;

export interface Monitor {
  /** False until the first snapshot arrives. */
  ready: boolean;
  now: number;
  learningSecondsLeft: number;
  apps: AppView[];
  connections: ConnView[];
  /** Newest first. */
  events: FlowEvent[];
  /** Every name learned so far, by remote address. */
  hosts: ReadonlyMap<string, string>;
  trustApp(id: string): void;
  trustAll(): void;
}

interface State {
  ready: boolean;
  now: number;
  learningSecondsLeft: number;
  apps: AppView[];
  connections: ConnView[];
  events: FlowEvent[];
  hosts: Map<string, string>;
}

const EMPTY: State = {
  ready: false,
  now: Date.now(),
  learningSecondsLeft: 0,
  apps: [],
  connections: [],
  events: [],
  hosts: new Map(),
};

/** Folds one snapshot into what we already know. */
function merge(prev: State, snapshot: Snapshot): State {
  // The first full snapshot and the first ticks can arrive in either order,
  // so add events by sequence number and never twice.
  const seen = new Set(prev.events.map((e) => e.seq));
  const fresh = snapshot.events.filter((e) => !seen.has(e.seq));
  const events = fresh.length
    ? [...prev.events, ...fresh].sort((a, b) => b.seq - a.seq).slice(0, MAX_EVENTS)
    : prev.events;

  let hosts = prev.hosts;
  for (const conn of snapshot.connections) {
    if (conn.host && hosts.get(conn.remoteIp) !== conn.host) {
      if (hosts === prev.hosts) hosts = new Map(prev.hosts);
      hosts.set(conn.remoteIp, conn.host);
    }
  }

  return {
    ready: true,
    now: snapshot.now,
    learningSecondsLeft: snapshot.learningSecondsLeft,
    apps: snapshot.apps,
    connections: snapshot.connections,
    events,
    hosts,
  };
}

/** Live picture of which apps are online, kept up to date by the backend. */
export function useMonitor(): Monitor {
  const [state, setState] = useState<State>(EMPTY);

  useEffect(() => {
    let active = true;
    const apply = (snapshot: Snapshot) => {
      if (active) setState((prev) => merge(prev, snapshot));
    };

    backend.getSnapshot().then(apply).catch(console.error);
    const stop = backend.onTick(apply);

    return () => {
      active = false;
      stop.then((unlisten) => unlisten()).catch(console.error);
    };
  }, []);

  const trustApp = useCallback((id: string) => {
    // Update the screen right away; the next tick confirms it.
    setState((prev) => ({ ...prev, apps: prev.apps.map((a) => (a.id === id ? { ...a, trusted: true } : a)) }));
    backend.trustApp(id).catch(console.error);
  }, []);

  const trustAll = useCallback(() => {
    setState((prev) => ({ ...prev, apps: prev.apps.map((a) => ({ ...a, trusted: true })) }));
    backend.trustAll().catch(console.error);
  }, []);

  return { ...state, trustApp, trustAll };
}
