// Save: one button for the common case (every packet, in a file Wireshark opens),
// and a small "…" menu beside it: which packets, then which kind of file.

import { useCallback, useRef, useState } from "react";

import { useDismiss } from "../../hooks/useDismiss";
import { count } from "../../lib/format";
import type { ExportFormat, PacketSelection } from "../../lib/types";
import { SaveIcon } from "../icons";

interface Props {
  packets: number;
  shown: number;
  filtered: boolean;
  /** The chosen packet's number, if any. */
  selected: number | null;
  busy: boolean;
  onSave(format: ExportFormat, selection: PacketSelection): void;
}

type Which = "all" | "shown" | "packet" | "range";

const FORMATS: Array<[ExportFormat, string, string]> = [
  ["pcapng", "Save for Wireshark", "The packets themselves (.pcapng). Wireshark and most network tools open it."],
  ["csv", "Export list as CSV", "The rows of the list, for a spreadsheet."],
  ["text", "Export breakdown as text", "Every layer in plain words. Good for reports, or pasting into an AI chat."],
  ["json", "Export breakdown as JSON", "Every layer and field, for programs."],
];

export function SaveMenu({ packets, shown, filtered, selected, busy, onSave }: Props) {
  const [open, setOpen] = useState(false);
  const [which, setWhich] = useState<Which>("all");
  const [range, setRange] = useState({ from: 1, to: 1 });
  const root = useRef<HTMLDivElement>(null);
  const close = useCallback(() => setOpen(false), []);
  useDismiss(root, open, close);
  const empty = packets === 0;

  const toggle = () => {
    if (!open) {
      setRange({ from: 1, to: packets });
      // A choice that no longer applies falls back to all packets.
      if ((which === "shown" && !filtered) || (which === "packet" && selected === null)) setWhich("all");
    }
    setOpen((v) => !v);
  };

  const selection = (): PacketSelection => {
    switch (which) {
      case "shown":
        return { kind: "shown" };
      case "packet":
        return selected !== null ? { kind: "packets", numbers: [selected] } : { kind: "all" };
      case "range":
        return { kind: "range", from: range.from, to: range.to };
      default:
        return { kind: "all" };
    }
  };

  const choices: Array<[Which, string, boolean]> = [
    ["all", `All ${count(packets)}`, true],
    ["shown", filtered ? `Shown ${count(shown)}` : "Shown", filtered],
    ["packet", selected !== null ? `Packet ${selected}` : "This packet", selected !== null],
    ["range", "A range", true],
  ];

  const rangeValid = range.from >= 1 && range.to >= 1 && range.from <= packets && range.to <= packets;

  return (
    <div className="popover button-group" ref={root}>
      <button
        className="button"
        disabled={empty || busy}
        onClick={() => onSave("pcapng", { kind: "all" })}
        title={`Save all ${count(packets)} packets in a file Wireshark opens (.pcapng)`}
      >
        <SaveIcon /> {busy ? "Saving…" : "Save"}
      </button>
      <button
        className="button button-icon"
        aria-label="Other ways to save"
        title="Other ways to save"
        aria-expanded={open}
        disabled={empty || busy}
        onClick={toggle}
      >
        …
      </button>
      {open && (
        <div className="menu save-menu" role="dialog" aria-label="Other ways to save">
          <h2 className="section-title">Which packets</h2>
          <div className="segmented save-which" role="group" aria-label="Which packets">
            {choices.map(([id, label, enabled]) => (
              <button
                key={id}
                className={which === id ? "is-active" : ""}
                aria-pressed={which === id}
                disabled={!enabled}
                title={!enabled ? (id === "shown" ? "Search first to pick some packets" : "Pick a packet in the list first") : undefined}
                onClick={() => setWhich(id)}
              >
                {label}
              </button>
            ))}
          </div>
          {which === "range" && (
            <div className="save-range step-in">
              <label>
                From packet
                <input
                  type="number"
                  min={1}
                  max={packets}
                  value={range.from}
                  onChange={(event) => setRange((r) => ({ ...r, from: Number(event.target.value) }))}
                />
              </label>
              <label>
                to
                <input
                  type="number"
                  min={1}
                  max={packets}
                  value={range.to}
                  onChange={(event) => setRange((r) => ({ ...r, to: Number(event.target.value) }))}
                />
              </label>
            </div>
          )}

          <h2 className="section-title">As</h2>
          <ul className="picker-list">
            {FORMATS.map(([format, title, sub]) => (
              <li key={format}>
                <button
                  className="picker-option"
                  disabled={which === "range" && !rangeValid}
                  onClick={() => {
                    setOpen(false);
                    onSave(format, selection());
                  }}
                >
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
    </div>
  );
}
