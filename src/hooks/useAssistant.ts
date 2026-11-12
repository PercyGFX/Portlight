// AI assistant access for the whole window: the switch, connecting apps, what was shared, and consent prompts.

import { useCallback, useEffect, useState } from "react";

import { backend } from "../lib/backend";
import type { AssistantApp, AssistantConsent, AssistantLogEntry, AssistantStatus } from "../lib/types";

/** A consent prompt, with when it arrived so it can count down. */
export interface PendingConsent extends AssistantConsent {
  receivedAt: number;
}

export interface Assistant {
  /** Null until known. */
  status: AssistantStatus | null;
  /** Newest first. */
  log: AssistantLogEntry[];
  /** Which app is being added or removed right now. */
  busy: AssistantApp | null;
  /** Apps added in this session; Claude Code has no "configured" flag of its own. */
  added: Set<AssistantApp>;
  /** The last thing that went wrong, in words. */
  error: string | null;
  /** Consent prompts waiting for an answer, oldest first. */
  consents: PendingConsent[];
  setEnabled(enabled: boolean): void;
  connect(app: AssistantApp, connect: boolean): void;
  clearLog(): void;
  answer(id: number, allow: boolean): void;
  /** Drops a prompt that ran out of time; the assistant already counts it as "Don't allow". */
  expire(id: number): void;
}

const LOG_LIMIT = 300;

export function useAssistant(): Assistant {
  const [status, setStatus] = useState<AssistantStatus | null>(null);
  const [log, setLog] = useState<AssistantLogEntry[]>([]);
  const [busy, setBusy] = useState<AssistantApp | null>(null);
  const [added, setAdded] = useState<Set<AssistantApp>>(() => new Set());
  const [error, setError] = useState<string | null>(null);
  const [consents, setConsents] = useState<PendingConsent[]>([]);

  useEffect(() => {
    let active = true;
    backend.assistantStatus().then((next) => active && setStatus(next)).catch(console.error);
    backend
      .assistantLog()
      .then((entries) => {
        if (!active) return;
        // Keep anything that arrived while this was on its way.
        setLog((current) => merge(current, entries));
      })
      .catch(console.error);
    const stopActivity = backend.onAssistantActivity((entry) => {
      if (active) setLog((current) => merge([entry], current));
    });
    const stopConsent = backend.onAssistantConsent((consent) => {
      if (!active) return;
      setConsents((current) =>
        current.some((c) => c.id === consent.id) ? current : [...current, { ...consent, receivedAt: Date.now() }],
      );
    });
    return () => {
      active = false;
      stopActivity.then((unlisten) => unlisten()).catch(console.error);
      stopConsent.then((unlisten) => unlisten()).catch(console.error);
    };
  }, []);

  const setEnabled = useCallback((enabled: boolean) => {
    setError(null);
    // Move the switch at once; the answer confirms it, or puts it back.
    setStatus((current) => (current ? { ...current, enabled } : current));
    backend
      .setAssistantAccess(enabled)
      .then(setStatus)
      .catch((reason) => {
        setError(String(reason));
        setStatus((current) => (current ? { ...current, enabled: !enabled } : current));
      });
  }, []);

  const connect = useCallback((app: AssistantApp, on: boolean) => {
    setBusy(app);
    setError(null);
    backend
      .connectAssistantApp(app, on)
      .then((next) => {
        setStatus(next);
        setAdded((current) => {
          const copy = new Set(current);
          if (on) copy.add(app);
          else copy.delete(app);
          return copy;
        });
      })
      .catch((reason) => setError(String(reason)))
      .finally(() => setBusy(null));
  }, []);

  const clearLog = useCallback(() => {
    setLog([]);
    backend.clearAssistantLog().catch((reason) => setError(String(reason)));
  }, []);

  const drop = useCallback((id: number) => setConsents((current) => current.filter((c) => c.id !== id)), []);

  const answer = useCallback(
    (id: number, allow: boolean) => {
      drop(id);
      backend.assistantAnswer(id, allow).catch(console.error);
    },
    [drop],
  );

  return { status, log, busy, added, error, consents, setEnabled, connect, clearLog, answer, expire: drop };
}

/** Newest first, no repeats, at most LOG_LIMIT. */
function merge(newer: AssistantLogEntry[], older: AssistantLogEntry[]): AssistantLogEntry[] {
  const seen = new Set(newer.map((entry) => entry.id));
  return [...newer, ...older.filter((entry) => !seen.has(entry.id))].slice(0, LOG_LIMIT);
}
