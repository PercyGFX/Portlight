// A number that rolls smoothly to its new value instead of jumping.

import { useEffect, useRef, useState } from "react";

const DURATION_MS = 260;
const reducedMotion = () => window.matchMedia("(prefers-reduced-motion: reduce)").matches;

export function Ticker({ value, format }: { value: number; format(value: number): string }) {
  const [shown, setShown] = useState(value);
  const current = useRef(value);
  // When the last animation frame ran; frames stop in hidden or busy windows.
  const lastFrame = useRef(0);

  useEffect(() => {
    const from = current.current;
    const stalled = performance.now() - lastFrame.current > 600;
    // Going down means a new capture: jump straight there. So do windows that are not drawing frames.
    if (value <= from || stalled || reducedMotion() || document.hidden) {
      lastFrame.current = performance.now();
      current.current = value;
      setShown(value);
      return;
    }
    let frame = 0;
    const startedAt = performance.now();
    const step = (time: number) => {
      lastFrame.current = time;
      const t = Math.min(1, (time - startedAt) / DURATION_MS);
      const next = from + (value - from) * (1 - (1 - t) ** 3);
      current.current = next;
      setShown(next);
      if (t < 1) frame = requestAnimationFrame(step);
    };
    frame = requestAnimationFrame(step);
    return () => cancelAnimationFrame(frame);
  }, [value]);

  return <>{format(shown)}</>;
}
