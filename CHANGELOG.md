# Changelog

All notable changes to Portlight are listed here. Versions follow
[Semantic Versioning](https://semver.org/).

## [0.5.1] - 2026-11-12

### Fixed

- **Claude Code couldn't load Portlight's tools** ("tools fetch failed").
  Apps using the newest MCP version (2026-07-28) need caching details on the
  tool list, and Portlight now sends them.

## [0.5.0] - 2026-11-12

Ask your AI assistant about your network.

### Added

- **AI assistants (MCP).** Claude Desktop, Claude Code, Gemini CLI, Cursor and
  other apps that support the Model Context Protocol can now ask Portlight
  questions: which apps are online, what an app talks to and who owns those
  servers, what changed today, what a captured packet means. Off until you
  turn it on in Settings › AI assistants.
- **One click to connect** Claude Desktop or Claude Code, and ready-to-copy
  settings for other apps.
- **A full guide** to connecting assistants, every tool and troubleshooting:
  [docs/MCP.md](docs/MCP.md).
- **You stay in charge.** Assistants can look, not change anything. Starting
  a capture shows an Allow / Don't allow prompt in Portlight and stops by
  itself. Every request is listed under "What was shared".
- Uninstalling Portlight also takes it out of Claude Desktop's and Claude
  Code's settings.

## [0.4.0] - 2026-11-12

Know your apps, and remember what they did.

### Added

- **App icons** in the app list, the map, the details and the capture page.
- **Who made each app**: "Signed by Mozilla Corporation" or "Not signed",
  checked on your PC without going online.
- **Updates no longer re-alert.** When an app updates into a new folder
  (Discord, Slack and many others do), Portlight recognises it by its
  publisher and program name and keeps it trusted.
- **Who owns each server and where its network is registered**, such as
  "Cloudflare · United States", from a public-domain list bundled with
  Portlight. Nothing is looked up online.
- **History**: which app talked to which server, how often and how much
  data, by day. On by default, kept 90 days, stored only on your PC. Turn it
  off, change how long it is kept, or clear it in Settings.
- **Settings** in one place: start at sign-in, keep running in the tray,
  theme (including "match Windows"), new-app notifications and pop-ups,
  detailed monitoring, history, server name lookups, Npcap and updates.
- **Learn again**: trust every app you have now and start a fresh two-minute
  quiet period, for when a burst of alerts arrives at once. Or forget every
  known app and start over.
- **About**: version, credits, links, and what's new in each version, inside
  the app.

## [0.3.0] - 2026-11-12

Detailed monitoring and packet capture.

### Added

- **Detailed monitoring**, through a small background service, Portlight
  Collector, that you turn on from inside the app with one administrator
  prompt. With it on, Portlight sees:
  - every connection, including ones that last only milliseconds;
  - UDP traffic (QUIC, games, calls, name lookups);
  - how much data each app and each connection sends and receives;
  - the site names apps actually looked up ("github.com"), instead of a
    hosting company's server name;
  - the names of Windows system processes.
  Without it, Portlight works as before.
- **Packet capture** on its own page. Capture one app, one server, one site,
  one connection or everything, then read each packet as a plain-English
  summary or a Wireshark-style line, with a layer-by-layer breakdown and the
  raw bytes. Uses Npcap, the capture driver Wireshark uses; Portlight tells
  you how to get it if it is missing.
- **Search and filter** captured packets: type plain words, or use
  Wireshark-style filters such as `dns`, `tls`, `tcp.port == 443`.
- **Save captures** as `.pcapng` files that Wireshark opens (each packet
  notes which app it came from), export the list as CSV, or export the full
  breakdown as text or JSON. Save everything, only what your search shows,
  one packet or a range.
- **Stop a capture automatically** after a number of seconds, packets or
  megabytes.
- **Export the live view** (apps, open connections, connection history) as CSV.
- **Copy buttons** for paths, addresses, packet fields and bytes.

### Changed

- When several apps go online at once, Windows shows one notification
  instead of one per app.
- The new-apps menu scrolls when the list is long, instead of running off
  the window.

## [0.2.0] - 2026-11-11

A new look of Portlight's own.

### Changed

- New colours: sea-teal ink on warm paper, lamplit water in dark mode, and
  brass for anything new.
- New typefaces: Manrope for words and JetBrains Mono for addresses and times.
  The app is about 3 MB smaller as a result.
- New layout: apps and search in a sidebar, the map above the connection flow,
  and details in their own column.
- The map is drawn over sonar rings around this PC, with curved, dotted lanes.
- Rounded panels, buttons and tabs; apps in the list show an initial.
- The connection flow says what happened in fewer words, with the server in
  its own column.
- The app icon is now a round porthole with a brass ring.

## [0.1.0] - 2026-11-11

First release: the monitoring MVP.

### Added

- Live list of every app with an open internet connection, with friendly app
  names taken from each program's version details.
- Live map: apps, this PC, the internet and the servers each app is talking
  to, with a dot travelling along the lanes when a connection opens or closes.
- Per-app details: program path, first seen, open connections and the servers
  it is connected to.
- Connection flow: connections opening and closing, in plain words.
- New-app alerts: a Windows notification and an in-app alert the first time an
  app goes online. The first two minutes of the first run learn what is
  already there without alerting.
- Tray icon: closing the window keeps Portlight watching.
- Light and dark themes, search (Ctrl+K), pause (Space).
- Installer (per user, no administrator prompt) with optional start at sign-in.
