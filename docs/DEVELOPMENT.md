# Development

Setting up, everyday commands, and how to cut a release.

## What you need

| Tool | Version | Get it |
|---|---|---|
| Windows | 10 (1809+) or 11, 64-bit | |
| Rust | stable, 1.85 or newer | https://rustup.rs |
| Visual Studio Build Tools | "Desktop development with C++" | Needed by Rust on Windows |
| Node.js | 22 or newer | https://nodejs.org |
| Inno Setup 6 | only to build the installer | `winget install JRSoftware.InnoSetup` |

WebView2 is already part of Windows 11.

## First run

```powershell
git clone https://github.com/PercyGFX/Portlight.git
cd Portlight
npm install
npm run tauri dev
```

The first build compiles a few hundred Rust crates and takes several minutes. Later builds take seconds.

## Everyday commands

| Command | What it does |
|---|---|
| `npm run tauri dev` | The desktop app with live reload for both the UI and Rust |
| `npm run dev` | Only the UI, in a browser at http://localhost:1420, with demo data |
| `cargo test --workspace` | All Rust tests |
| `cargo test -p portlight-core` | Only the core rules; runs in a second |
| `cargo run -p portlight-cli` | The basic live feed in a console (add `-- --seconds 10` to stop by itself) |
| `cargo test -p portlight-packets` | Only the packet reader and display filters |
| `target\debug\portlight-service.exe console` | The collector's event feed in a console. Run from an administrator prompt |
| `npm run build` | Type-check and build the UI |
| `npm run tauri -- build --no-bundle` | The release exe at `target\release\portlight.exe` |
| `installer\build.ps1` | Tests, release exe and installer in `installer\Output` |

### Working on the UI

`npm run dev` needs no Rust at all. In a browser the UI uses the invented data in `src/lib/demo.ts`, which plays the same sequence every time, including a new app going online after about six seconds. The README screenshots are taken this way, so they never show anyone's real connections.

### Working on the rules

Put the rule in `crates/core` and write its test first, using the fake system at the bottom of `tracker.rs`. If a rule needs the time, take it as a parameter.

### Working on detailed monitoring

`npm run tauri dev` builds the whole workspace, so `target\debug\portlight-service.exe` sits next to the app and **Turn on detailed monitoring** installs that build (one prompt). After changing the collector, choose **Update the background service**, or run `portlight-service.exe install` again from an administrator prompt. To see raw readings without the window, stop the service (`sc stop PortlightCollector`) and run `portlight-service.exe console` as administrator.

### Working on packet capture

Install [Npcap](https://npcap.com/#download) with its default options. In a browser, `npm run dev` plays an invented capture (`src/lib/demoCapture.ts`), which is how the capture page is developed and screenshotted.

### Working on server owners

The owner list is not in git (it is about 10 MB and changes hourly upstream). Run `scripts\update-owners.ps1` once: it downloads the iptoasn.com data and writes `owners.bin` into `target\debug` and `target\release`, next to the exe, where Portlight looks for it. Without it, owners are simply not shown. `cargo run -p portlight-geo --bin portlight-owners -- lookup target\debug\owners.bin 1.1.1.1` checks it.

### Where Portlight keeps its files

`%APPDATA%\com.portlight.app\known-apps.json`, `settings.json` and `history.sqlite`. Delete `known-apps.json` to see the first-run learning period again.

## Conventions

- **Layers.** Read [ARCHITECTURE.md](ARCHITECTURE.md) before adding a file, and put code in the lowest layer that can hold it.
- **Types cross the Rust and TypeScript boundary by hand.** When you change `crates/core/src/model.rs`, `UpdateStatus`, `CollectorStatus` (`src-tauri/src/collector.rs`), the capture shapes (`src-tauri/src/capture.rs`) or `Summary` / `Layer` / `Field` (`crates/packets`), change `src/lib/types.ts` in the same commit.
- **The collector's messages** (`crates/core/src/wire.rs`) are read by windows of other versions. Change them only together with `wire::PROTOCOL`.
- **iPhone-simple.** Every feature gets a one-click path first. Technical options go behind "More options".
- **Colours and sizes** go in `src/styles/tokens.css`, for both themes. No colour values in components.
- **Words on screen** are for people new to networking. Prefer "secure web traffic" to "TLS over TCP 443".
- **Unsafe Rust** stays inside `crates/platform`, each block with a `SAFETY` comment.

## Pipelines

Both live in `.github/workflows`.

- **CI** (`ci.yml`) runs on every push to `main` and every pull request: installs web dependencies, builds the UI, builds the Rust workspace, runs the tests.
- **Release** (`release.yml`) runs when a tag starting with `v` is pushed: tests, builds the exe and the collector service (`portlight-service.exe`), builds the installer, and publishes a GitHub Release with the installer attached and the notes taken from `CHANGELOG.md`.

## Cutting a release

1. Set the new version in **one place**: `version` under `[workspace.package]` in the root `Cargo.toml`. Also update `version` in `package.json` to match, for tidiness.
2. Add a `## [x.y.z] - date` section to `CHANGELOG.md`. The release pipeline fails without it.
3. Commit, then tag and push:

   ```powershell
   git tag v0.3.0
   git push origin main v0.3.0
   ```

4. The pipeline publishes `Portlight-Setup-0.3.0.exe`. Installed copies find it within 12 hours and update themselves.

The tag must match the version in `Cargo.toml`; the pipeline checks.

### How self-update works

`src-tauri/src/update.rs`, the same flow as SpeedMeter:

1. Ask the GitHub API for the latest release.
2. If its tag is newer than this build, find the asset named `Portlight-Setup-*.exe`.
3. Download it over HTTPS to the temp folder and compare its SHA-256 with the digest GitHub publishes for the asset. A mismatch discards the file.
4. Run it with `/VERYSILENT`. The installer asks the running Portlight to quit, replaces the files and starts it again.

Only a copy installed in `%LOCALAPPDATA%\Programs\Portlight` updates itself, so a development build never replaces itself.

## Not done yet

- **Code signing.** The exe and installer are unsigned, so SmartScreen warns on first run. Signing needs a certificate; the pipeline has no signing step yet.
- **Front-end tests.** The UI is checked by the TypeScript compiler only.
- **Automated tests of the live parts.** The event session, the service install and Npcap capture need administrator rights or a driver, so they are tried by hand (see "Working on detailed monitoring" and "Working on packet capture"). Their logic is tested through the core (`flows.rs`) and `capture.rs` with built frames.
