// Made-up app icons for the demo, drawn as small SVGs. No real logos.

const svg = (body: string) =>
  `data:image/svg+xml;charset=utf-8,${encodeURIComponent(
    `<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64" viewBox="0 0 64 64">${body}</svg>`,
  )}`;

/** A rounded tile with a gradient, the usual shape of a Windows app icon. */
const tile = (id: string, from: string, to: string, glyph: string) =>
  svg(
    `<defs><linearGradient id="${id}" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="${from}"/><stop offset="1" stop-color="${to}"/></linearGradient></defs>` +
      `<rect x="4" y="4" width="56" height="56" rx="14" fill="url(#${id})"/>` +
      `<rect x="4.5" y="4.5" width="55" height="55" rx="13.5" fill="none" stroke="#000" stroke-opacity=".12"/>` +
      glyph,
  );

const ICONS: Record<string, string> = {
  // A browser: a warm globe with an orbit.
  firefox: svg(
    `<defs><radialGradient id="b" cx=".35" cy=".3" r=".8"><stop offset="0" stop-color="#ffd25a"/><stop offset=".55" stop-color="#ff7139"/><stop offset="1" stop-color="#c4246b"/></radialGradient>` +
      `<linearGradient id="g" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#5b3bd6"/><stop offset="1" stop-color="#2a1a8a"/></linearGradient></defs>` +
      `<circle cx="32" cy="32" r="27" fill="url(#b)"/>` +
      `<circle cx="34" cy="34" r="15" fill="url(#g)"/>` +
      `<path d="M21 34a13 13 0 0 1 26 0M21 34h26M34 21c-4 4-4 22 0 26M34 21c4 4 4 22 0 26" stroke="#bfb2ff" stroke-width="1.6" fill="none" opacity=".75"/>` +
      `<path d="M9 40c6 10 20 16 32 12" stroke="#fff3c4" stroke-width="3" stroke-linecap="round" fill="none" opacity=".9"/>`,
  ),
  // A code editor: angle brackets on blue.
  code: tile(
    "c",
    "#2b9cf0",
    "#0b5cad",
    `<path d="M25 22 14 32l11 10M39 22l11 10-11 10" stroke="#fff" stroke-width="5" stroke-linecap="round" stroke-linejoin="round" fill="none"/>` +
      `<path d="M35 18 29 46" stroke="#cfe8ff" stroke-width="4" stroke-linecap="round"/>`,
  ),
  // A music player: a note on coral.
  tunes: tile(
    "t",
    "#ff6b8b",
    "#d6245a",
    `<path d="M27 42V20l17-4v22" stroke="#fff" stroke-width="4" stroke-linejoin="round" fill="none"/>` +
      `<ellipse cx="22.5" cy="43" rx="6" ry="5" fill="#fff"/><ellipse cx="39.5" cy="39" rx="6" ry="5" fill="#fff"/>`,
  ),
  // A chat app: a speech bubble on indigo.
  chatter: tile(
    "h",
    "#7b83ff",
    "#4a3fd0",
    `<path d="M16 22a6 6 0 0 1 6-6h20a6 6 0 0 1 6 6v13a6 6 0 0 1-6 6H30l-8 7v-7a6 6 0 0 1-6-6Z" fill="#fff"/>` +
      `<circle cx="25" cy="28.5" r="2.6" fill="#5a52e0"/><circle cx="32" cy="28.5" r="2.6" fill="#5a52e0"/><circle cx="39" cy="28.5" r="2.6" fill="#5a52e0"/>`,
  ),
  // A mail app: an envelope on sky blue.
  mail: tile(
    "m",
    "#4fc3f7",
    "#1976d2",
    `<rect x="14" y="20" width="36" height="25" rx="4" fill="#fff"/>` +
      `<path d="m15 22 17 13 17-13" stroke="#1e88e5" stroke-width="3" stroke-linejoin="round" fill="none"/>`,
  ),
  // A game launcher: a controller on deep navy.
  games: tile(
    "l",
    "#30415f",
    "#121a2b",
    `<path d="M22 24h20c6 0 10 6 10 13 0 5-3 8-6 8-3 0-5-3-7-5H25c-2 2-4 5-7 5-3 0-6-3-6-8 0-7 4-13 10-13Z" fill="#9fd3ff"/>` +
      `<path d="M22 31v8M18 35h8" stroke="#121a2b" stroke-width="3" stroke-linecap="round"/>` +
      `<circle cx="41" cy="32.5" r="2.4" fill="#121a2b"/><circle cx="45.5" cy="37" r="2.4" fill="#121a2b"/>`,
  ),
  // An unsigned helper with Windows' plain "program" picture.
  generic: svg(
    `<rect x="8" y="12" width="48" height="40" rx="4" fill="#f4f6f8" stroke="#8a97a3" stroke-width="2"/>` +
      `<rect x="8" y="12" width="48" height="9" rx="4" fill="#6f8fb0"/><rect x="8" y="17" width="48" height="4" fill="#6f8fb0"/>` +
      `<rect x="15" y="27" width="20" height="3" rx="1.5" fill="#b8c2cc"/><rect x="15" y="34" width="30" height="3" rx="1.5" fill="#b8c2cc"/><rect x="15" y="41" width="24" height="3" rx="1.5" fill="#b8c2cc"/>`,
  ),
};

/** The demo icon for an icon key, or null. */
export function demoIcon(key: string | null): string | null {
  return key ? (ICONS[key] ?? null) : null;
}
