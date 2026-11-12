# Architecture

How Portlight's code is organised, and how data moves from Windows to the screen.

## The layers

Portlight is four layers, plus a small service that runs beside it. Each layer only knows about the layers below it.

```
┌──────────────────────────────────────────────────────────────┐
│  UI            src/              React + TypeScript          │
│                draws what it is given; knows no networking   │
├──────────────────────────────────────────────────────────────┤
│  Shell         src-tauri/        the desktop app (Tauri)     │
│                window, tray, notifications, capture, timers  │
├──────────────────────────────────────────────────────────────┤
│  Core          crates/core/      the rules, pure Rust        │
│  Packets       crates/packets/   reading packets, pure Rust  │
│  Owners        crates/geo/       who owns an IP, pure Rust   │
├──────────────────────────────────────────────────────────────┤
│  Platform      crates/platform/  operating-system calls      │
│                tables, events, pipe, Npcap, service checks   │
└──────────────────────────────────────────────────────────────┘

  Collector     crates/collector/  portlight-service.exe, a Windows
                service that reads network events with administrator
                rights and streams them to the shell over a named pipe

  MCP bridge    crates/mcp/        portlight-mcp.exe, started by AI
                assistant apps; speaks MCP on stdio and asks the window
                over a per-user pipe
```

| Layer | Crate or folder | Knows about | Must not contain |
|---|---|---|---|
| Platform | `crates/platform` (`portlight-platform`) | Windows APIs | Rules, UI |
| Core | `crates/core` (`portlight-core`) | Connections, apps, events, readings, the known-apps file, the collector's message format | Windows APIs, Tauri, clocks, threads |
| Packets | `crates/packets` (`portlight-packets`) | Packet formats, display filters, pcapng | Windows APIs, Tauri, threads |
| Owners | `crates/geo` (`portlight-geo`) | The compact owner file: building it from iptoasn.com data and looking addresses up | Windows APIs, Tauri, threads |
| Collector | `crates/collector` (`portlight-collector`, binary `portlight-service`) | The service lifecycle, the event loop | Rules about apps |
| MCP bridge | `crates/mcp` (`portlight-mcp`) | The MCP protocol (both eras), forwarding tool calls | Any data or decision: the window answers |
| Shell | `src-tauri` (`portlight`) | Tauri, threads, the network (DNS lookups, update checks), capture sessions | Rules about apps or connections |
| UI | `src` | The data shapes the shell sends | Any decision about what is new or trusted |

A tiny front-end, `crates/cli` (`portlight-cli`), prints the basic feed to a console. It proves the core is usable without the window.

### Why it is split this way

- **The core and the packet reader have no operating-system code**, so their rules are tested in milliseconds (`cargo test -p portlight-core -p portlight-packets`). The core takes the current time as a parameter and never reads a clock.
- **The platform is behind one trait**, `Platform`, for the basic feed. Supporting Linux means writing it in a new module.
- **The collector is a separate program** because Windows' network events need administrator rights. Running it as a service keeps the window an ordinary, unprivileged program. The collector only collects; it sends readings and the window applies every rule, so a person's alerts and trusted apps stay theirs.
- **The UI talks to one small interface**, `Backend` in `src/lib/backend.ts`. The desktop app implements it by calling Rust. A plain browser implements it with invented data (`demo.ts`, `demoCapture.ts`), which is how the UI is developed and how the README screenshots are made.

## Project layout

```
Portlight/
├─ Cargo.toml                 Rust workspace; the version lives here
├─ package.json               web UI dependencies and scripts
├─ crates/
│  ├─ core/src/
│  │  ├─ model.rs             shared types, readings (Observation) and the Platform trait
│  │  ├─ tracker.rs           readings in, apps and events out
│  │  ├─ flows.rs             network events in, readings out (used by the collector)
│  │  ├─ wire.rs              the collector's messages (JSON lines)
│  │  ├─ assistant.rs         the MCP tool catalogue and the bridge's request/reply
│  │  └─ known_apps.rs        apps seen before, saved as JSON
│  ├─ packets/src/            dissectors per protocol, summaries, display filters, pcapng writer
│  ├─ geo/src/                owner file format, builder (bin: portlight-owners), lookups
│  ├─ mcp/src/                portlight-mcp: protocol.rs (pure, tested) and main.rs (stdio, pipe)
│  ├─ platform/src/windows/
│  │  ├─ tcp_table.rs         TCP connections and UDP ports with their process IDs
│  │  ├─ process.rs           process ID to program path and name
│  │  ├─ etw.rs               live network and DNS events (needs administrator rights)
│  │  ├─ pipe.rs              the named pipe between collector and window
│  │  ├─ service.rs           is the collector installed? run with one admin prompt
│  │  ├─ adapters.rs          network cards: friendly names and addresses
│  │  ├─ signature.rs         who signed a program (offline Authenticode check)
│  │  ├─ icon.rs              a program's icon as RGBA pixels
│  │  ├─ clock.rs             the local time zone, for history days
│  │  ├─ user_pipe.rs         a two-way pipe only this Windows user can open
│  │  ├─ startup.rs           start at sign-in (the Run key)
│  │  └─ npcap.rs             packet capture through Npcap, loaded at run time
│  ├─ collector/src/
│  │  ├─ main.rs              service entry point and commands
│  │  ├─ collect.rs           events → readings → every connected window
│  │  └─ setup.rs             install to Program Files, register, remove
│  └─ cli/src/main.rs         console front-end
├─ src-tauri/
│  ├─ tauri.conf.json         window, security policy, app identifier
│  ├─ capabilities/           what the window may ask Rust to do
│  └─ src/
│     ├─ lib.rs               start-up wiring, command-line flags
│     ├─ state.rs             state shared between threads
│     ├─ monitor.rs           the half-second loop; basic or detailed feed
│     ├─ collector.rs         the collector's feed; install and remove it
│     ├─ capture.rs           capture sessions: record, match to apps, filter, save
│     ├─ identity.rs          background signature checks; icons as PNG data URLs
│     ├─ owners.rs            server owner lookups from owners.bin
│     ├─ history.rs           daily history in SQLite
│     ├─ export.rs            live view as CSV
│     ├─ assistant.rs         answers AI assistants: tools, permission prompts, log, app setup
│     ├─ dns.rs               background reverse lookups
│     ├─ commands.rs          calls the window can make
│     ├─ tray.rs              tray icon and menu
│     ├─ settings.rs          settings.json
│     └─ update.rs            self-update from GitHub Releases
├─ src/
│  ├─ App.tsx                 page layout, live view or capture page
│  ├─ components/             Sidebar, Topbar, AppList, LiveMap, DetailsPanel, FlowTable, capture/...
│  ├─ hooks/                  useMonitor, useCapture, useCollector, useUpdates, useTheme, useHotkeys
│  ├─ lib/                    backend adapter, demo data, types, text helpers
│  └─ styles/                 tokens.css (colours, sizes), app.css (components)
├─ scripts/update-owners.ps1   downloads iptoasn.com data and builds owners.bin
├─ installer/                 Inno Setup script and local build script
├─ .github/workflows/         CI and release pipelines
└─ docs/
```

## How data flows

There are two sources. The monitor loop uses the collector when it is running and falls back to the TCP table otherwise, trying the collector again every three seconds.

```
Basic (no admin)                       Detailed (Portlight Collector service, SYSTEM)
Windows TCP table                      Kernel-Network + DNS-Client events (ETW)
      │ every 500 ms                         │ as they happen
      │                                      ▼
      │                                FlowTable.apply()        (core)  flows, bytes, UDP idle timeout
      │                                FlowTable.reconcile_tcp()        checked against the TCP table every 2 s
      │                                FlowTable.take() every 500 ms
      │                                      │ Observation + process names + site names
      │                                      ▼
      │                                named pipe \\.\pipe\portlight-collector (JSON lines)
      ▼                                      ▼
Tracker.poll()                         Tracker.update(observation)    (core)
      └──────────────┬───────────────────────┘
                     │  new connection → "opened"; gone → "closed";
                     │  opened and closed in between → both; unknown program → new app
                     ▼
monitor loop        (shell)  saves known apps, queues reverse lookups, notifies, feeds the capture
      │  event "monitor:tick" carrying a Snapshot (source: basic | detailed)
      ▼
useMonitor hook     (UI)     merges the snapshot into React state
```

1. **Basic.** `tcp_connections()` lists open TCP connections with their process IDs; `Tracker::poll` compares with last time.
2. **Detailed.** The collector's `EventSession` reads Kernel-Network events (connect, accept, disconnect, send, receive for TCP and UDP, with process ID and bytes) and DNS-Client event 3008 (a finished lookup: name and addresses). `FlowTable` turns them into flows. UDP flows ignore the local port, so an app's many exchanges with one server are one conversation, closed after 20 quiet seconds. A connection that opened and closed between two readings is reported in `closed`, so the window still shows it and still alerts for a new app.
3. **Core.** `Tracker::update` adds byte counts per connection and per app, keeps the names apps looked up in preference to reverse lookups, and forgets process IDs once they have no connections, because Windows reuses them.
4. **Shell.** One thread runs the loop in `monitor.rs`. It holds the tracker's lock only while updating, never while notifying or talking to the window.
5. **UI.** The window asks for one full snapshot when it opens (`get_snapshot`), then follows tick events.

### The collector service

- Installed by `portlight-service install`, which the window runs through one Windows administrator prompt (`ShellExecuteEx` with `runas`). It copies itself to `C:\Program Files\Portlight`, registers the service `PortlightCollector` (automatic start, LocalSystem, restart on failure) and starts it. `uninstall` reverses all of it; the Portlight uninstaller runs it when the service exists.
- The pipe only accepts local clients, gives read access only to people signed in at this PC, and the window checks the server runs in session 0 (a service), so another program cannot pose as the collector.
- First message: `Hello { version, protocol, names }`. Then a `Reading` twice a second. A window that falls far behind is disconnected and reconnects.
- Errors go to `C:\ProgramData\Portlight\collector.log`. `portlight-service console` runs the same loop in a console for debugging (as administrator).

### App identity, owners and history

- **Signatures.** When the tracker meets a program it has never seen, outside the learning period, it checks the signature straight away (the answer decides whether to alert). Every other program is checked once in the background by `identity.rs`. Checks never go online. The publisher is saved in `known-apps.json`.
- **Owners.** Every address seen for the first time is looked up in `owners.bin` (loaded into memory once; each lookup is a binary search, well under a microsecond). Private and link-local addresses are "Your home network". Owners reach the window on each `ConnView`.
- **History.** Each update reports `activity`: connections opened and bytes moved per app and server. `history.rs` adds these up per local day in memory and writes them to SQLite every 15 seconds (and when Portlight quits), pruning days past the limit every few hours.
- **Icons** are read on request (`app_icon`), converted to PNG, cached per app, and sent as `data:` URLs, which the content security policy allows.

### What counts as a new app

- An app is identified by the lower-cased path of its program file.
- A program at a new path whose publisher signature and file name match a known app is that app updated: it keeps its trust and raises no alert.
- `KnownApps` is the list of every app seen online, saved to `known-apps.json`.
- On the very first run the list starts a two-minute **learning period**: apps seen then are recorded as trusted, with no alert.
- After that, an app not in the list is recorded as untrusted and reported as new, once. Trusting it updates the list.
- Programs nobody can name are grouped as one app, "Windows system services". With the collector running, almost every program can be named.

These rules are in `crates/core/src/tracker.rs` and each has a test.

## Packet capture

`src-tauri/src/capture.rs`, using Npcap through `portlight-platform::npcap` and the reader in `portlight-packets`.

- **Npcap** is loaded at run time from `System32\Npcap\wpcap.dll`, so Portlight runs without it. Captures are never promiscuous: only this PC's own traffic.
- **Targets.** One app, one server address, one site name, one connection, or everything. The target becomes a capture filter for the driver where possible (`host 1.2.3.4`, `tcp and host … and port …`), plus the person's own optional filter, so unwanted packets are dropped early.
- **Matching packets to apps.** While a capture runs, the monitor loop copies a port-to-app table into the capture: TCP local ports from the tracker's connections, UDP ports from the UDP table, kept 5 s after they close. A packet whose port is not known yet waits up to 2 s for an app capture. Site-name captures match the addresses that name resolves to, and the name's own lookups.
- **Storage.** Raw packets in memory, up to 200,000 packets or 512 MB. Rows and breakdowns are worked out when the window asks, so names learned later show up.
- **Display filter.** `portlight_packets::filter`: a friendly subset of Wireshark's language, with plain words as search. Applied to every packet when it changes and to each new packet as it arrives.
- **Stop conditions.** Optionally stop after so many seconds, packets or megabytes, like Wireshark's automatic stop.
- **Saving.** All packets, the shown ones, the selected one or a range, as pcapng (one interface block per network card, the app name as each packet's comment), the list as CSV, or the full breakdown as readable text or JSON. Rust opens the Save dialog, so the window needs no file access.

## AI assistants (MCP)

The user-facing guide, with the tool list, is [MCP.md](MCP.md).

```
AI app (Claude Desktop, Claude Code, Gemini CLI...)
   │  starts portlight-mcp.exe, MCP JSON-RPC on stdin/stdout (one message per line)
   ▼
portlight-mcp            protocol.rs: initialize (legacy) or per-request _meta (2026-07-28),
   │                     server/discover, tools/list, tools/call → one Request
   │  \\.\pipe\portlight-assistant-<user SID>   (access list: this user only; one request per connection)
   ▼
Portlight window         assistant.rs: switch off? → polite error. Rate limit. Run the tool against
                         the tracker, history, owners and capture. start_capture → "assistant:consent"
                         event → Allow / Don't allow (2 minutes, then no) → capture with stop conditions.
                         Every request → "What was shared" (memory only) and "assistant:activity".
```

- The tool list lives once, in `crates/core/src/assistant.rs`, so the bridge and the window can't disagree.
- Tool errors are returned as tool results (`isError: true`) worded for the assistant to pass on; protocol mistakes are JSON-RPC errors.
- Each tool call runs on its own thread in the bridge, so a permission prompt doesn't block other requests.
- Setup: `assistant.rs` merges a `portlight` entry into Claude Desktop's `claude_desktop_config.json` (regular or Microsoft Store install, keeping a backup) or runs `claude mcp add --scope user`. `portlight.exe --forget-assistants`, run by the uninstaller, removes both.

## The window and Rust

The commands are in `src-tauri/src/commands.rs`:

| Command | Does |
|---|---|
| `get_snapshot` | Returns the full current picture |
| `trust_app`, `trust_all` | Acknowledge new apps |
| `update_status`, `check_for_update`, `set_auto_update`, `install_update` | The update menu |
| `collector_status`, `install_collector`, `uninstall_collector` | Detailed monitoring on and off |
| `capture_support`, `capture_start`, `capture_stop`, `capture_clear`, `capture_status` | Capture sessions |
| `capture_set_filter`, `capture_rows`, `capture_packet` | Reading a capture |
| `capture_export` | Save chosen packets (all, shown, selected, a range) as pcapng, CSV, text or JSON |
| `capture_save`, `capture_export_csv`, `open_npcap_download` | Shortcuts for the common saves, and help |
| `export_live` | The apps, open connections or connection history as CSV |
| `app_icon` | An app's icon as a PNG `data:` URL |
| `app_history`, `history_summary` | Daily history for one app, or every app's daily totals |
| `history_settings`, `set_history_settings`, `clear_history` | History on or off, how long it is kept, clearing it |
| `get_settings`, `set_settings` | The Settings page: sign-in start, tray, notifications, name lookups, auto-update |
| `learn_again`, `forget_known_apps` | Trust everything now and learn for two minutes; or start over |
| `about`, `open_link` | The About section, and opening Portlight's own fixed web pages or data folder |
| `assistant_status`, `set_assistant_access`, `connect_assistant_app` | Settings › AI assistants: the switch and connecting apps |
| `assistant_answer`, `assistant_log`, `clear_assistant_log` | Permission prompts and "What was shared" |

Rust sends the window six events: `monitor:tick` (a `Snapshot`), `update:status`, `collector:status`, `capture:status`, `assistant:consent` and `assistant:activity`.

The shapes are defined once in Rust (`crates/core/src/model.rs`, `src-tauri/src/collector.rs`, `src-tauri/src/capture.rs`, `crates/packets`, `src-tauri/src/update.rs`) and mirrored by hand in `src/lib/types.ts`. Change both together.

Security settings worth knowing:

- The window loads only files bundled inside the app. The content security policy in `tauri.conf.json` forbids loading scripts, styles or data from the network.
- The window's permissions (`capabilities/default.json`) are the Tauri defaults plus notifications. It cannot read files or run programs; saving a capture goes through a command that opens the Save dialog itself.

## The UI

- **State** lives in a few places: `useMonitor` (what the backend says), `useCapture` and `useCollector`, `App.tsx` (what the person is doing: the page, selection, search, pause) and small components' own state.
- **Pages.** The live view (sidebar, map and flow, details column) and the capture page, which replaces it entirely and has its own back button.
- **Styling** is plain CSS. Every colour and size is a variable in `tokens.css`; dark mode only redefines those variables.
- **The live map** is one SVG. Isometric shapes are built from a single box primitive in `iso.tsx`. Each travelling dot is moved by the browser's Web Animations API, so React renders a dot once when it appears and not on every frame.
- **Motion** is turned off for people who ask their system for reduced motion.

## Process model

- The window is one process. Closing the window hides it; the tray icon brings it back or quits.
- A second launch does not start a second copy: it hands its command line to the first (`--quit` exits it; anything else shows the window).
- `--background` starts hidden, used by the sign-in entry.
- The collector, when turned on, is a separate Windows service process that runs whether or not the window is open.
