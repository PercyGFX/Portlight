// Asks, over any page, whether an AI assistant may start a capture. One prompt at a time; no answer in time means no.

import { useEffect, useRef, useState } from "react";

import type { Assistant } from "../hooks/useAssistant";
import { countdown } from "../lib/format";

/** How long the assistant waits for an answer. */
const WAIT_MS = 120_000;
const NOTE_MS = 4500;

export function ConsentPrompt({ assistant }: { assistant: Assistant }) {
  const { consents, answer, expire } = assistant;
  const current = consents[0] ?? null;
  const [now, setNow] = useState(() => Date.now());
  const [note, setNote] = useState<string | null>(null);
  const cardRef = useRef<HTMLDivElement>(null);
  const noteTimer = useRef(0);

  useEffect(() => () => window.clearTimeout(noteTimer.current), []);

  // Tick the countdown, and let a prompt that ran out of time go.
  useEffect(() => {
    if (!current) return;
    setNow(Date.now());
    const timer = window.setInterval(() => {
      const time = Date.now();
      setNow(time);
      if (time - current.receivedAt >= WAIT_MS) expire(current.id);
    }, 1000);
    return () => window.clearInterval(timer);
  }, [current, expire]);

  // Focus the card, not a button, so a stray Enter does not answer; give focus back afterwards.
  const currentId = current?.id ?? null;
  useEffect(() => {
    if (currentId === null) return;
    const before = document.activeElement as HTMLElement | null;
    cardRef.current?.focus();
    return () => before?.focus?.();
  }, [currentId]);

  const reply = (allow: boolean) => {
    if (!current) return;
    answer(current.id, allow);
    if (allow) {
      setNote(`Capture started for ${current.client}`);
      window.clearTimeout(noteTimer.current);
      noteTimer.current = window.setTimeout(() => setNote(null), NOTE_MS);
    }
  };

  // While a prompt is open it has the keyboard: Esc means Don't allow, and page shortcuts wait.
  const replyRef = useRef(reply);
  replyRef.current = reply;
  useEffect(() => {
    if (!current) return;
    const onKey = (event: KeyboardEvent) => {
      event.stopPropagation();
      if (event.key === "Escape") {
        event.preventDefault();
        replyRef.current(false);
      } else if (event.key === "Tab") {
        // Keep Tab inside the card.
        const buttons = Array.from(cardRef.current?.querySelectorAll("button") ?? []);
        if (!buttons.length) return;
        const at = buttons.indexOf(document.activeElement as HTMLButtonElement);
        const next = event.shiftKey ? (at <= 0 ? buttons.length - 1 : at - 1) : (at + 1) % buttons.length;
        event.preventDefault();
        buttons[next].focus();
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [current]);

  const left = current ? Math.max(0, Math.ceil((current.receivedAt + WAIT_MS - now) / 1000)) : 0;
  const waiting = consents.length - 1;

  return (
    <>
      {current && (
        <div className="consent-layer">
          <div
            key={current.id}
            ref={cardRef}
            className="consent"
            role="alertdialog"
            aria-modal="true"
            aria-labelledby="consent-title"
            aria-describedby="consent-body"
            tabIndex={-1}
          >
            <span className="consent-kicker">AI assistant</span>
            <h2 id="consent-title">{current.client} wants to start a capture</h2>
            <div id="consent-body">
              <p className="consent-action">{current.action}</p>
              {current.detail && <p className="consent-detail">{current.detail}</p>}
            </div>
            <div className="consent-foot">
              <span className="consent-timer">
                Closes in {countdown(left)}, counts as Don't allow
                {waiting > 0 && ` · ${waiting} more waiting`}
              </span>
              <span className="settings-actions">
                <button className="button" onClick={() => reply(false)}>
                  Don't allow
                </button>
                <button className="button button-solid" onClick={() => reply(true)}>
                  Allow
                </button>
              </span>
            </div>
          </div>
        </div>
      )}
      {note && (
        <div className="consent-note" role="status">
          <span className="live-dot" />
          {note}
        </div>
      )}
    </>
  );
}
