// An app's icon, or its first letter in a circle while the icon loads or when it has none.

import type { CSSProperties } from "react";

import { useAppIcon } from "../hooks/useAppIcon";

interface Props {
  appId: string;
  name: string;
  /** Pixels; 32 by default. */
  size?: number;
  /** Connected right now: a green ring (letter) or a small green dot (icon). */
  online?: boolean;
  className?: string;
}

export function AppIcon({ appId, name, size = 32, online = false, className = "" }: Props) {
  const icon = useAppIcon(appId);
  const style = { "--icon-size": `${size}px` } as CSSProperties;
  return (
    <span className={`app-icon ${icon ? "has-image" : ""} ${online ? "is-online" : ""} ${className}`} style={style} aria-hidden="true">
      {icon ? (
        <img src={icon} alt="" draggable={false} />
      ) : (
        <span className={`avatar ${online ? "is-online" : ""}`}>{name.charAt(0).toUpperCase()}</span>
      )}
    </span>
  );
}
