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

/** 1234567 -> "1.2 MB". Decimal units, like Wireshark. */
export function bytes(count: number): string {
  if (count < 1000) return `${Math.round(count)} B`;
  const units = ["kB", "MB", "GB", "TB"];
  let value = count / 1000;
  let unit = 0;
  while (value >= 1000 && unit < units.length - 1) {
    value /= 1000;
    unit += 1;
  }
  return `${value < 10 ? value.toFixed(1) : Math.round(value)} ${units[unit]}`;
}

/** Bytes per second, e.g. "320 kB/s". */
export function rate(perSecond: number): string {
  return `${bytes(perSecond)}/s`;
}

/** 1234 -> "1,234" */
export function count(n: number): string {
  return Math.round(n).toLocaleString("en-US");
}

/** Seconds to "0:42" or "1:02:03". */
export function elapsed(totalSeconds: number): string {
  const s = Math.max(0, Math.floor(totalSeconds));
  const hours = Math.floor(s / 3600);
  const minutes = Math.floor((s % 3600) / 60);
  return hours ? `${hours}:${pad(minutes)}:${pad(s % 60)}` : `${minutes}:${pad(s % 60)}`;
}

/** 14:03 */
export function hourMinute(ms: number): string {
  const d = new Date(ms);
  return `${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

/** "Today", "Yesterday", a weekday this past week, then "Nov 3" (with the year when it differs). */
export function dayLabel(day: string, now = Date.now()): string {
  const [y, m, d] = day.split("-").map(Number);
  const date = new Date(y, m - 1, d);
  const today = new Date(now);
  today.setHours(0, 0, 0, 0);
  const back = Math.round((today.getTime() - date.getTime()) / 86_400_000);
  if (back === 0) return "Today";
  if (back === 1) return "Yesterday";
  if (back > 1 && back < 7) return date.toLocaleDateString("en-US", { weekday: "long" });
  const options: Intl.DateTimeFormatOptions = { month: "short", day: "numeric" };
  if (date.getFullYear() !== today.getFullYear()) options.year = "numeric";
  return date.toLocaleDateString("en-US", options);
}
