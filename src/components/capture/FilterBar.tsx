// The search and filter box above the packet list, with quick chips and a short guide to the filter language.

import { useCallback, useEffect, useRef, useState, type RefObject } from "react";

import { useDismiss } from "../../hooks/useDismiss";
import { HelpIcon, SearchIcon } from "../icons";

interface Props {
  inputRef: RefObject<HTMLInputElement | null>;
  /** The filter applied now. */
  applied: string;
  error: string | null;
  onApply(filter: string): void;
}

const DEBOUNCE_MS = 300;

const CHIPS: Array<[label: string, filter: string, hint: string]> = [
  ["All", "", "Show every packet"],
  ["Web", "tls or http or quic", "Web pages and apps talking to their servers"],
  ["Names", "dns", "Looking up the address of a site name"],
  ["Connections opening", "tcp.flags.syn == 1", "The first packets of each new connection"],
  ["Problems", "tcp.flags.reset == 1 or icmp", "Connections cut off, and network error messages"],
];

export function FilterBar({ inputRef, applied, error, onApply }: Props) {
  const [text, setText] = useState(applied);
  // The text last sent, so we know whether the error belongs to what is typed.
  const [sent, setSent] = useState(applied);

  // Follow the applied filter when it changes from elsewhere (a chip, an example).
  useEffect(() => {
    setText((current) => (current.trim() === applied ? current : applied));
    setSent(applied);
  }, [applied]);

  const apply = useCallback(
    (value: string) => {
      const filter = value.trim();
      setSent(filter);
      onApply(filter);
    },
    [onApply],
  );

  // Apply while typing, after a short pause.
  useEffect(() => {
    if (text.trim() === sent) return;
    const timer = window.setTimeout(() => apply(text), DEBOUNCE_MS);
    return () => window.clearTimeout(timer);
  }, [text, sent, apply]);

  const typed = text.trim();
  const settled = typed === sent;
  const state = !settled ? "is-pending" : error && sent !== applied ? "is-invalid" : typed ? "is-valid" : "";

  const choose = (filter: string) => {
    setText(filter);
    apply(filter);
  };

  return (
    <div className="filter-bar">
      <div className="filter-row">
        <label className={`search filter-input ${state}`}>
          <SearchIcon />
          <input
            ref={inputRef}
            type="search"
            value={text}
            spellCheck={false}
            placeholder='Search packets — try "github", dns, tls or tcp.port == 443'
            aria-label="Search packets"
            aria-invalid={state === "is-invalid"}
            onChange={(event) => setText(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter") apply(text);
              if (event.key === "Escape" && text) {
                event.stopPropagation();
                choose("");
              }
            }}
          />
          <kbd>Ctrl F</kbd>
        </label>
        <FilterHelp onExample={choose} />
      </div>
      <div className="chips" role="group" aria-label="Quick filters">
        {CHIPS.map(([label, filter, hint]) => (
          <button
            key={label}
            className={`chip ${applied === filter ? "is-active" : ""}`}
            aria-pressed={applied === filter}
            title={filter ? `${hint}  (${filter})` : hint}
            onClick={() => choose(filter)}
          >
            {label}
          </button>
        ))}
      </div>
      {state === "is-invalid" && (
        <p className="filter-error" role="alert">
          {error} <span>Still showing the last filter that worked{applied ? `: ${applied}` : ""}.</span>
        </p>
      )}
    </div>
  );
}

const EXAMPLES: Array<[filter: string, meaning: string]> = [
  ["github", "Any packet that mentions github: names, addresses, the description"],
  ["dns", "Name lookups. Also: tls, tcp, udp, quic, http, arp, icmp"],
  ["tcp.port == 443", "Secure web traffic on its usual port. Also port == 443"],
  ["ip.addr == 192.168.0.0/16", "To or from your home network"],
  ['dns.qry.name contains "google"', "Lookups for any name with google in it"],
  ['app == "Firefox"', "Only one app's packets (Portlight's own field)"],
  ["frame.len > 1000", "Big packets"],
  ["tls and not ip.addr == 192.168.1.1", "Combine with and, or, not and ( )"],
];

/** The "?" button: what can be typed in the box. */
function FilterHelp({ onExample }: { onExample(filter: string): void }) {
  const [open, setOpen] = useState(false);
  const root = useRef<HTMLDivElement>(null);
  const close = useCallback(() => setOpen(false), []);
  useDismiss(root, open, close);

  return (
    <div className="popover" ref={root}>
      <button
        className="button button-icon"
        aria-expanded={open}
        aria-label="How to search and filter"
        title="How to search and filter"
        onClick={() => setOpen((v) => !v)}
      >
        <HelpIcon />
      </button>
      {open && (
        <div className="menu help-menu" role="dialog" aria-label="How to search and filter">
          <div className="menu-head">
            <h2 className="section-title">Search or filter</h2>
          </div>
          <p className="menu-text">
            Type any word to search: a site, an app, an address. Or use a filter, the same language as Wireshark's display
            filters, so what you learn here works there too. Click an example to try it.
          </p>
          <ul className="help-examples">
            {EXAMPLES.map(([filter, meaning]) => (
              <li key={filter}>
                <button
                  className="link"
                  onClick={() => {
                    onExample(filter);
                    setOpen(false);
                  }}
                >
                  <code>{filter}</code>
                  <span>{meaning}</span>
                </button>
              </li>
            ))}
          </ul>
          <p className="menu-text">
            Compare with <code>== != &gt; &lt; contains</code>. Fields include ip.src, ip.dst, tcp.flags.syn,
            tls.sni, http.host and frame.number. If Portlight does not understand something, it says why under the box.
          </p>
        </div>
      )}
    </div>
  );
}
