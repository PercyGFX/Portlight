// Under the top strip in basic mode: one line on what detailed monitoring adds, and one button to turn it on.

import { useState } from "react";

import type { Collector } from "../hooks/useCollector";
import type { Source } from "../lib/types";
import { CopyButton } from "./CopyButton";
import { ChevronDownIcon, CloseIcon } from "./icons";

const DISMISSED_KEY = "portlight.detailedNoticeDismissed";

export function CollectorNotice({ collector, source }: { collector: Collector; source: Source | null }) {
  const [dismissed, setDismissed] = useState(() => localStorage.getItem(DISMISSED_KEY) === "1");
  const [more, setMore] = useState(false);
  const { status, busy, error } = collector;
  if (!status || !source) return null;

  const outdated = status.installed && status.outdated;
  // An out-of-date service is worth saying again, even after "not now".
  if (source === "detailed" && !outdated) return null;
  if (dismissed && !outdated && !busy && !error) return null;
  if (!status.canInstall && !outdated) return null;

  const dismiss = () => {
    localStorage.setItem(DISMISSED_KEY, "1");
    setDismissed(true);
  };

  return (
    <section className="notice step-in" aria-label="Detailed monitoring">
      <div className="notice-text">
        <p>
          {outdated ? (
            <strong>The background service needs an update.</strong>
          ) : (
            <>
              <strong>See every connection and the data each app uses.</strong>{" "}
            </>
          )}
          <button className="link notice-more" aria-expanded={more} onClick={() => setMore((v) => !v)}>
            Learn more <ChevronDownIcon />
          </button>
        </p>
        {more && (
          <p className="notice-details step-in">
            {outdated
              ? "It is older than this version of Portlight. Updating asks Windows for permission once."
              : "Detailed monitoring adds very short connections, UDP traffic (calls, games, streaming), data used per app, the real site names apps ask for, and names for Windows system processes. It installs a small background service and asks Windows for permission once. You can turn it off from the version menu."}
          </p>
        )}
        {error && (
          <span className="notice-error has-copy" role="alert">
            <span>{error}</span>
            <CopyButton value={error} what="message" />
          </span>
        )}
      </div>
      <button className="button button-solid" disabled={busy !== null} onClick={collector.install}>
        {busy === "installing" ? "Waiting for Windows…" : outdated ? "Update" : "Turn on"}
      </button>
      {!outdated && (
        <button className="button button-icon button-quiet" aria-label="Not now" title="Not now. You can turn it on later from the version menu." onClick={dismiss}>
          <CloseIcon />
        </button>
      )}
    </section>
  );
}
