import { useEffect, useRef } from "react";

interface Hotkeys {
  onSearch(): void;
  onTogglePause(): void;
  onEscape(): void;
}

/** Ctrl+K: search. Space: pause. Esc: back to the overview. */
export function useHotkeys(handlers: Hotkeys) {
  // Keep the latest handlers without re-attaching the listener every render.
  const latest = useRef(handlers);
  latest.current = handlers;

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement | null;
      const typing = target?.tagName === "INPUT" || target?.tagName === "TEXTAREA";

      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "k") {
        event.preventDefault();
        latest.current.onSearch();
      } else if (event.key === "Escape") {
        latest.current.onEscape();
      } else if (event.key === " " && !typing && target?.tagName !== "BUTTON") {
        event.preventDefault();
        latest.current.onTogglePause();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);
}
