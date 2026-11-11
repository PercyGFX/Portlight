// Packet capture state for the whole window: Npcap support, the capture's status,
// and the commands. It lives above the pages so the live view can show "Recording".

import { useCallback, useEffect, useState } from "react";

import { backend } from "../lib/backend";
import type { CaptureStartOptions, CaptureStatus, CaptureSupport, ExportFormat, PacketSelection } from "../lib/types";

export interface SavedFile {
  path: string;
  format: ExportFormat;
}

export interface Capture {
  support: CaptureSupport | null;
  checking: boolean;
  checkSupport(): void;
  status: CaptureStatus;
  /** A command is on its way. */
  busy: "starting" | "stopping" | "saving" | null;
  /** The last command that failed, in words. */
  error: string | null;
  /** Packets that would be lost by starting again or clearing. */
  unsaved: boolean;
  /** The last file written, shown briefly. */
  saved: SavedFile | null;
  start(options: CaptureStartOptions): void;
  stop(): void;
  clear(): void;
  setFilter(filter: string): void;
  /** Asks where to save, then writes the chosen packets. */
  save(format: ExportFormat, selection: PacketSelection): void;
  dismissError(): void;
}

const IDLE: CaptureStatus = {
  state: "idle",
  target: null,
  startedAt: null,
  stoppedAt: null,
  packets: 0,
  shown: 0,
  bytes: 0,
  dropped: 0,
  filter: "",
  filterError: null,
  limitReached: false,
  error: null,
  stopAfter: { packets: null, seconds: null, megabytes: null },
  stopReason: null,
};

const SAVED_NOTE_MS = 9000;

export function useCapture(): Capture {
  const [support, setSupport] = useState<CaptureSupport | null>(null);
  const [checking, setChecking] = useState(false);
  const [status, setStatus] = useState<CaptureStatus>(IDLE);
  const [busy, setBusy] = useState<Capture["busy"]>(null);
  const [error, setError] = useState<string | null>(null);
  const [saved, setSaved] = useState<SavedFile | null>(null);
  // The capture (by start time) and packet count last saved in full.
  const [savedFor, setSavedFor] = useState<string | null>(null);

  const checkSupport = useCallback(() => {
    setChecking(true);
    backend
      .captureSupport()
      .then(setSupport)
      .catch((reason) => setError(String(reason)))
      .finally(() => setChecking(false));
  }, []);

  useEffect(() => {
    let active = true;
    const apply = (next: CaptureStatus) => {
      if (active) setStatus(next);
    };
    checkSupport();
    backend.captureStatus().then(apply).catch(console.error);
    const stop = backend.onCaptureStatus(apply);
    return () => {
      active = false;
      stop.then((unlisten) => unlisten()).catch(console.error);
    };
  }, [checkSupport]);

  useEffect(() => {
    if (!saved) return;
    const timer = window.setTimeout(() => setSaved(null), SAVED_NOTE_MS);
    return () => window.clearTimeout(timer);
  }, [saved]);

  const run = useCallback((kind: NonNullable<Capture["busy"]>, work: () => Promise<CaptureStatus>) => {
    setBusy(kind);
    setError(null);
    work()
      .then(setStatus)
      .catch((reason) => setError(String(reason)))
      .finally(() => setBusy(null));
  }, []);

  const start = useCallback(
    (options: CaptureStartOptions) => {
      setSaved(null);
      run("starting", () => backend.captureStart(options));
    },
    [run],
  );
  const stop = useCallback(() => run("stopping", () => backend.captureStop()), [run]);
  const clear = useCallback(() => run("stopping", () => backend.captureClear()), [run]);

  const setFilter = useCallback((filter: string) => {
    backend
      .captureSetFilter(filter)
      .then(setStatus)
      .catch((reason) => setError(String(reason)));
  }, []);

  const fingerprint = `${status.startedAt}:${status.packets}`;
  const save = useCallback(
    (format: ExportFormat, selection: PacketSelection) => {
      setBusy("saving");
      setError(null);
      const savedAs = fingerprint;
      backend
        .captureExport(format, selection)
        .then((path) => {
          if (!path) return;
          setSaved({ path, format });
          // Only a full Wireshark file counts as "saved" when starting again.
          if (format === "pcapng" && selection.kind === "all") setSavedFor(savedAs);
        })
        .catch((reason) => setError(String(reason)))
        .finally(() => setBusy(null));
    },
    [fingerprint],
  );

  const unsaved = status.packets > 0 && savedFor !== fingerprint;
  const dismissError = useCallback(() => setError(null), []);

  return { support, checking, checkSupport, status, busy, error, unsaved, saved, start, stop, clear, setFilter, save, dismissError };
}
