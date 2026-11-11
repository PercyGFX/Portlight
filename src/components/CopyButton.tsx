// A small icon button that copies a value to the clipboard and says so for a moment.

import { useEffect, useRef, useState, type MouseEvent, type ReactNode } from "react";

import { CheckIcon, CopyIcon } from "./icons";

const COPIED_MS = 1500;

interface Props {
  /** The text to copy. */
  value: string;
  /** What is being copied, for the label: "Copy {what}". */
  what: string;
  /** Element whose text to select if the clipboard refuses; by default the element just before the button. */
  fallback?: () => HTMLElement | null;
  className?: string;
  /** Text shown next to the icon, e.g. "Copy as filter". */
  children?: ReactNode;
}

export function CopyButton({ value, what, fallback, className = "", children }: Props) {
  const [copied, setCopied] = useState(false);
  const timer = useRef(0);
  const label = copied ? "Copied" : `Copy ${what}`;

  useEffect(() => () => window.clearTimeout(timer.current), []);

  const copy = async (event: MouseEvent) => {
    event.stopPropagation();
    const button = event.currentTarget as HTMLElement;
    try {
      await navigator.clipboard.writeText(value);
      setCopied(true);
      window.clearTimeout(timer.current);
      timer.current = window.setTimeout(() => setCopied(false), COPIED_MS);
    } catch {
      // No clipboard access: select the text so Ctrl+C works.
      const element = fallback?.() ?? (button.previousElementSibling as HTMLElement | null);
      if (element) window.getSelection()?.selectAllChildren(element);
    }
  };

  return (
    <button
      type="button"
      className={`copy-button ${children ? "has-text" : ""} ${copied ? "is-copied" : ""} ${className}`}
      aria-label={label}
      title={label}
      onClick={copy}
    >
      {copied ? <CheckIcon /> : <CopyIcon />}
      {children && <span>{copied ? "Copied" : children}</span>}
    </button>
  );
}
