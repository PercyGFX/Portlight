# Portlight plan

What Portlight is for, what exists today, what comes next, and the decisions made along the way. Update this file when a milestone lands or a decision changes.

Last updated: 11 November 2026, version 0.1.0.

## The idea

Wireshark shows packets and leaves you to work out what they mean. Portlight starts from **apps** ("Chrome is talking to github.com") and only shows packets when you drill in. It should be easy for someone new to networking, and pleasant to look at, with animation that explains what is happening.

No packet says which app sent it, so the finished product joins two data feeds:

1. **Who is connecting.** The operating system reports every connection with the process ID that made it.
2. **What is on the wire.** Raw packets from a capture driver, matched to an app by address and port.

Version 0.1.0 builds the first feed. The second arrives with packet capture.

One limit applies to every tool of this kind: most traffic is encrypted (HTTPS), so Portlight can show who, where, when and how much, but not the contents.

## Decisions

| Decision | Choice | Why |
|---|---|---|
| App shell | Tauri 2: Rust core with a web UI (React, TypeScript) | The animated isometric look is far easier in SVG and CSS than in a pure-Rust GUI |
| Platforms | Windows first, Linux and others later | The connection and blocking parts are OS-specific; they sit behind one interface (`Platform`) so other systems can be added without touching the core or the UI |
| Distribution | Public release, MIT licence | Decided at the start; it drives the installer, updater and signing work below |
| Installer | Inno Setup, per user, no administrator prompt | Same approach as SpeedMeter |
| Updates | Self-update from GitHub Releases, checked against GitHub's SHA-256 | Same flow as SpeedMeter |
| First data source | Snapshots of the TCP connection table | Needs no administrator rights and no driver, so the first release installs and runs for anyone |
| Name | Portlight | A portlight is the small window in a ship's hull: a window onto your network ports |

## What is built (0.1.0)

- **Live app list.** Every app with an open connection, named from the program's version details, busiest first.
- **Live map.** Apps, this PC, the internet and servers, with a dot travelling along the lanes when a connection opens or closes.
- **Per-app details.** Program path, first seen, open connections, servers.
- **Connection flow.** Connections opening and closing, in plain words.
- **New-app alerts.** A Windows notification and an in-app alert the first time an app goes online, with a two-minute learning period on first run and a Trust button.
- **Background running.** Tray icon; closing the window keeps Portlight watching; optional start at sign-in.
- **Light and dark themes**, search, pause, keyboard shortcuts.
- **Demo mode.** The UI runs in a plain browser with invented data, for UI work and screenshots.
- **Console front-end** (`portlight-cli`) on the same core.
- **Installer, self-updater, CI and release pipelines.**
- **Tests** for the core rules (new-app detection, learning, event diffing, process-ID reuse).

## Known limits of 0.1.0

| Limit | Cause | Fixed by |
|---|---|---|
| Very short connections can be missed | The connection table is read twice a second | Milestone 2 (event-based source) |
| UDP is not shown | The table used only covers TCP | Milestone 2 |
| System processes are one group, "Windows system services" | Windows hides their paths from non-administrator programs | Milestone 2 (runs elevated) |
| Server names are often a hosting company's | Names come from reverse DNS | Milestone 2 (read the names apps asked for) |
| An app that updates into a new folder alerts again | Apps are identified by program path | Milestone 3 (identify by publisher signature) |
| No data volumes | The table has no byte counts | Milestone 2 |

## Roadmap

Each milestone ends with something that works.

### Milestone 2: the event-based source

Replace polling with Windows' built-in event stream (ETW), which reports every connection as it happens, with its process ID.

- Connections that last milliseconds are no longer missed.
- UDP traffic appears.
- Bytes sent and received per app and per connection.
- DNS events give the name each app actually asked for ("github.com"), replacing reverse lookups.
- Needs administrator rights, which means splitting Portlight into a small background service that collects data and the normal window that shows it. The service is installed once, with one administrator prompt.

### Milestone 3: richer app identity and history

- App icons in the list and on the map.
- Identify an app by its publisher's signature as well as its path, so updates do not re-alert, and show the publisher ("Signed by Mozilla Corporation" or "Unsigned").
- Country and owner of each server, from an offline database, so no lookups leave the PC.
- History on disk (SQLite): what each app connected to, by day.

### Milestone 4: blocking

- **Block an app** and **block an address**, using the Windows Filtering Platform, the firewall engine built into Windows. No driver needed.
- **Block a domain.** Harder, because firewalls work on addresses and not names. Portlight would block the addresses it sees that domain resolve to. Two known weaknesses: sites that share an address (CDNs) get blocked together, and browsers using encrypted DNS can slip past.
- A Block button on the new-app alert.
- **Strict mode:** block everything until you allow it. Without it, an alert arrives just after an app's first connection, not before.

### Milestone 5: packet capture

- Start and stop a capture, filtered by app.
- Click a packet for a plain-English breakdown of its layers.
- Save as pcapng, which Wireshark opens.
- Uses Npcap, the driver Wireshark uses. Its free licence does not allow bundling, so Portlight will detect it and guide the user to install it.

### Milestone 6: replay and polish

- Replay a recorded capture on the map, with play, step and speed controls.
- A settings page.
- Translations.

### Milestone 7: ask questions, with your own AI key

Let people ask about what they see in plain language, using an AI model they pay for themselves.

- You enter your own API key for an AI provider. Portlight ships with no key and has no account of its own.
- Example questions: "What is this app and why would it connect to this server?", "Is anything unusual online right now?", "Summarise what went online today."
- An **Explain** button on a new-app alert, an app or a connection, which asks the question for you.
- Rules that keep the "everything stays on your PC" promise honest:
  - Nothing is sent unless you ask a question. No background analysis.
  - Before sending, Portlight shows exactly what will leave the PC (app names, server names, counts) and you can remove items.
  - The key is kept in Windows Credential Manager, not in a settings file.
  - The README's "Your data" section changes in the same release.
- The provider sits behind one small interface, so more than one can be supported.
- Most useful once history (Milestone 3) exists, because "today" and "unusual" need a record to compare against.

### Later

- Linux, then macOS, by adding a module to `portlight-platform`.
- Code signing. Until the installer is signed, Windows SmartScreen warns on first run, and some antivirus tools are wary of unsigned network software.

## Open questions

- **Service or elevated app?** Milestone 2 needs administrator rights. A service is more work but keeps the window unprivileged, which is safer. Current plan: service.
- **Blocking a domain reliably** may need inspecting DNS traffic directly, which needs a driver. Revisit after the simple version ships.
- **Trademark and domain** for the name "Portlight" have not been checked. Do this before promoting the project.
