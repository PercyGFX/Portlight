import { useCallback, useMemo, useRef, useState } from "react";

import { AppList } from "./components/AppList";
import { DetailsPanel } from "./components/DetailsPanel";
import { FlowTable } from "./components/FlowTable";
import { Header } from "./components/Header";
import { LiveMap } from "./components/LiveMap";
import { Toasts } from "./components/Toasts";
import { useHotkeys } from "./hooks/useHotkeys";
import { useMonitor } from "./hooks/useMonitor";
import { useTheme } from "./hooks/useTheme";
import { useUpdates } from "./hooks/useUpdates";
import { backend } from "./lib/backend";
import { countdown } from "./lib/format";
import type { FlowEvent } from "./lib/types";

export default function App() {
  const monitor = useMonitor();
  const [theme, setTheme] = useTheme();
  const updates = useUpdates();
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  // While paused, the flow table shows the events as they were at that moment.
  const [frozen, setFrozen] = useState<FlowEvent[] | null>(null);
  // Events up to this sequence number were cleared from the table.
  const [clearedUpTo, setClearedUpTo] = useState(0);
  const searchRef = useRef<HTMLInputElement>(null);

  const paused = frozen !== null;
  const togglePause = useCallback(() => {
    setFrozen((current) => (current === null ? monitor.events : null));
  }, [monitor.events]);

  useHotkeys({
    onSearch: () => searchRef.current?.focus(),
    onTogglePause: togglePause,
    onEscape: () => {
      if (document.activeElement === searchRef.current && query) setQuery("");
      else setSelectedId(null);
    },
  });

  const needle = query.trim().toLowerCase();
  const selected = monitor.apps.find((app) => app.id === selectedId) ?? null;
  const newApps = useMemo(() => monitor.apps.filter((app) => !app.trusted), [monitor.apps]);

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

  return (
    <div className="shell">
      <Header
        appsOnline={monitor.apps.filter((app) => app.connections > 0).length}
        connections={monitor.connections.length}
        query={query}
        searchRef={searchRef}
        theme={theme}
        newApps={newApps}
        updates={updates}
        demo={backend.demo}
        onQuery={setQuery}
        onTheme={setTheme}
        onSelect={setSelectedId}
        onTrust={monitor.trustApp}
        onTrustAll={monitor.trustAll}
      />

      {monitor.learningSecondsLeft > 0 && (
        <p className="banner" role="status">
          <strong>Learning.</strong> Portlight is noting which apps are already online on this PC. Alerts for new
          apps start in {countdown(monitor.learningSecondsLeft)}.
        </p>
      )}

      <main className="workspace">
        <AppList apps={listedApps} selectedId={selectedId} filtered={needle !== ""} onSelect={setSelectedId} />
        <LiveMap
          ready={monitor.ready}
          apps={monitor.apps}
          connections={monitor.connections}
          events={monitor.events}
          hosts={monitor.hosts}
          selectedId={selectedId}
          paused={paused}
          onSelect={setSelectedId}
        />
        <DetailsPanel
          app={selected}
          apps={monitor.apps}
          connections={monitor.connections}
          hosts={monitor.hosts}
          now={monitor.now}
          onTrust={monitor.trustApp}
          onBack={() => setSelectedId(null)}
        />
      </main>

      <FlowTable
        events={shownEvents}
        hosts={monitor.hosts}
        scope={selected?.name ?? null}
        paused={paused}
        onTogglePause={togglePause}
        onClear={() => setClearedUpTo(monitor.events[0]?.seq ?? 0)}
        onSelectApp={setSelectedId}
      />

      <Toasts ready={monitor.ready} newApps={newApps} onSelect={setSelectedId} />
    </div>
  );
}
