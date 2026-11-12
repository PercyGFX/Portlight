// Settings, iPhone style: a short list of sections on the left, grouped rows on the right,
// each row a plain label, an optional one-line hint and one control.

import { useEffect, useState, type ReactNode } from "react";

import type { Capture } from "../../hooks/useCapture";
import type { Collector } from "../../hooks/useCollector";
import { useHistorySettings } from "../../hooks/useHistory";
import { TOASTS_PREF, usePref } from "../../hooks/usePref";
import { useSettings } from "../../hooks/useSettings";
import type { ThemeState } from "../../hooks/useTheme";
import type { Updates } from "../../hooks/useUpdates";
import { backend } from "../../lib/backend";
import { bytes, count, dayLabel } from "../../lib/format";
import type { LinkTarget } from "../../lib/types";
import { CopyButton } from "../CopyButton";
import { BackIcon, ExternalIcon, Logo } from "../icons";
import { Changelog } from "./Changelog";

export type SettingsSection = "general" | "alerts" | "monitoring" | "privacy" | "capture" | "updates" | "about";

const SECTIONS: Array<[SettingsSection, string]> = [
  ["general", "General"],
  ["alerts", "Alerts"],
  ["monitoring", "Monitoring"],
  ["privacy", "Privacy & data"],
  ["capture", "Packet capture"],
  ["updates", "Updates"],
  ["about", "About"],
];

const KEEP_CHOICES = [7, 30, 90, 365];

interface Props {
  section: SettingsSection;
  theme: ThemeState;
  updates: Updates;
  collector: Collector;
  capture: Capture;
  leaving: boolean;
  onSection(section: SettingsSection): void;
  onBack(): void;
}

const open = (link: LinkTarget) => void backend.openLink(link).catch(console.error);

export function SettingsPage({ section, theme, updates, collector, capture, leaving, onSection, onBack }: Props) {
  const settings = useSettings();

  // Esc goes back to the live view, unless a field or a confirm has focus.
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      const target = event.target as HTMLElement | null;
      if (target?.closest(".settings-confirm") || target?.tagName === "INPUT") return;
      if (document.querySelector(".settings-confirm")) return;
      onBack();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onBack]);

  const available = updates.status?.availableVersion ?? null;

  return (
    <div className={`settings-page ${leaving ? "is-leaving" : "is-entering"}`}>
      <header className="capture-head settings-head">
        <button className="button capture-back" onClick={onBack} title="Back to the live view (Esc)">
          <BackIcon /> Live view
        </button>
        <div className="capture-heading">
          <h1>Settings</h1>
        </div>
      </header>

      <div className="settings-body">
        <nav className="settings-nav" aria-label="Settings sections">
          {SECTIONS.map(([id, label]) => (
            <button
              key={id}
              className={`settings-nav-item ${section === id ? "is-active" : ""}`}
              aria-current={section === id ? "page" : undefined}
              onClick={() => onSection(id)}
            >
              {label}
              {id === "updates" && available && <span className="tag tag-new">New</span>}
              {id === "monitoring" && collector.status?.installed && collector.status.outdated && <span className="tag tag-new">Update</span>}
            </button>
          ))}
        </nav>

        <section className="settings-content panel" aria-label={SECTIONS.find(([id]) => id === section)?.[1]}>
          <div className="settings-scroll" key={section}>
            {settings.error && (
              <p className="menu-error has-copy settings-error" role="alert">
                <span>{settings.error}</span>
                <CopyButton value={settings.error} what="message" />
              </p>
            )}
            {section === "general" && <General settings={settings} theme={theme} />}
            {section === "alerts" && <Alerts settings={settings} />}
            {section === "monitoring" && <Monitoring collector={collector} />}
            {section === "privacy" && <Privacy settings={settings} />}
            {section === "capture" && <CaptureSection capture={capture} />}
            {section === "updates" && <UpdatesSection updates={updates} />}
            {section === "about" && <AboutSection settings={settings} />}
          </div>
        </section>
      </div>
    </div>
  );
}

type SettingsProps = { settings: ReturnType<typeof useSettings> };

function General({ settings, theme }: SettingsProps & { theme: ThemeState }) {
  const s = settings.settings;
  return (
    <>
      <Title text="General" />
      <Group>
        <Row
          label="Start Portlight when I sign in"
          hint={s && !s.canStartAtSignIn ? "Only the installed copy can start by itself." : "Opens quietly in the tray, ready to tell you about new apps."}
          control={
            <Switch
              label="Start Portlight when I sign in"
              checked={(s?.canStartAtSignIn && s.startAtSignIn) ?? false}
              disabled={!s || !s.canStartAtSignIn}
              onChange={(value) => settings.change({ startAtSignIn: value })}
            />
          }
        />
        <Row
          label="Keep running in the tray when I close the window"
          hint={s?.closeToTray === false ? "Closing the window quits Portlight." : "Portlight keeps watching. Quit from the tray icon."}
          control={
            <Switch
              label="Keep running in the tray when I close the window"
              checked={s?.closeToTray ?? true}
              disabled={!s}
              onChange={(value) => settings.change({ closeToTray: value })}
            />
          }
        />
      </Group>
      <Group>
        <Row
          label="Theme"
          control={
            <Segmented
              label="Theme"
              value={theme.choice}
              options={[
                ["light", "Light"],
                ["dark", "Dark"],
                ["system", "Match Windows"],
              ]}
              onChange={theme.setChoice}
            />
          }
        />
      </Group>
    </>
  );
}

function Alerts({ settings }: SettingsProps) {
  const s = settings.settings;
  const [toasts, setToasts] = usePref(TOASTS_PREF, true);
  const [done, setDone] = useState<string | null>(null);

  const run = (work: () => Promise<void>, message: string) => {
    setDone(null);
    work()
      .then(() => setDone(message))
      .catch((reason) => setDone(String(reason)));
  };

  return (
    <>
      <Title text="Alerts" hint="Portlight tells you the first time an app goes online." />
      <Group>
        <Row
          label="Notify me when a new app goes online"
          hint="A Windows notification, even when the window is closed."
          control={
            <Switch
              label="Notify me when a new app goes online"
              checked={s?.notifyNewApps ?? true}
              disabled={!s}
              onChange={(value) => settings.change({ notifyNewApps: value })}
            />
          }
        />
        <Row
          label="Pop-ups inside the window"
          hint="A small card in the corner while the window is open."
          control={<Switch label="Pop-ups inside the window" checked={toasts} onChange={setToasts} />}
        />
      </Group>
      <Group>
        <Row
          label="Learn again"
          hint="Too many alerts? Treat every app seen so far as known, then watch quietly for two minutes."
          control={<ConfirmButton label="Learn again" question="Treat every app as known?" confirm="Learn again" onConfirm={() => run(() => backend.learnAgain(), "Learning for two minutes. Every app seen so far is known.")} />}
        />
        <Row
          label="Forget all known apps"
          hint="Start over. Apps online now stay trusted; any other app alerts again the next time it goes online."
          control={
            <ConfirmButton
              label="Forget all"
              question="Forget every known app?"
              confirm="Forget all"
              danger
              onConfirm={() => run(() => backend.forgetKnownApps(), "Done. Portlight is learning again for two minutes.")}
            />
          }
        />
      </Group>
      {done && (
        <p className="settings-note" role="status">
          {done}
        </p>
      )}
    </>
  );
}

function Monitoring({ collector }: { collector: Collector }) {
  const { status, busy, error } = collector;
  const on = status?.installed ?? false;
  const state = !status
    ? "Checking…"
    : busy === "installing"
      ? "Waiting for Windows to allow it…"
      : busy === "removing"
        ? "Turning off…"
        : on
          ? `On${status.version ? ` · background service ${status.version}` : ""}`
          : "Off · basic monitoring";

  return (
    <>
      <Title
        text="Monitoring"
        hint="Basic checks the list of open connections twice a second. Detailed sees every connection, UDP and the data each app uses."
      />
      <Group>
        <Row
          label="Detailed monitoring"
          hint={
            on
              ? `${state}. Turning it off asks Windows for permission once.`
              : status && !status.canInstall
                ? "Not available in this copy: the background service is missing."
                : `${state}. Turning it on installs a small background service and asks Windows for permission once.`
          }
          control={
            on ? (
              <button className="button" disabled={busy !== null} onClick={collector.uninstall}>
                {busy === "removing" ? "Turning off…" : "Turn off"}
              </button>
            ) : (
              <button className="button button-solid" disabled={busy !== null || !status?.canInstall} onClick={collector.install}>
                {busy === "installing" ? "Waiting…" : "Turn on"}
              </button>
            )
          }
        />
        {on && status?.outdated && (
          <Row
            label="Update the background service"
            hint="It is older than this app. One Windows permission prompt."
            control={
              <button className="button" disabled={busy !== null} onClick={collector.install}>
                Update
              </button>
            }
          />
        )}
      </Group>
      {error && <ErrorNote text={error} />}
    </>
  );
}

function Privacy({ settings }: SettingsProps) {
  const s = settings.settings;
  const history = useHistorySettings();
  const h = history.settings;
  const [cleared, setCleared] = useState(false);
  const about = settings.about;

  return (
    <>
      <Title text="Privacy & data" hint="Everything Portlight learns stays on this PC." />
      <Group title="History">
        <Row
          label="Keep a history"
          hint="Which app talked to which server, by day. Only counts and times, never what was sent."
          control={<Switch label="Keep a history" checked={h?.enabled ?? false} disabled={!h} onChange={history.setEnabled} />}
        />
        <Row
          label="Keep for"
          control={
            <Segmented
              label="Keep history for"
              value={String(h?.keepDays ?? 90)}
              options={KEEP_CHOICES.map((days) => [String(days), `${days} days`] as [string, string])}
              onChange={(value) => history.setKeepDays(Number(value))}
            />
          }
        />
        <Row
          label="Clear history"
          hint={
            h
              ? cleared
                ? "History cleared."
                : h.oldestDay
                  ? `${bytes(h.sizeBytes)} on disk, since ${dayLabel(h.oldestDay)}.`
                  : `Nothing recorded yet. ${bytes(h.sizeBytes)} on disk.`
              : undefined
          }
          control={
            <ConfirmButton
              label="Clear history"
              question="Delete all history?"
              confirm="Delete"
              danger
              disabled={!h?.oldestDay}
              onConfirm={() => void history.clear().then(() => setCleared(true))}
            />
          }
        />
      </Group>
      {history.error && <ErrorNote text={history.error} />}

      <Group title="Servers">
        <Row
          label="Look up server names"
          hint={
            s?.reverseLookups === false
              ? "Off. Portlight asks no one; servers show as addresses unless an app looked their name up."
              : "Asks your DNS server for the name behind an address, the way any app would."
          }
          control={
            <Switch
              label="Look up server names"
              checked={s?.reverseLookups ?? true}
              disabled={!s}
              onChange={(value) => settings.change({ reverseLookups: value })}
            />
          }
        />
        <Row
          label="Server owner list"
          hint={
            about?.ownersBuiltAt
              ? `From ${new Date(about.ownersBuiltAt).toLocaleDateString("en-GB", { day: "numeric", month: "short", year: "numeric" })}${about.ownerRanges ? ` · ${count(about.ownerRanges)} network ranges` : ""} · IPtoASN, public domain. Looked up on this PC.`
              : "IPtoASN, public domain. Looked up on this PC; nothing is sent anywhere."
          }
          control={<LinkButton label="IPtoASN" onClick={() => open("iptoasn")} />}
        />
      </Group>

      <Group>
        <Row
          label="Data folder"
          hint={about?.dataFolder ?? "Where Portlight keeps its settings and history."}
          mono={about?.dataFolder !== undefined}
          control={<LinkButton label="Open" onClick={() => open("dataFolder")} />}
        />
      </Group>
    </>
  );
}

function CaptureSection({ capture }: { capture: Capture }) {
  const support = capture.support;
  return (
    <>
      <Title text="Packet capture" hint="Portlight records packets with Npcap, the capture driver Wireshark uses, and only while you press Record." />
      <Group>
        <Row
          label="Npcap"
          hint={
            support === null
              ? "Checking…"
              : support.available
                ? `Installed${support.npcapVersion ? ` · version ${support.npcapVersion}` : ""}. ${plural(support.interfaces.filter((i) => i.active).length)} ready.`
                : (support.problem ?? "Not installed.")
          }
          control={
            support?.available ? (
              <span className="settings-ok">Ready</span>
            ) : (
              <span className="settings-actions">
                <button className="button" disabled={capture.checking} onClick={capture.checkSupport}>
                  {capture.checking ? "Checking…" : "Check again"}
                </button>
                <button className="button button-solid" onClick={() => open("npcap")}>
                  Get Npcap
                </button>
              </span>
            )
          }
        />
      </Group>
      <p className="settings-foot">Npcap is a separate download with its own licence. It is free for personal use.</p>
    </>
  );
}

const plural = (cards: number) => `${cards} network card${cards === 1 ? "" : "s"}`;

function UpdatesSection({ updates }: { updates: Updates }) {
  const { status, busy, error, upToDate } = updates;
  if (!status) return <Title text="Updates" hint="Checking…" />;
  const available = status.availableVersion;

  return (
    <>
      <Title text="Updates" hint={`You have Portlight ${status.current}.`} />
      {available && (
        <div className="settings-update step-in">
          <div>
            <strong>Version {available} is ready</strong>
            <span>{status.canInstall ? "It installs in a moment and Portlight restarts." : "This copy cannot update itself. Download it from GitHub."}</span>
          </div>
          <span className="settings-actions">
            <button className="button button-quiet" onClick={() => open("releases")}>
              What's new <ExternalIcon />
            </button>
            {status.canInstall && (
              <button className="button button-solid" disabled={busy !== null} onClick={updates.install}>
                {busy === "installing" ? "Downloading…" : `Update to ${available}`}
              </button>
            )}
          </span>
        </div>
      )}
      <Group>
        <Row
          label="Install updates automatically"
          hint={status.canInstall ? "Portlight checks GitHub twice a day and updates itself quietly." : "This copy was not set up by the installer, so it will not update itself."}
          control={<Switch label="Install updates automatically" checked={status.autoUpdate} onChange={updates.setAutoUpdate} />}
        />
        <Row
          label="Check for updates"
          hint={error ?? (busy === "checking" ? "Asking GitHub…" : upToDate ? "You have the latest version." : undefined)}
          control={
            <button className="button" disabled={busy !== null} onClick={updates.check}>
              {busy === "checking" ? "Checking…" : "Check now"}
            </button>
          }
        />
      </Group>
    </>
  );
}

function AboutSection({ settings }: SettingsProps) {
  const about = settings.about;
  return (
    <>
      <div className="about-hero">
        <Logo />
        <div>
          <h2>Portlight</h2>
          <p>
            Version {about?.version ?? "…"} · See which apps are online
          </p>
        </div>
      </div>
      <Group>
        <Row label="Made by" control={<span className="settings-value">{about?.authors ?? "…"}</span>} />
        <Row label="Source code" hint="Free and open, on GitHub." control={<LinkButton label="GitHub" onClick={() => open("repository")} />} />
        <Row label="Report a problem" hint="Or suggest something. Opens GitHub issues." control={<LinkButton label="Report" onClick={() => open("issues")} />} />
        <Row label="Licence" control={<LinkButton label={about?.license ?? "MIT"} onClick={() => open("license")} />} />
      </Group>

      <Group title="What's new">
        <div className="settings-changelog">
          <Changelog />
        </div>
      </Group>

      <Group title="Credits">
        <Credit name="IPtoASN" text="Server owner and country data, by Frank Denis. Public domain (PDDL)." onClick={() => open("iptoasn")} />
        <Credit name="Manrope and JetBrains Mono" text="The typefaces. SIL Open Font License." />
        <Credit name="Tauri and React" text="What Portlight is built with." />
        <Credit name="Npcap" text="The capture driver, installed separately under its own licence." onClick={() => open("npcap")} />
      </Group>

    </>
  );
}

// ---------- Building blocks ----------

function Title({ text, hint }: { text: string; hint?: string }) {
  return (
    <div className="settings-title">
      <h2>{text}</h2>
      {hint && <p>{hint}</p>}
    </div>
  );
}

function Group({ title, children }: { title?: string; children: ReactNode }) {
  return (
    <div className="settings-group">
      {title && <h3 className="section-title">{title}</h3>}
      <div className="settings-rows">{children}</div>
    </div>
  );
}

function Row({ label, hint, control, mono = false }: { label: string; hint?: string; control?: ReactNode; mono?: boolean }) {
  return (
    <div className="settings-row">
      <div className="settings-row-text">
        <span className="settings-label">{label}</span>
        {hint && <span className={`settings-hint ${mono ? "mono" : ""}`}>{hint}</span>}
      </div>
      {control && <div className="settings-control">{control}</div>}
    </div>
  );
}

function Credit({ name, text, onClick }: { name: string; text: string; onClick?: () => void }) {
  return <Row label={name} hint={text} control={onClick ? <LinkButton label="Visit" onClick={onClick} /> : undefined} />;
}

function Switch({ label, checked, disabled = false, onChange }: { label: string; checked: boolean; disabled?: boolean; onChange(value: boolean): void }) {
  return (
    <label className={`switch ${disabled ? "is-disabled" : ""}`}>
      <input type="checkbox" role="switch" aria-label={label} checked={checked} disabled={disabled} onChange={(event) => onChange(event.target.checked)} />
      <span className="switch-track" aria-hidden="true">
        <span className="switch-thumb" />
      </span>
    </label>
  );
}

function Segmented<T extends string>({ label, value, options, onChange }: { label: string; value: T; options: Array<[T, string]>; onChange(value: T): void }) {
  return (
    <div className="segmented" role="radiogroup" aria-label={label}>
      {options.map(([id, text]) => (
        <button key={id} role="radio" aria-checked={value === id} className={value === id ? "is-active" : ""} onClick={() => onChange(id)}>
          {text}
        </button>
      ))}
    </div>
  );
}

function LinkButton({ label, onClick }: { label: string; onClick(): void }) {
  return (
    <button className="button" onClick={onClick}>
      {label} <ExternalIcon />
    </button>
  );
}

/** A button that asks once more, in place, before doing something that cannot be undone. */
function ConfirmButton(props: { label: string; question: string; confirm: string; danger?: boolean; disabled?: boolean; onConfirm(): void }) {
  const { label, question, confirm, danger = false, disabled = false, onConfirm } = props;
  const [asking, setAsking] = useState(false);
  if (!asking) {
    return (
      <button className={`button ${danger ? "button-calm-danger" : ""}`} disabled={disabled} onClick={() => setAsking(true)}>
        {label}
      </button>
    );
  }
  return (
    <span
      className={`settings-confirm step-in ${danger ? "is-danger" : ""}`}
      role="alertdialog"
      aria-label={question}
      onKeyDown={(event) => event.key === "Escape" && setAsking(false)}
    >
      <span>{question}</span>
      <button
        className={`button ${danger ? "button-danger" : "button-solid"}`}
        autoFocus
        onClick={() => {
          setAsking(false);
          onConfirm();
        }}
      >
        {confirm}
      </button>
      <button className="button button-quiet" onClick={() => setAsking(false)}>
        Cancel
      </button>
    </span>
  );
}

function ErrorNote({ text }: { text: string }) {
  return (
    <p className="menu-error has-copy settings-error" role="alert">
      <span>{text}</span>
      <CopyButton value={text} what="message" />
    </p>
  );
}
