# Architecture

How Portlight's code is organised, and how data moves from Windows to the screen.

## The layers

Portlight is four layers. Each one only knows about the layers below it.

```
┌──────────────────────────────────────────────────────────────┐
│  UI            src/              React + TypeScript          │
│                draws what it is given; knows no networking   │
├──────────────────────────────────────────────────────────────┤
│  Shell         src-tauri/        the desktop app (Tauri)     │
│                window, tray, notifications, updater, timers  │
├──────────────────────────────────────────────────────────────┤
│  Core          crates/core/      the rules, pure Rust        │
│                apps, events, what counts as "new"            │
├──────────────────────────────────────────────────────────────┤
│  Platform      crates/platform/  operating-system calls      │
│                "which process owns which connection?"        │
└──────────────────────────────────────────────────────────────┘
```

| Layer | Crate or folder | Knows about | Must not contain |
|---|---|---|---|
| Platform | `crates/platform` (`portlight-platform`) | Windows APIs | Rules, UI, threads |
| Core | `crates/core` (`portlight-core`) | Connections, apps, events, the known-apps file | Windows APIs, Tauri, clocks, threads |
| Shell | `src-tauri` (`portlight`) | Tauri, threads, the network (DNS lookups, update checks) | Rules about apps or connections |
| UI | `src` | The data shapes the core sends | Any decision about what is new or trusted |

A second, tiny front-end, `crates/cli` (`portlight-cli`), prints the same feed to a console. It exists to prove the core is usable without the window and to debug without one.

### Why it is split this way

- **The core has no operating-system code**, so its rules are tested with a fake system in milliseconds (`cargo test -p portlight-core`). The core takes the current time as a parameter and never reads a clock, which makes tests repeatable.
- **The platform is behind one trait**, `Platform`, with two methods. Supporting Linux means writing those two methods in a new module; the core, shell and UI do not change.
- **The UI talks to one small interface**, `Backend` in `src/lib/backend.ts`. The desktop app implements it by calling Rust. A plain browser implements it with invented data, which is how the UI is developed and how the README screenshots are made.

## Project layout

```
Portlight/
├─ Cargo.toml                 Rust workspace; the version lives here
├─ package.json               web UI dependencies and scripts
├─ crates/
│  ├─ core/src/
│  │  ├─ model.rs             shared types and the Platform trait
│  │  ├─ tracker.rs           snapshots in, apps and events out
│  │  └─ known_apps.rs        apps seen before, saved as JSON
│  ├─ platform/src/
│  │  └─ windows/
│  │     ├─ tcp_table.rs      reads the TCP connection table
│  │     └─ process.rs        process ID to program path and name
│  └─ cli/src/main.rs         console front-end
├─ src-tauri/
│  ├─ tauri.conf.json         window, security policy, app identifier
│  ├─ capabilities/           what the window may ask Rust to do
│  └─ src/
│     ├─ lib.rs               start-up wiring, command-line flags
│     ├─ state.rs             state shared between threads
│     ├─ monitor.rs           the half-second loop
│     ├─ dns.rs               background name lookups
│     ├─ commands.rs          calls the window can make
│     ├─ tray.rs              tray icon and menu
│     ├─ settings.rs          settings.json
│     └─ update.rs            self-update from GitHub Releases
├─ src/
│  ├─ App.tsx                 page layout and page-level state
│  ├─ components/             Sidebar, Topbar, AppList, LiveMap, DetailsPanel, FlowTable...
│  ├─ hooks/                  useMonitor, useUpdates, useTheme, useHotkeys
│  ├─ lib/                    backend adapter, demo data, types, text helpers
│  └─ styles/                 tokens.css (colours, sizes), app.css (components)
├─ installer/                 Inno Setup script and local build script
├─ .github/workflows/         CI and release pipelines
└─ docs/
```

## How data flows

```
Windows TCP table
      │  every 500 ms             (platform)
      ▼
Tracker.update()                  (core)   compares with last time:
      │                                    new connection  → "opened" event
      │                                    gone connection → "closed" event
      │                                    unknown program → new app
      ▼
monitor loop                      (shell)  saves known apps if changed
      │                                    queues new addresses for name lookup
      │                                    shows a Windows notification per new app
      │  event "monitor:tick" carrying a Snapshot
      ▼
useMonitor hook                   (UI)     merges the snapshot into React state
      ▼
Sidebar (AppList) · Topbar · LiveMap · FlowTable · DetailsPanel
```

1. **Platform.** `tcp_connections()` returns every open TCP connection with its process ID. `process_info(pid)` returns the program's path and friendly name.
2. **Core.** `Tracker::update` drops connections to this same machine, then compares what is open now with what was open last time. It keeps a map from process ID to app, forgotten as soon as that process has no connections, because Windows reuses process IDs.
3. **Shell.** One thread runs the loop in `monitor.rs`. It holds the tracker's lock only while updating, never while notifying or talking to the window. Four worker threads in `dns.rs` turn addresses into names without slowing the loop.
4. **UI.** The window asks for one full snapshot when it opens (`get_snapshot`), then follows tick events. Each event carries only the events that are new since the last tick; the hook adds them by sequence number, so nothing is shown twice whichever arrives first.

### What counts as a new app

- An app is identified by the lower-cased path of its program file.
- `KnownApps` is the list of every app seen online, saved to `known-apps.json`.
- On the very first run the list starts a two-minute **learning period**: apps seen then are recorded as trusted, with no alert.
- After that, an app not in the list is recorded as untrusted and reported as new, once. Trusting it updates the list.
- Programs Windows will not name are grouped as one app, "Windows system services".

These rules are in `crates/core/src/tracker.rs` and each has a test.

## The window and Rust

The window can call seven commands, all in `src-tauri/src/commands.rs`:

| Command | Does |
|---|---|
| `get_snapshot` | Returns the full current picture |
| `trust_app`, `trust_all` | Acknowledge new apps |
| `update_status`, `check_for_update`, `set_auto_update`, `install_update` | The update menu |

Rust sends the window two events: `monitor:tick` (a `Snapshot`) and `update:status`.

The shapes are defined once in Rust (`crates/core/src/model.rs`, `src-tauri/src/update.rs`) and mirrored by hand in `src/lib/types.ts`. Change both together.

Security settings worth knowing:

- The window loads only files bundled inside the app. The content security policy in `tauri.conf.json` forbids loading scripts, styles or data from the network.
- The window's permissions (`capabilities/default.json`) are the Tauri defaults plus notifications. It cannot read files or run programs.

## The UI

- **State** lives in three places: `useMonitor` (what the backend says), `App.tsx` (what the user is doing: selection, search, pause) and small components' own state (which tab, which menu is open).
- **Layout** is three columns: a sidebar (name, search, app list, version and theme), a middle stage (totals, the map, the connection flow) and a details column.
- **Styling** is plain CSS. Every colour and size is a variable in `tokens.css`; dark mode only redefines those variables.
- **The live map** is one SVG. Isometric shapes are built from a single box primitive in `iso.tsx`. Each travelling dot is moved by the browser's Web Animations API, so React renders a dot once when it appears and not on every frame. At most ten dots start per update, so a burst of connections stays readable.
- **Motion** is turned off for people who ask their system for reduced motion.

## Process model

- One process. Closing the window hides it; the tray icon brings it back or quits.
- A second launch does not start a second copy: it hands its command line to the first (`--quit` exits it; anything else shows the window).
- `--background` starts hidden, used by the sign-in entry.

Milestone 2 in [PLAN.md](PLAN.md) changes this: data collection moves to a small Windows service, and this process becomes its viewer.
