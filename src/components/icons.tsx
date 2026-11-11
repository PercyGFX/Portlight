// Small line icons, drawn to sit on the text baseline at 14px.

import type { ReactNode } from "react";

function Icon({ children, size = 14 }: { children: ReactNode; size?: number }) {
  return (
    <svg
      className="icon"
      width={size}
      height={size}
      viewBox="0 0 16 16"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.5"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      {children}
    </svg>
  );
}

export const SearchIcon = () => (
  <Icon>
    <circle cx="7" cy="7" r="4.5" />
    <path d="M10.5 10.5 14 14" />
  </Icon>
);

export const SunIcon = () => (
  <Icon>
    <circle cx="8" cy="8" r="2.6" />
    <path d="M8 1.5v1.6M8 12.9v1.6M1.5 8h1.6M12.9 8h1.6M3.4 3.4l1.1 1.1M11.5 11.5l1.1 1.1M3.4 12.6l1.1-1.1M11.5 4.5l1.1-1.1" />
  </Icon>
);

export const MoonIcon = () => (
  <Icon>
    <path d="M13 9.6A5.6 5.6 0 0 1 6.4 3 5.6 5.6 0 1 0 13 9.6Z" />
  </Icon>
);

export const PauseIcon = () => (
  <Icon>
    <path d="M5 3v10M11 3v10" strokeWidth="2.4" />
  </Icon>
);

export const PlayIcon = () => (
  <Icon>
    <path d="M4.5 2.8v10.4L13 8Z" fill="currentColor" />
  </Icon>
);

export const BellIcon = () => (
  <Icon>
    <path d="M3.5 11.5h9l-1-1.6V7a3.5 3.5 0 0 0-7 0v2.9ZM6.6 13.6h2.8" />
  </Icon>
);

export const CloseIcon = () => (
  <Icon>
    <path d="m3.5 3.5 9 9M12.5 3.5l-9 9" />
  </Icon>
);

export const CheckIcon = () => (
  <Icon>
    <path d="m3 8.4 3.2 3.1L13 4.6" />
  </Icon>
);

export const ClearIcon = () => (
  <Icon>
    <path d="M3 4.5h10M6.2 4.5V3h3.6v1.5M4.4 4.5l.6 8.5h6l.6-8.5" />
  </Icon>
);

export const BackIcon = () => (
  <Icon>
    <path d="M9.5 3.5 5 8l4.5 4.5" />
  </Icon>
);

/** A recording dot: "capture packets". */
export const RecordIcon = () => (
  <Icon>
    <circle cx="8" cy="8" r="5.5" />
    <circle cx="8" cy="8" r="2.4" fill="currentColor" stroke="none" />
  </Icon>
);

export const StopIcon = () => (
  <Icon>
    <rect x="3.5" y="3.5" width="9" height="9" rx="1.6" fill="currentColor" />
  </Icon>
);

export const ChevronDownIcon = () => (
  <Icon>
    <path d="m4 6 4 4 4-4" />
  </Icon>
);

export const ChevronRightIcon = () => (
  <Icon>
    <path d="m6 4 4 4-4 4" />
  </Icon>
);

export const SaveIcon = () => (
  <Icon>
    <path d="M8 2.5v7.5M4.8 7 8 10.2 11.2 7M3 12.8h10" />
  </Icon>
);

export const HelpIcon = () => (
  <Icon>
    <circle cx="8" cy="8" r="6" />
    <path d="M6.3 6.3a1.8 1.8 0 1 1 2.4 1.7c-.5.2-.7.6-.7 1.1v.3" />
    <circle cx="8" cy="11.6" r=".5" fill="currentColor" />
  </Icon>
);

export const LockIcon = () => (
  <Icon>
    <rect x="3.5" y="7" width="9" height="6.5" rx="1.5" />
    <path d="M5.5 7V5.2a2.5 2.5 0 0 1 5 0V7" />
  </Icon>
);

export const CopyIcon = () => (
  <Icon>
    <rect x="5.5" y="5.5" width="7.5" height="7.5" rx="1.5" />
    <path d="M10.5 3.6V3.5A1.5 1.5 0 0 0 9 2H4.5A1.5 1.5 0 0 0 3 3.5V8a1.5 1.5 0 0 0 1.5 1.5h.1" />
  </Icon>
);

export const DownIcon = () => (
  <Icon>
    <path d="M8 3v10M4 9l4 4 4-4" />
  </Icon>
);

/** The Portlight mark: a ship's porthole with one light on the water line. */
export function Logo() {
  return (
    <svg className="logo" width="44" height="44" viewBox="0 0 44 44" fill="none" aria-hidden="true">
      <circle className="logo-plate" cx="22" cy="22" r="20.5" />
      <circle className="logo-ring" cx="22" cy="22" r="12.5" />
      <circle className="logo-glass" cx="22" cy="22" r="8" />
      <path className="logo-wave" d="M14.6 24.4c2.4-2.2 4.9-2.2 7.4 0s5 2.2 7.4 0" />
      <circle className="logo-light" cx="25.4" cy="18.6" r="1.9" />
      <circle className="logo-bolt" cx="22" cy="6.3" r="1.2" />
      <circle className="logo-bolt" cx="22" cy="37.7" r="1.2" />
      <circle className="logo-bolt" cx="6.3" cy="22" r="1.2" />
      <circle className="logo-bolt" cx="37.7" cy="22" r="1.2" />
    </svg>
  );
}
