// Settings › AI assistants: the switch, adding Portlight to Claude and other apps, what they can do, and what was shared.

import { useState } from "react";

import type { Assistant } from "../../hooks/useAssistant";
import { bytes, hourMinute } from "../../lib/format";
import type { AssistantApp, AssistantLogEntry, AssistantOutcome } from "../../lib/types";
import { CopyButton } from "../CopyButton";
import { CheckIcon, ChevronRightIcon, CloseIcon } from "../icons";
import { ErrorNote, Group, Row, Switch, Title } from "./parts";

/** Rows shown before "Show all". */
const FIRST_ROWS = 40;

const CAN = [
  "See apps, connections, servers and their owners",
  "Read the history",
  "Read captured packets",
  "Start a capture, after you allow it",
];

const CANNOT = ["Trust, block or forget apps", "Change any setting", "See anything while the switch is off"];

const TAGS: Record<AssistantOutcome, [label: string, className: string] | null> = {
  answered: null,
  allowed: ["Allowed", "tag-allowed"],
  denied: ["Denied", "tag-denied"],
  error: ["Error", "tag-error"],
};

export function AssistantSection({ assistant }: { assistant: Assistant }) {
  const { status, error } = assistant;
  const enabled = status?.enabled ?? false;
  const missing = status !== null && status.mcpPath === null;

  return (
    <>
      <Title
        text="AI assistants"
        hint="Let AI assistants like Claude answer questions about your PC's network, using what Portlight knows. Portlight never sends anything itself: what an assistant reads goes to the assistant, and from there to its AI service."
      />
      <Group>
        <Row
          label="Let AI assistants read Portlight"
          hint="Off by default. Assistants can look, not change anything. Starting a capture always asks you first."
          control={<Switch label="Let AI assistants read Portlight" checked={enabled} disabled={!status} onChange={assistant.setEnabled} />}
        />
      </Group>

      <Group title="Connect an app">
        <ClaudeDesktopRow assistant={assistant} />
        <ClaudeCodeRow assistant={assistant} />
        <OtherAppsRow assistant={assistant} />
      </Group>
      {error && <ErrorNote text={error} />}
      {missing ? (
        <p className="settings-foot">
          The assistant program is not part of this copy of Portlight (a development build), so apps cannot be added from here. The installed
          Portlight has it.
        </p>
      ) : (
        status &&
        !enabled && <p className="settings-foot">Turn on the switch above too. Until then, assistants get no answers from Portlight.</p>
      )}

      <Group title="What assistants can do">
        <div className="assistant-can">
          <List title="Can" items={CAN} yes />
          <List title="Can't" items={CANNOT} />
        </div>
      </Group>

      <SharedLog assistant={assistant} />
    </>
  );
}

function ClaudeDesktopRow({ assistant }: { assistant: Assistant }) {
  const { status, added } = assistant;
  const desktop = status?.claudeDesktop;
  const configured = desktop?.configured ?? false;
  const installed = desktop?.installed ?? false;

  const hint = configured
    ? added.has("claudeDesktop")
      ? "Restart Claude Desktop to use it."
      : "Portlight is in Claude Desktop's settings."
    : installed
      ? "Adds Portlight to Claude Desktop's settings."
      : "Claude's app for Windows.";

  return (
    <Row
      label="Claude Desktop"
      hint={<span title={desktop?.configPath ?? undefined}>{hint}</span>}
      control={
        configured ? (
          <Added app="claudeDesktop" assistant={assistant} />
        ) : installed ? (
          <AddButton app="claudeDesktop" assistant={assistant} />
        ) : (
          <span className="settings-muted">{status ? "Not installed" : "…"}</span>
        )
      }
    />
  );
}

function ClaudeCodeRow({ assistant }: { assistant: Assistant }) {
  const { status, added } = assistant;
  const command = status?.claudeCodeCommand ?? null;
  const oneClick = status?.claudeCodeInstalled ?? false;
  const isAdded = added.has("claudeCode") || (status?.claudeCodeConfigured ?? false);
  const check = "Then type /mcp in Claude Code to check it's connected.";

  return (
    <Row
      label="Claude Code"
      hint={isAdded ? `Added for every project. ${check}` : oneClick ? `One click, or paste this command in any terminal. ${check}` : `Paste this command in a terminal. ${check}`}
      control={oneClick ? isAdded ? <Added app="claudeCode" assistant={assistant} /> : <AddButton app="claudeCode" assistant={assistant} /> : undefined}
      extra={command ? <Code text={command} what="command" /> : undefined}
    />
  );
}

function OtherAppsRow({ assistant }: { assistant: Assistant }) {
  const [open, setOpen] = useState(false);
  const snippet = assistant.status?.configSnippet ?? null;
  const path = assistant.status?.mcpPath ?? null;

  return (
    <div className={`settings-row is-stacked assistant-other ${open ? "is-open" : ""}`}>
      <button className="settings-disclosure" aria-expanded={open} onClick={() => setOpen((current) => !current)}>
        <span className="settings-row-text">
          <span className="settings-label">Other apps</span>
          <span className="settings-hint">Gemini CLI, Cursor, VS Code and any other app that works with MCP.</span>
        </span>
        <ChevronRightIcon />
      </button>
      {open && (
        <div className="assistant-more step-in">
          {snippet || path ? (
            <>
              {snippet && (
                <>
                  <span className="settings-hint">Add this to the app's MCP settings file.</span>
                  <Code text={snippet} what="settings" />
                </>
              )}
              {path && (
                <>
                  <span className="settings-hint">The assistant program, if the app asks for it:</span>
                  <Code text={path} what="path" />
                </>
              )}
            </>
          ) : (
            <span className="settings-hint">Not available in this copy of Portlight.</span>
          )}
        </div>
      )}
    </div>
  );
}

function AddButton({ app, assistant }: { app: AssistantApp; assistant: Assistant }) {
  const ready = assistant.status?.mcpPath != null;
  const working = assistant.busy === app;
  return (
    <button
      className="button"
      disabled={!ready || assistant.busy !== null}
      title={ready ? undefined : "Not available in this copy of Portlight"}
      onClick={() => assistant.connect(app, true)}
    >
      {working ? "Adding…" : "Add"}
    </button>
  );
}

function Added({ app, assistant }: { app: AssistantApp; assistant: Assistant }) {
  const working = assistant.busy === app;
  return (
    <span className="settings-actions">
      <span className="settings-ok">Added</span>
      <button className="button button-quiet button-small" disabled={assistant.busy !== null} onClick={() => assistant.connect(app, false)}>
        {working ? "Removing…" : "Remove"}
      </button>
    </span>
  );
}

/** A monospace box with a copy button, for commands and settings to paste elsewhere. */
function Code({ text, what }: { text: string; what: string }) {
  return (
    <div className="assistant-code">
      <pre className="mono">{text}</pre>
      <CopyButton value={text} what={what} />
    </div>
  );
}

function List({ title, items, yes = false }: { title: string; items: string[]; yes?: boolean }) {
  return (
    <div className={`assistant-can-col ${yes ? "is-yes" : "is-no"}`}>
      <h4>{title}</h4>
      <ul>
        {items.map((item) => (
          <li key={item}>
            {yes ? <CheckIcon /> : <CloseIcon />}
            <span>{item}</span>
          </li>
        ))}
      </ul>
    </div>
  );
}

function SharedLog({ assistant }: { assistant: Assistant }) {
  const { log } = assistant;
  const [all, setAll] = useState(false);
  const shown = all ? log : log.slice(0, FIRST_ROWS);

  return (
    <>
      <Group
        title="What was shared"
        action={
          <button className="button button-quiet button-small" disabled={log.length === 0} onClick={assistant.clearLog}>
            Clear
          </button>
        }
      >
        {log.length === 0 ? (
          <Row label="Nothing yet" hint="When an assistant asks Portlight something, it shows up here." />
        ) : (
          <ol className="assistant-log" aria-live="polite">
            {shown.map((entry) => (
              <LogRow key={entry.id} entry={entry} />
            ))}
          </ol>
        )}
        {log.length > shown.length && (
          <button className="settings-more" onClick={() => setAll(true)}>
            Show all {log.length}
          </button>
        )}
      </Group>
      <p className="settings-foot">Kept in memory only, and cleared when Portlight quits.</p>
    </>
  );
}

function LogRow({ entry }: { entry: AssistantLogEntry }) {
  const tag = TAGS[entry.outcome];
  return (
    <li className={`assistant-log-row is-${entry.outcome}`} title={`${entry.tool} · ${new Date(entry.time).toLocaleString()}`}>
      <span className="assistant-log-time">{hourMinute(entry.time)}</span>
      <span className="assistant-log-client">{entry.client}</span>
      <span className="assistant-log-summary">
        <span>{entry.summary}</span>
        {tag && <span className={`tag ${tag[1]}`}>{tag[0]}</span>}
      </span>
      <span className="assistant-log-size">{entry.bytes > 0 ? bytes(entry.bytes) : ""}</span>
    </li>
  );
}
