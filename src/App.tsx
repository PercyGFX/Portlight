import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { AppList } from "./components/AppList";
import { CapturePage } from "./components/capture/CapturePage";
import { CollectorNotice } from "./components/CollectorNotice";
import { DetailsPanel } from "./components/DetailsPanel";
import { FlowTable } from "./components/FlowTable";
import { LiveMap } from "./components/LiveMap";
import { SettingsPage, type SettingsSection } from "./components/settings/SettingsPage";
import { Sidebar } from "./components/Sidebar";
import { Toasts } from "./components/Toasts";
import { Topbar } from "./components/Topbar";
import { useCapture } from "./hooks/useCapture";
import { useCollector } from "./hooks/useCollector";
import { useHotkeys } from "./hooks/useHotkeys";
import { useMonitor } from "./hooks/useMonitor";
import { useTheme } from "./hooks/useTheme";
import { useUpdates } from "./hooks/useUpdates";
import { backend } from "./lib/backend";
import { countdown } from "./lib/format";
import { EVERYTHING } from "./lib/targets";
import type { CaptureTarget, FlowEvent } from "./lib/types";

type View = "monitor" | "capture" | "settings";

/** How long the old page takes to step out before the new one steps in. */
const LEAVE_MS = 170;

// Two pages: the live view (a sidebar of apps, a middle stage with the map above
// the connection flow, and a details column on the right) and the packet capture
// page, which replaces the whole window while it is open.
export default function App() {
  const monitor = useMonitor();
  const theme = useTheme();
  const [settingsSection, setSettingsSection] = useState<SettingsSection>("general");
  const updates = useUpdates();
  const collector = useCollector();
  const capture = useCapture();
  const [view, setView] = useState<View>("monitor");
  const [leaving, setLeaving] = useState(false);
  // False until the first page change, so the window does not animate on start-up.
  const [moved, setMoved] = useState(false);
  const [captureTarget, setCaptureTarget] = useState<CaptureTarget>(EVERYTHING);
  // Set when an entry point (an app, a server, a connection) should start recording at once.
  const [autoStart, setAutoStart] = useState(false);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  // While paused, the flow table shows the events as they were at that moment.
  const [frozen, setFrozen] = useState<FlowEvent[] | null>(null);
  // Events up to this sequence number were cleared from the table.
  const [clearedUpTo, setClearedUpTo] = useState(0);
  const searchRef = useRef<HTMLInputElement>(null);
  const leaveTimer = useRef(0);

  useEffect(() => () => window.clearTimeout(leaveTimer.current), []);

  const go = useCallback(
    (next: View) => {
      if (next === view || leaving) return;
      setMoved(true);
      if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) {
        setView(next);
        return;
      }
      setLeaving(true);
      leaveTimer.current = window.setTimeout(() => {
        setView(next);
        setLeaving(false);
      }, LEAVE_MS);
    },
    [view, leaving],
  );

  /** Opens the capture page. With a target it starts recording straight away; without one it
      offers "Everything" (or shows what is already recording) and waits for Start. */
  const openCapture = useCallback(
    (target?: CaptureTarget) => {
      if (target) {
        setCaptureTarget(target);
        setAutoStart(true);
      } else if (capture.status.state !== "running") {
        setCaptureTarget(EVERYTHING);
      }
      go("capture");
    },
    [capture.status.state, go],
  );

  const clearAutoStart = useCallback(() => setAutoStart(false), []);

  const paused = frozen !== null;
  const togglePause = useCallback(() => {
    setFrozen((current) => (current === null ? monitor.events : null));
  }, [monitor.events]);

  useHotkeys({
    onSearch: () => view === "monitor" && searchRef.current?.focus(),
    onTogglePause: () => view === "monitor" && togglePause(),
    onEscape: () => {
      if (view !== "monitor") return;
      if (document.activeElement === searchRef.current && query) setQuery("");
      else setSelectedId(null);
    },
  });

  const needle = query.trim().toLowerCase();
  const selected = monitor.apps.find((app) => app.id === selectedId) ?? null;
  const newApps = useMemo(() => monitor.apps.filter((app) => !app.trusted), [monitor.apps]);
  const serverCount = useMemo(
    () => new Set(monitor.connections.map((conn) => conn.remoteIp)).size,
    [monitor.connections],
  );

  const listedApps = useMemo(
    () =>
      needle
        ? monitor.apps.filter((app) => `${app.name} ${app.exe}`.toLowerCase().includes(needle))
        : monitor.apps,
    [monitor.apps, needle],
  );

  const shownEvents = useMemo(() => {
    const source = frozen ?? monitor.events;
    return source.filter((event) => {
      if (event.seq <= clearedUpTo) return false;
      if (selectedId && event.appId !== selectedId) return false;
      if (!needle) return true;
      const host = monitor.hosts.get(event.remoteIp) ?? event.host ?? "";
      return `${event.appName} ${host} ${event.remoteIp}`.toLowerCase().includes(needle);
    });
  }, [frozen, monitor.events, monitor.hosts, clearedUpTo, selectedId, needle]);

  const openSettings = useCallback(
    (section?: SettingsSection) => {
      if (section) setSettingsSection(section);
      go("settings");
    },
    [go],
  );

  if (view === "settings") {
    return (
      <SettingsPage
        section={settingsSection}
        theme={theme}
        updates={updates}
        collector={collector}
        capture={capture}
        leaving={leaving}
        onSection={setSettingsSection}
        onBack={() => go("monitor")}
      />
    );
  }

  if (view === "capture") {
    return (
      <CapturePage
        capture={capture}
        target={captureTarget}
        apps={monitor.apps}
        connections={monitor.connections}
        hosts={monitor.hosts}
        owners={monitor.owners}
        leaving={leaving}
        autoStart={autoStart}
        onAutoStarted={clearAutoStart}
        onTarget={setCaptureTarget}
        onBack={() => go("monitor")}
      />
    );
  }

  return (
    <div className={`shell ${leaving ? "is-leaving" : moved ? "is-entering" : ""}`}>
      <Sidebar
        query={query}
        searchRef={searchRef}
        theme={theme.theme}
        updates={updates}
        capture={capture.status}
        onOpenCapture={() => openCapture()}
        onOpenSettings={openSettings}
        onQuery={setQuery}
        onTheme={theme.setChoice}
      >
        <AppList apps={listedApps} selectedId={selectedId} filtered={needle !== ""} onSelect={setSelectedId} />
      </Sidebar>

      <main className="stage">
        <Topbar
          appsOnline={monitor.apps.filter((app) => app.connections > 0).length}
          connections={monitor.connections.length}
          servers={serverCount}
          paused={paused}
          newApps={newApps}
          demo={backend.demo}
          source={monitor.source}
          onSelect={setSelectedId}
          onTrust={monitor.trustApp}
          onTrustAll={monitor.trustAll}
        />

        <CollectorNotice collector={collector} source={monitor.source} />

        {monitor.learningSecondsLeft > 0 && (
          <p className="banner" role="status">
            <strong>Learning.</strong> Portlight is noting which apps are already online on this PC. Alerts for new
            apps start in {countdown(monitor.learningSecondsLeft)}.
          </p>
        )}

        <LiveMap
          ready={monitor.ready}
          apps={monitor.apps}
          connections={monitor.connections}
          events={monitor.events}
          hosts={monitor.hosts}
          selectedId={selectedId}
          paused={paused}
          onSelect={setSelectedId}
          onCapture={openCapture}
        />

        <FlowTable
          events={shownEvents}
          hosts={monitor.hosts}
          owners={monitor.owners}
          scope={selected?.name ?? null}
          paused={paused}
          onTogglePause={togglePause}
          onClear={() => setClearedUpTo(monitor.events[0]?.seq ?? 0)}
          onSelectApp={setSelectedId}
        />
      </main>

      <DetailsPanel
        app={selected}
        apps={monitor.apps}
        connections={monitor.connections}
        hosts={monitor.hosts}
        owners={monitor.owners}
        now={monitor.now}
        source={monitor.source}
        onTrust={monitor.trustApp}
        onBack={() => setSelectedId(null)}
        onSelect={setSelectedId}
        onCapture={openCapture}
      />

      <Toasts ready={monitor.ready} newApps={newApps} onSelect={setSelectedId} />
    </div>
  );
}
