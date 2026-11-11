// Small text helpers.

const pad = (n: number) => String(n).padStart(2, "0");

/** 14:03:27 */
export function clock(ms: number): string {
  const d = new Date(ms);
  return `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
}

/** "just now", "12 min ago", "3 h ago", then a date. */
export function ago(ms: number, now: number): string {
  const minutes = Math.floor((now - ms) / 60_000);
  if (minutes < 1) return "just now";
  if (minutes < 60) return `${minutes} min ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours} h ago`;
  return new Date(ms).toLocaleDateString(undefined, { year: "numeric", month: "short", day: "numeric" });
}

/** plural(1, "server") -> "1 server"; plural(2, "server") -> "2 servers" */
export function plural(count: number, word: string): string {
  return `${count} ${word}${count === 1 ? "" : "s"}`;
}

/** Cuts long text and marks the cut with an ellipsis. */
export function shorten(text: string, max: number): string {
  return text.length <= max ? text : `${text.slice(0, max - 1)}…`;
}

/** 125 -> "2:05" */
export function countdown(seconds: number): string {
  return `${Math.floor(seconds / 60)}:${pad(seconds % 60)}`;
}
