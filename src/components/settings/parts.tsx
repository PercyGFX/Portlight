// The building blocks of every Settings section: titles, grouped rows, switches and small buttons.

import { useState, type ReactNode } from "react";

import { CopyButton } from "../CopyButton";
import { ExternalIcon } from "../icons";

export function Title({ text, hint }: { text: string; hint?: string }) {
  return (
    <div className="settings-title">
      <h2>{text}</h2>
      {hint && <p>{hint}</p>}
    </div>
  );
}

/** A rounded group of rows; `action` sits at the right of the title. */
export function Group({ title, action, children }: { title?: string; action?: ReactNode; children: ReactNode }) {
  return (
    <div className="settings-group">
      {(title || action) && (
        <div className="settings-group-head">
          {title && <h3 className="section-title">{title}</h3>}
          {action}
        </div>
      )}
      <div className="settings-rows">{children}</div>
    </div>
  );
}

/** A label, an optional hint and one control; `extra` spans the row underneath. */
export function Row(props: { label: string; hint?: ReactNode; control?: ReactNode; mono?: boolean; extra?: ReactNode }) {
  const { label, hint, control, mono = false, extra } = props;
  const main = (
    <>
      <div className="settings-row-text">
        <span className="settings-label">{label}</span>
        {hint && <span className={`settings-hint ${mono ? "mono" : ""}`}>{hint}</span>}
      </div>
      {control && <div className="settings-control">{control}</div>}
    </>
  );
  if (!extra) return <div className="settings-row">{main}</div>;
  return (
    <div className="settings-row is-stacked">
      <div className="settings-row-top">{main}</div>
      {extra}
    </div>
  );
}

export function Credit({ name, text, onClick }: { name: string; text: string; onClick?: () => void }) {
  return <Row label={name} hint={text} control={onClick ? <LinkButton label="Visit" onClick={onClick} /> : undefined} />;
}

export function Switch({ label, checked, disabled = false, onChange }: { label: string; checked: boolean; disabled?: boolean; onChange(value: boolean): void }) {
  return (
    <label className={`switch ${disabled ? "is-disabled" : ""}`}>
      <input type="checkbox" role="switch" aria-label={label} checked={checked} disabled={disabled} onChange={(event) => onChange(event.target.checked)} />
      <span className="switch-track" aria-hidden="true">
        <span className="switch-thumb" />
      </span>
    </label>
  );
}

export function Segmented<T extends string>({ label, value, options, onChange }: { label: string; value: T; options: Array<[T, string]>; onChange(value: T): void }) {
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

export function LinkButton({ label, onClick }: { label: string; onClick(): void }) {
  return (
    <button className="button" onClick={onClick}>
      {label} <ExternalIcon />
    </button>
  );
}

/** A button that asks once more, in place, before doing something that cannot be undone. */
export function ConfirmButton(props: { label: string; question: string; confirm: string; danger?: boolean; disabled?: boolean; onConfirm(): void }) {
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

export function ErrorNote({ text }: { text: string }) {
  return (
    <p className="menu-error has-copy settings-error" role="alert">
      <span>{text}</span>
      <CopyButton value={text} what="message" />
    </p>
  );
}
