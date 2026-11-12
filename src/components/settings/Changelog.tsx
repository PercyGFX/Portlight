// "What's new": the repository's CHANGELOG.md, bundled at build time and drawn with a tiny Markdown reader.

import { useMemo, useState, type ReactNode } from "react";

import changelog from "../../../CHANGELOG.md?raw";
import { ChevronRightIcon } from "../icons";

interface Release {
  version: string;
  date: string | null;
  /** Lines under the version heading. */
  lines: string[];
}

/** Splits the file into releases: "## [0.4.0] - 2026-11-12" starts one. */
function releases(text: string): Release[] {
  const result: Release[] = [];
  for (const line of text.replace(/\r/g, "").split("\n")) {
    const heading = /^##\s+\[?([^\]\s]+)\]?(?:\s+-\s+(\S+))?/.exec(line);
    if (heading) {
      result.push({ version: heading[1], date: heading[2] ?? null, lines: [] });
    } else if (result.length && !/^\[[^\]]+\]:\s/.test(line)) {
      result[result.length - 1].lines.push(line);
    }
  }
  return result;
}

/** **bold**, `code` and [links](...) (links show as their words). */
function inline(text: string): ReactNode[] {
  const parts: ReactNode[] = [];
  const pattern = /\*\*(.+?)\*\*|`([^`]+)`|\[([^\]]+)\]\([^)]+\)/g;
  let last = 0;
  let match: RegExpExecArray | null;
  while ((match = pattern.exec(text))) {
    if (match.index > last) parts.push(text.slice(last, match.index));
    if (match[1] !== undefined) parts.push(<strong key={match.index}>{inline(match[1])}</strong>);
    else if (match[2] !== undefined) parts.push(<code key={match.index}>{match[2]}</code>);
    else parts.push(match[3]);
    last = match.index + match[0].length;
  }
  if (last < text.length) parts.push(text.slice(last));
  return parts;
}

interface Item {
  text: string;
  children: string[];
}

type Block = { kind: "heading"; text: string } | { kind: "para"; text: string } | { kind: "list"; items: Item[] };

/** Headings, paragraphs and bullet lists (one level of nesting); wrapped lines are joined. */
function blocks(lines: string[]): Block[] {
  const result: Block[] = [];
  let para: string[] = [];
  let list: Item[] | null = null;
  const flush = () => {
    if (para.length) result.push({ kind: "para", text: para.join(" ") });
    if (list) result.push({ kind: "list", items: list });
    para = [];
    list = null;
  };

  for (const raw of lines) {
    const line = raw.trimEnd();
    if (!line.trim()) {
      flush();
      continue;
    }
    const heading = /^#{3,6}\s+(.*)/.exec(line);
    const bullet = /^- (.*)/.exec(line);
    const sub = /^\s{2,}- (.*)/.exec(line);
    if (heading) {
      flush();
      result.push({ kind: "heading", text: heading[1] });
    } else if (bullet) {
      if (para.length) flush();
      list = list ?? [];
      list.push({ text: bullet[1], children: [] });
    } else if (sub && list?.length) {
      list[list.length - 1].children.push(sub[1]);
    } else if (list?.length) {
      // A wrapped line belongs to the last bullet (or its last sub-bullet).
      const item = list[list.length - 1];
      if (item.children.length) item.children[item.children.length - 1] += ` ${line.trim()}`;
      else item.text += ` ${line.trim()}`;
    } else {
      para.push(line.trim());
    }
  }
  flush();
  return result;
}

function ReleaseBody({ lines }: { lines: string[] }) {
  return (
    <div className="changelog-body">
      {blocks(lines).map((block, i) =>
        block.kind === "heading" ? (
          <h4 key={i}>{block.text}</h4>
        ) : block.kind === "para" ? (
          <p key={i}>{inline(block.text)}</p>
        ) : (
          <ul key={i}>
            {block.items.map((item, j) => (
              <li key={j}>
                {inline(item.text)}
                {item.children.length > 0 && (
                  <ul>
                    {item.children.map((child, k) => (
                      <li key={k}>{inline(child)}</li>
                    ))}
                  </ul>
                )}
              </li>
            ))}
          </ul>
        ),
      )}
    </div>
  );
}

function formatDate(date: string | null): string {
  if (!date) return "";
  const [y, m, d] = date.split("-").map(Number);
  if (!y || !m || !d) return date;
  return new Date(y, m - 1, d).toLocaleDateString("en-GB", { day: "numeric", month: "short", year: "numeric" });
}

/** The newest release open, older ones one tap away. */
export function Changelog() {
  const list = useMemo(() => releases(changelog).filter((release) => release.version.toLowerCase() !== "unreleased"), []);
  const [open, setOpen] = useState<ReadonlySet<string>>(() => new Set(list.slice(0, 1).map((r) => r.version)));

  if (!list.length) return <p className="empty">No release notes in this copy.</p>;

  return (
    <ul className="changelog">
      {list.map((release) => {
        const isOpen = open.has(release.version);
        return (
          <li key={release.version} className={isOpen ? "is-open" : ""}>
            <button
              className="changelog-head"
              aria-expanded={isOpen}
              onClick={() =>
                setOpen((current) => {
                  const next = new Set(current);
                  if (next.has(release.version)) next.delete(release.version);
                  else next.add(release.version);
                  return next;
                })
              }
            >
              <strong>Version {release.version}</strong>
              <span>{formatDate(release.date)}</span>
              <ChevronRightIcon />
            </button>
            {isOpen && <ReleaseBody lines={release.lines} />}
          </li>
        );
      })}
    </ul>
  );
}
