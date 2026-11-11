// The "…" on the connection flow panel: save what the live view shows as a spreadsheet file.

import { useCallback, useEffect, useRef, useState } from "react";

import { useDismiss } from "../hooks/useDismiss";
import { backend } from "../lib/backend";
import type { LiveExport } from "../lib/types";
import { CopyButton } from "./CopyButton";
import { CloseIcon } from "./icons";

const CHOICES: Array<[LiveExport, string, string]> = [
  ["apps", "Apps (CSV)", "Every app seen, with connections and data used"],
  ["connections", "Open connections (CSV)", "Who is connected to what, right now"],
  ["flow", "Connection history (CSV)", "Connections opening and closing: the last 2,000"],
];

const NOTE_MS = 9000;

export function ExportMenu() {
  const [open, setOpen] = useState(false);
  const [busy, setBusy] = useState(false);
  const [note, setNote] = useState<{ path: string } | { error: string } | null>(null);
  const root = useRef<HTMLDivElement>(null);
  const close = useCallback(() => setOpen(false), []);
  useDismiss(root, open, close);

  useEffect(() => {
    if (!note) return;
    const timer = window.setTimeout(() => setNote(null), NOTE_MS);
    return () => window.clearTimeout(timer);
  }, [note]);

  const run = (what: LiveExport) => {
    setOpen(false);
    setBusy(true);
    backend
      .exportLive(what)
      .then((path) => path && setNote({ path }))
      .catch((reason) => setNote({ error: String(reason) }))
      .finally(() => setBusy(false));
  };

  return (
    <div className="popover" ref={root}>
      <button
        className="button button-icon"
        aria-label="Export"
        title="Export what you see as a spreadsheet"
        aria-expanded={open}
        disabled={busy}
        onClick={() => setOpen((v) => !v)}
      >
        …
      </button>
      {open && (
        <div className="menu export-menu" role="dialog" aria-label="Export">
          <h2 className="section-title">Export</h2>
          <ul className="picker-list">
            {CHOICES.map(([what, title, sub]) => (
              <li key={what}>
                <button className="picker-option" onClick={() => run(what)}>
                  <span className="picker-option-text">
                    <strong>{title}</strong>
                    <span>{sub}</span>
                  </span>
                </button>
              </li>
            ))}
          </ul>
        </div>
      )}
      {note && (
        <div className={`export-note ${"error" in note ? "is-error" : ""}`} role="status">
          {"error" in note ? (
            <span>{note.error}</span>
          ) : (
            <>
              <span>
                Saved: <span className="mono">{note.path}</span>
              </span>
              <CopyButton value={note.path} what="file path" />
            </>
          )}
          <button className="button button-icon button-quiet" aria-label="Dismiss" onClick={() => setNote(null)}>
            <CloseIcon />
          </button>
        </div>
      )}
    </div>
  );
}
