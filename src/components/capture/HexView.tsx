// The raw bytes of a packet: 16 per line, as hex and as text, with the hovered field lit up.

import { useEffect, useMemo, useRef } from "react";

import { CopyButton } from "../CopyButton";

interface Props {
  bytes: number[];
  /** Bytes to light up. */
  start: number;
  length: number;
  onHoverByte(offset: number | null): void;
  onPickByte(offset: number): void;
}

const PER_LINE = 16;
const hex2 = (n: number) => n.toString(16).padStart(2, "0");
const printable = (n: number) => (n >= 0x20 && n < 0x7f ? String.fromCharCode(n) : "·");

export function HexView({ bytes, start, length, onHoverByte, onPickByte }: Props) {
  const lines = useMemo(() => {
    const result: number[] = [];
    for (let offset = 0; offset < bytes.length; offset += PER_LINE) result.push(offset);
    return result;
  }, [bytes]);

  const asHex = useMemo(() => bytes.map(hex2).join(" "), [bytes]);
  const asText = useMemo(() => bytes.map(printable).join(""), [bytes]);
  const end = start + length;
  const scroller = useRef<HTMLDivElement>(null);

  // Bring the lit bytes into view when a field far down the packet is picked.
  useEffect(() => {
    if (length <= 0) return;
    scroller.current?.querySelector(".is-lit")?.scrollIntoView({ block: "nearest" });
  }, [start, length]);
  const lit = (offset: number) => (offset >= start && offset < end ? "is-lit" : "");

  return (
    <div className="hex">
      <div className="hex-head">
        <h3 className="section-title">Bytes</h3>
        <span className="hex-note">{length > 0 ? `${length} highlighted, from byte ${start}` : `${bytes.length} bytes`}</span>
        <CopyButton value={asHex} what="bytes as hex">Hex</CopyButton>
        <CopyButton value={asText} what="bytes as text">Text</CopyButton>
      </div>
      <div className="hex-scroll" ref={scroller} onMouseLeave={() => onHoverByte(null)}>
        {lines.map((lineStart) => {
          const line = bytes.slice(lineStart, lineStart + PER_LINE);
          return (
            <div key={lineStart} className="hex-line">
              <span className="hex-offset">{lineStart.toString(16).padStart(4, "0")}</span>
              <span className="hex-bytes">
                {line.map((byte, i) => {
                  const offset = lineStart + i;
                  return (
                    <span
                      key={i}
                      className={`hex-byte ${lit(offset)} ${i === 7 ? "is-gap" : ""}`}
                      onMouseEnter={() => onHoverByte(offset)}
                      onClick={() => onPickByte(offset)}
                    >
                      {hex2(byte)}
                    </span>
                  );
                })}
              </span>
              <span className="hex-text">
                {line.map((byte, i) => {
                  const offset = lineStart + i;
                  return (
                    <span key={i} className={`hex-char ${lit(offset)}`} onMouseEnter={() => onHoverByte(offset)} onClick={() => onPickByte(offset)}>
                      {printable(byte)}
                    </span>
                  );
                })}
              </span>
            </div>
          );
        })}
      </div>
    </div>
  );
}
