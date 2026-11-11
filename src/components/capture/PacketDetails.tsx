// One packet, layer by layer: what each layer is for in plain words, then its
// fields. Hovering a field lights up its bytes in the hex view, and back.

import { CopyButton } from "../CopyButton";
import { ChevronRightIcon } from "../icons";
import type { PacketDetail, PacketField, PacketLayer } from "../../lib/types";

/** Bytes to light up in the hex view, and which layer or field they belong to. */
export interface ByteRange {
  key: string;
  start: number;
  length: number;
}

interface Props {
  detail: PacketDetail;
  collapsed: ReadonlySet<string>;
  /** The hovered or picked layer or field. */
  active: string | null;
  onToggle(layerId: string): void;
  onHover(range: ByteRange | null): void;
  onPick(range: ByteRange): void;
}

export function PacketDetails({ detail, collapsed, active, onToggle, onHover, onPick }: Props) {
  return (
    <div className="layers">
      {detail.layers.map((layer, index) => {
        const key = `${index}`;
        const open = !collapsed.has(layer.id);
        const range = { key, start: layer.start, length: layer.length };
        return (
          <section key={key} className={`layer layer-${layer.id} ${open ? "is-open" : ""} ${active === key ? "is-active" : ""}`}>
            <button
              className="layer-head"
              aria-expanded={open}
              onClick={() => onToggle(layer.id)}
              onMouseEnter={() => onHover(range)}
              onMouseLeave={() => onHover(null)}
              onFocus={() => onHover(range)}
              onBlur={() => onHover(null)}
            >
              <ChevronRightIcon />
              <span className="layer-title">{layer.title}</span>
              <span className="layer-summary">{layer.summary}</span>
            </button>
            {open && (
              <div className="layer-body">
                <p className="layer-explain">{layer.explain}</p>
                <FieldList layer={layer} fields={layer.fields} prefix={key} active={active} onHover={onHover} onPick={onPick} />
              </div>
            )}
          </section>
        );
      })}
    </div>
  );
}

interface FieldListProps {
  layer: PacketLayer;
  fields: PacketField[];
  prefix: string;
  active: string | null;
  onHover(range: ByteRange | null): void;
  onPick(range: ByteRange): void;
}

function FieldList({ layer, fields, prefix, active, onHover, onPick }: FieldListProps) {
  if (!fields.length) return null;
  return (
    <ul className="fields">
      {fields.map((field, index) => {
        const key = `${prefix}.${index}`;
        const range = field.length > 0 ? { key, start: field.start, length: field.length } : null;
        const filter = filterFor(layer, field);
        return (
          <li key={key}>
            <div
              className={`field ${active === key ? "is-active" : ""} ${range ? "has-bytes" : ""}`}
              onMouseEnter={() => onHover(range)}
              onMouseLeave={() => onHover(null)}
              onClick={() => range && onPick(range)}
            >
              <span className="field-label">{field.label}</span>
              <span className="field-value">{field.value}</span>
              <span className="field-actions">
                <CopyButton value={field.value} what={field.label.toLowerCase()} />
                {filter && (
                  <CopyButton value={filter} what={`as filter: ${filter}`} className="as-filter">
                    Filter
                  </CopyButton>
                )}
              </span>
              {field.explain && <span className="field-explain">{field.explain}</span>}
            </div>
            <FieldList layer={layer} fields={field.children} prefix={key} active={active} onHover={onHover} onPick={onPick} />
          </li>
        );
      })}
    </ul>
  );
}

/** The first word of a value: "192.168.1.23 (This PC)" -> "192.168.1.23". */
const firstWord = (value: string) => value.trim().split(/\s+/)[0] ?? "";
const quoted = (value: string) => `"${value.replace(/"/g, '\\"')}"`;

/** A Wireshark-style filter that matches this field's value, when there is an obvious one. */
export function filterFor(layer: PacketLayer, field: PacketField): string | null {
  const label = field.label.toLowerCase();
  const value = field.value.trim();
  if (!value) return null;
  switch (layer.id) {
    case "ip":
    case "ipv6":
      return /address/.test(label) ? `${layer.id}.addr == ${firstWord(value)}` : null;
    case "tcp":
    case "udp":
      return /port/.test(label) ? `${layer.id}.port == ${firstWord(value)}` : null;
    case "eth":
      return label === "source" || label === "destination" ? `eth.addr == ${firstWord(value)}` : null;
    case "dns":
    case "mdns":
      return label === "name" ? `dns.qry.name == ${quoted(value)}` : null;
    case "tls":
      return label.startsWith("server name (sni)") ? `tls.sni == ${quoted(value)}` : null;
    case "http":
      return label === "host" ? `http.host == ${quoted(value)}` : null;
    case "frame":
      return label === "app" && value !== "—" ? `app == ${quoted(value)}` : null;
    default:
      return null;
  }
}

/** The whole breakdown as indented plain text, for pasting into a chat or a bug report. */
export function breakdownText(detail: PacketDetail, summary: string | null): string {
  const lines: string[] = [];
  if (summary) lines.push(summary, "");
  const walk = (fields: PacketField[], depth: number) => {
    for (const field of fields) {
      lines.push(`${"  ".repeat(depth)}${field.label}: ${field.value}`);
      walk(field.children, depth + 1);
    }
  };
  for (const layer of detail.layers) {
    lines.push(`${layer.title}: ${layer.summary}`);
    walk(layer.fields, 1);
  }
  return lines.join("\n");
}

/** The smallest layer or field that covers a byte, for hovering in the hex view. */
export function rangeAt(detail: PacketDetail, offset: number): ByteRange | null {
  let best: ByteRange | null = null;
  const consider = (range: ByteRange) => {
    if (range.length > 0 && offset >= range.start && offset < range.start + range.length) {
      if (!best || range.length <= best.length) best = range;
    }
  };
  const walk = (fields: PacketField[], prefix: string) =>
    fields.forEach((field, index) => {
      const key = `${prefix}.${index}`;
      consider({ key, start: field.start, length: field.length });
      walk(field.children, key);
    });
  detail.layers.forEach((layer, index) => {
    if (layer.id === "frame") return;
    consider({ key: `${index}`, start: layer.start, length: layer.length });
    walk(layer.fields, `${index}`);
  });
  return best;
}
