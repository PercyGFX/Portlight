# Portlight plan

What Portlight is for, what exists today, what comes next, and the decisions made along the way. Update this file when a milestone lands or a decision changes.

Last updated: 12 November 2026, version 0.5.0.

## The idea

Wireshark shows packets and leaves you to work out what they mean. Portlight starts from **apps** ("Chrome is talking to github.com") and only shows packets when you drill in. It should be easy for someone new to networking, and pleasant to look at, with animation that explains what is happening.

No packet says which app sent it, so Portlight joins two data feeds:

1. **Who is connecting.** The operating system reports every connection with the process ID that made it: the TCP table (basic) or Windows' network events (detailed, from 0.3.0).
2. **What is on the wire.** Raw packets from Npcap, matched to an app by local port (from 0.3.0).

One limit applies to every tool of this kind: most traffic is encrypted (HTTPS), so Portlight can show who, where, when and how much, but not the contents.

## Decisions

| Decision | Choice | Why |
|---|---|---|
| Design rule | iPhone-simple: any task in a few clicks, plain words, technical options behind "More options" | Portlight exists because Wireshark is hard. Every feature gets a one-click path first; the detail is there for those who want it |
| App shell | Tauri 2: Rust core with a web UI (React, TypeScript) | The animated isometric look is far easier in SVG and CSS than in a pure-Rust GUI |
| Platforms | Windows first, Linux and others later | The connection and blocking parts are OS-specific; they sit behind one interface (`Platform`) so other systems can be added without touching the core or the UI |
| Distribution | Public release, MIT licence | Decided at the start; it drives the installer, updater and signing work below |
| Installer | Inno Setup, per user, no administrator prompt | Same approach as SpeedMeter |
| Updates | Self-update from GitHub Releases, checked against GitHub's SHA-256 | Same flow as SpeedMeter |
| First data source | Snapshots of the TCP connection table | Needs no administrator rights and no driver, so the first release installs and runs for anyone |
| Look | Sea-teal and brass, Manrope and JetBrains Mono, sidebar layout, sonar-ring map | The first version looked too close to the project that inspired it (Protocol Explorer). Version 0.2.0 keeps the idea of an isometric live map and gives Portlight its own identity |
| Name | Portlight | A portlight is the small window in a ship's hull: a window onto your network ports |
| Detailed monitoring (0.3.0) | A small Windows service, **Portlight Collector**, reads ETW events and streams them over a local named pipe | Reading the events needs administrator rights. A service keeps the window unprivileged, installs with one prompt from inside the app, and keeps the basic installer prompt-free. The service only collects; every rule stays in the window |
| Where the service runs from | Copied to `Program Files\Portlight` | It runs as SYSTEM, so its program must not sit in a folder the user (or any program they run) can write to |
| Packet capture order | Moved ahead of blocking | Seeing packets per app was the most asked-for feature; it does not depend on blocking |
| App identity (0.4.0) | Still the program path, plus the publisher's Authenticode signature: a new path with the same publisher and program name is the same app | Paths keep unsigned apps distinct; signatures can't be forged, so an update that moves folders keeps its trust while a look-alike does not |
| Signature checks | Offline: no revocation lists fetched | Keeps the "nothing leaves your PC" promise; the cost is that a certificate revoked after signing still counts |
| Server owners (0.4.0) | iptoasn.com data (public domain), converted to a compact file shipped in the installer and refreshed with each release | Free to redistribute, one file for owner and country, and every lookup stays on the PC. The country is where the network is registered, which is honest about what the data says |
| History (0.4.0) | SQLite (bundled), daily totals per app and server, on by default for 90 days | Small, fast and queryable; daily totals answer "what did this app talk to" without storing anything like a packet log |
| AI assistants (0.5.0) | An MCP server program (`portlight-mcp`) that the assistant app starts, speaking both MCP eras (2026-07-28 per-request, and the older `initialize` handshake); it forwards each tool call to the window over a per-user pipe | Works with every MCP app installed today. The window keeps the data and the decisions: the switch, the log, the permission prompt. The bridge itself never goes online |
| What assistants may do | Read-only tools, plus starting a capture after an Allow prompt (always time-limited) and stopping one; never trust, block, forget or change settings | Useful answers without handing an AI control of the PC |
| Capture design | Npcap loaded at run time; capture by app, server, site name or connection; Wireshark-style display filters; pcapng export | Wireshark captures by network card and needs filter syntax. Portlight captures what you clicked and writes the filter for you, while still speaking Wireshark's language for those who know it |

## What is built

### 0.1.0 and 0.2.0

- **Live app list**, **live map**, **per-app details**, **connection flow** in plain words.
- **New-app alerts** with a two-minute learning period on first run and a Trust button.
- **Background running** in the tray; optional start at sign-in.
- **Light and dark themes**, search, pause, keyboard shortcuts, **demo mode** for UI work.
- **Console front-end** (`portlight-cli`), **installer, self-updater, CI and release pipelines**, tests for the core rules.

### 0.3.0: detailed monitoring and packet capture

- **Portlight Collector** service (Milestone 2). Turned on from the app with one administrator prompt; off again from the same place.
  - Every connection, including ones that last milliseconds.
  - UDP (QUIC, games, calls, name lookups), grouped per app and server.
  - Bytes sent and received, per connection and per app.
  - The site names apps actually looked up ("github.com"), from DNS events, instead of reverse lookups.
  - Names for Windows system processes, which the window alone cannot read.
  - Falls back to basic monitoring whenever the service is not running.
- **Packet capture** (Milestone 5, moved forward), on its own page.
  - Capture one app, one server, one site name (every address it resolves to, plus its lookups), one connection, or everything.
  - Packet list with plain-English summaries or Wireshark-style technical lines, a layer-by-layer breakdown, and a hex view.
  - Search and filter with a friendly subset of Wireshark's display filters; plain words work as search.
  - One click from an app, server or connection starts recording; Start, Stop, Start again and Clear, like Wireshark.
  - Stop automatically after a number of seconds, packets or megabytes.
  - Save as pcapng for Wireshark (with the app name as each packet's comment), or export the list as CSV or the full breakdown as text or JSON. All packets, only the shown ones, the selected one or a range.
  - Animated: a strip above the list shows packets travelling between this PC and what is being captured.
  - Npcap is detected; when it is missing, Portlight explains and links to it.
### 0.4.0: app identity and history (Milestone 3)

- **App icons** everywhere an app appears.
- **Publisher signatures**, checked offline (embedded and Windows catalog signatures): "Signed by …" or "Not signed".
- **Updates no longer re-alert**: a new program path with the same publisher and program name inherits the earlier copy's trust.
- **Server owner and country** from the bundled iptoasn.com list (public domain); local addresses show as "Your home network".
- **History** in SQLite: daily totals per app and server (connections, data, first and last time), 90 days by default, can be turned off and cleared.
- **Settings page**: start at sign-in, close to tray, theme (light, dark, match Windows), new-app notifications and pop-ups, Learn again, forget known apps, detailed monitoring, history, reverse name lookups, Npcap status, updates.
- **About**: version, author, links, credits, and the changelog shown inside the app, with the update button.
- **Port names**: Cloudflare Tunnel, STUN, Remote Desktop, SSH and other common ports are named in the connection flow.

### 0.5.0: AI assistants (Milestone 7a)

- `portlight-mcp.exe`, an MCP server for Claude Desktop, Claude Code, Gemini CLI, Cursor and other MCP apps. Speaks MCP 2026-07-28 and the earlier handshake-based versions.
- Off by default; Settings › AI assistants turns it on, connects Claude Desktop or Claude Code in one click, and shows copyable settings for other apps.
- Read-only tools for apps, connections, owners, history and captured packets; `start_capture` asks the person first and stops by itself.
- "What was shared": every request, with the app's name, what it asked and how much was sent back.
- The uninstaller removes Portlight from Claude Desktop's and Claude Code's settings.

### 0.3.0 (continued)

- **Live view**
  - The map groups servers by site name, keeps local-network devices apart from the internet, opens the hidden apps and servers behind its "+N" bubbles, and, with detailed monitoring, draws busier lanes heavier with a live data rate.
  - Export the apps, open connections or connection history (last 2,000 events) as CSV.
  - Copy buttons for paths, addresses, packet fields and bytes.
  - New-app notifications are combined into one when several apps go online together; the new-apps menu scrolls.

## Known limits today

| Limit | Cause | Fixed by |
|---|---|---|
| An unsigned app that updates into a new folder alerts again | Without a signature, the program path is all there is | Nothing planned; most apps that move folders are signed |
| History counts bytes of a long connection on the day it closes or is measured, and connections open when Portlight starts count as opened that day | Totals are kept per day, not per moment | Good enough for daily totals |
| Server countries are where networks are registered, not where servers stand | The bundled data is about networks (autonomous systems) | A geolocation list could be added later, with attribution |
| In basic mode: short connections missed, TCP only, no bytes, reverse-DNS names, system processes grouped | No administrator rights | Turning on detailed monitoring |
| Name lookups appear under "Windows system services", not the app that asked | Windows' DNS Client service makes lookups on apps' behalf | Could be joined back using the DNS-Client event's process ID (later) |
| Programs that run for only a moment may be unnamed | The collector looks a process up within 0.1 s of its first event; very short-lived ones can be gone | Kernel process events (later) |
| A capture of one app can miss the very first packet of a new connection | Packets are matched by port, and the port is learned from the connection list a moment later; they wait up to 2 s | Good enough in practice; could use the collector's events directly |
| Capture keeps packets in memory, up to 200,000 packets or 512 MB | Simplicity | Ring buffer or capture to disk (later) |
| The collector service does not update itself | It needs administrator rights to replace | The app offers "Update the background service" (one prompt) |

## Roadmap

Each milestone ends with something that works.

| Order | Milestone | State |
|---|---|---|
| 1 | Monitoring MVP | Done (0.1.0, 0.2.0) |
| 2 | Event-based source (Portlight Collector) | Done (0.3.0) |
| 5 | Packet capture | Done (0.3.0), moved ahead of 3 and 4 |
| 3 | Richer app identity and history | Done (0.4.0) |
| 3b | Unusual-activity hints | Next |
| 4 | Blocking | After 3b |
| 5+ | Capture follow-ups | Any time |
| 6 | Replay and polish (the settings page part is done in 0.4.0) | Later |
| 7a | MCP server: let your own AI assistant read Portlight | Done (0.5.0), moved ahead of 3b |
| 7 | Ask questions with your own AI key | Later |

### Milestone 3: richer app identity and history (done in 0.4.0)

- App icons in the list and on the map.
- Identify an app by its publisher's signature as well as its path, so updates do not re-alert, and show the publisher ("Signed by Mozilla Corporation" or "Unsigned").
- Country and owner of each server, from an offline database, so no lookups leave the PC.
- History on disk (SQLite): what each app connected to, by day.

### Milestone 3b: unusual-activity hints (no AI needed)

Rules that point out what looks odd, each with its reason in plain words ("Runs from Downloads · Connects every 60 s · No name lookup first"). Hints, never verdicts: every rule has innocent exceptions.

- **The program:** runs from Downloads, Temp or AppData; a Windows name (`svchost.exe`, `explorer.exe`) outside `System32`; Windows tools that rarely go online (`powershell`, `rundll32`, `mshta`, `certutil`, `regsvr32`, `wscript`); unsigned (needs Milestone 3).
- **The connection** (detailed monitoring): straight to an address with no name lookup first; regular "beaconing" (the same gap between connections, again and again); much more data sent than received; unusual ports (4444, 1337, 6667, 9001, 9050); many different addresses in a short time.
- **Name lookups:** random-looking names or floods of lookups (malware-made domains, data hidden in DNS).
- **Known-bad lists**, downloaded daily and checked on the PC (for example abuse.ch's Feodo Tracker, Tor exit nodes). Nothing about your traffic is sent anywhere.
- An "Unusual" badge on the app, the reasons in its details, and an optional notification.
- Works best with history (Milestone 3), so "unusual" has a baseline.

### Milestone 4: blocking

- **Block an app** and **block an address**, using the Windows Filtering Platform, the firewall engine built into Windows. No driver needed. The collector service already has the rights this needs.
- **Block a domain.** Harder, because firewalls work on addresses and not names. Portlight would block the addresses it sees that domain resolve to. Two known weaknesses: sites that share an address (CDNs) get blocked together, and browsers using encrypted DNS can slip past.
- A Block button on the new-app alert.
- **Strict mode:** block everything until you allow it. Without it, an alert arrives just after an app's first connection, not before.

### Milestone 5 follow-ups (capture)

- Follow a TCP conversation as a whole ("Follow stream").
- Open a saved pcapng file.
- Capture to disk for long sessions.

### Milestone 6: replay and polish

- Replay a recorded capture on the map, with play, step and speed controls.
- Translations.
- Optional monthly refresh of the server owner list between releases (downloads the whole list, so it reveals nothing about which servers are used; off by default).
- Clearer wording for DHCPv6, NetBIOS and traffic from virtual machines (WSL, Hyper-V) in packet summaries.

### Milestone 7a: an MCP server for AI assistants (done in 0.5.0)

Built as planned, with these details:

- 14 tools: `portlight_status`, `list_apps`, `get_app`, `list_connections`, `recent_activity`, `new_apps`, `lookup_server`, `app_history`, `history_summary`, `capture_status`, `read_packets`, `explain_packet`, `start_capture`, `stop_capture`.
- Answers carry both readable JSON text and `structuredContent`; errors come back as tool results written for the assistant to relay ("AI assistant access is turned off in Portlight…").
- Rate limit of 120 requests a minute; lists capped (200 apps, 500 connections or events, 200 packets per call).
- "What was shared" is kept in memory, not on disk.
- Not yet: MCP resources and prompts, `subscriptions/listen` for live updates, the Streamable HTTP transport. None is needed by today's clients.

Original plan:

Let AI assistants that speak the Model Context Protocol (Claude Desktop, Claude Code, and other MCP clients) ask Portlight what is happening, so people can use the assistant they already have instead of entering a key into Portlight.

- A small program, `portlight-mcp`, that the assistant starts (MCP over stdio). It talks to the running Portlight window over a local named pipe, like the collector does, and never touches the network itself.
- **Read-only tools first:** list the apps online, one app's details and servers, the open connections, recent connection history, new apps waiting for review, and a running capture's summary and individual packet breakdowns (the same plain-English breakdown the capture page shows).
- **Actions only with consent:** starting or stopping a capture, or (after Milestone 4) blocking, shows a prompt in Portlight that the person must accept. Trusting an app is never done by an assistant.
- **Off by default.** Turned on in Portlight's settings, with a one-click "Add to Claude Desktop" that writes the client's MCP config, and a copyable config snippet for other clients.
- **Privacy, stated plainly:** Portlight itself still sends nothing anywhere. Whatever the assistant reads goes to the assistant, and from there to whichever AI service it uses. The settings page, the README's "Your data" section and the tool descriptions say so. A "what was shared" log in Portlight lists every request an assistant made.
- Answers carry facts, not verdicts: names, counts, bytes, times and the unusual-activity reasons from Milestone 3b, so the assistant does the explaining.
- Simpler than Milestone 7 (no keys, no provider code, no chat window), so it comes first; Milestone 7 can reuse the same tools for its own chat.

### Milestone 7: ask questions, with your own AI key

Let people ask about what they see in plain language, using an AI model they pay for themselves.

- You enter your own API key for an AI provider. Portlight ships with no key and has no account of its own.
- Example questions: "What is this app and why would it connect to this server?", "Is anything unusual online right now?", "Summarise what went online today."
- An **Explain** button on a new-app alert, an app, a connection or a packet, which asks the question for you.
- Rules that keep the "everything stays on your PC" promise honest:
  - Nothing is sent unless you ask a question. No background analysis.
  - Before sending, Portlight shows exactly what will leave the PC (app names, server names, counts) and you can remove items.
  - The key is kept in Windows Credential Manager, not in a settings file.
  - The README's "Your data" section changes in the same release.
- The provider sits behind one small interface, so more than one can be supported.
- Most useful once history (Milestone 3) exists, because "today" and "unusual" need a record to compare against.

### Later

- Linux, then macOS, by adding a module to `portlight-platform`.
- Code signing. Until the installer is signed, Windows SmartScreen warns on first run, and some antivirus tools are wary of unsigned network software (more so now that there is a service).

## Open questions

- **Blocking a domain reliably** may need inspecting DNS traffic directly, which needs a driver. Revisit after the simple version ships.
- **Trademark and domain** for the name "Portlight" have not been checked. Do this before promoting the project.
- **Should capture run inside the service?** Today it runs in the window, which works with Npcap's default install. If Npcap is installed for administrators only, capture fails with an explanation; routing capture through the service would avoid that at the cost of streaming packets over the pipe.
