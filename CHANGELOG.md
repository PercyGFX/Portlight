# Changelog

All notable changes to Portlight are listed here. Versions follow
[Semantic Versioning](https://semver.org/).

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
