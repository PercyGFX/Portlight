// Who runs a server's network, in words: "Cloudflare · United States".

import { isLocalAddress } from "./addresses";
import { countryName, inCountry } from "./countries";
import type { Signature } from "./types";

export const HOME_NETWORK = "Your home network";

/** What is known about a server's network. */
export interface Owner {
  owner: string | null;
  country: string | null;
}

// Legal endings that add nothing in a small label.
const SUFFIX = /[,.]?\s+(inc|incorporated|llc|l\.l\.c|ltd|limited|gmbh|ag|sa|sas|sarl|srl|b\.?v|bv|n\.?v|plc|pty|co|corp|corporation|company|ab|as|oy|kk|s\.?a\.?|s\.?r\.?l\.?|pte)\.?$/i;

/** "Cloudflare, Inc." -> "Cloudflare". */
export function shortOwner(owner: string): string {
  let name = owner.trim();
  for (let i = 0; i < 3; i++) {
    const next = name.replace(SUFFIX, "").trim();
    if (next === name || !next) break;
    name = next;
  }
  return name;
}

function isHome(owner: string | null, ip?: string): boolean {
  return owner === HOME_NETWORK || (ip !== undefined && isLocalAddress(ip));
}

/** "Cloudflare · United States", "Your home network", or null when nothing is known. */
export function ownerLine(info: Owner | null | undefined, ip?: string): string | null {
  if (isHome(info?.owner ?? null, ip)) return HOME_NETWORK;
  if (!info) return null;
  const parts = [info.owner && shortOwner(info.owner), info.country && countryName(info.country)].filter(Boolean);
  return parts.length ? parts.join(" · ") : null;
}

/** The full story for a tooltip: "Cloudflare, Inc. · Network registered in the United States". */
export function ownerTitle(info: Owner | null | undefined, ip?: string): string | null {
  if (isHome(info?.owner ?? null, ip)) return "A device on your home network";
  if (!info) return null;
  const parts = [info.owner, info.country && `Network registered in ${inCountry(info.country)}`].filter(Boolean);
  return parts.length ? parts.join(" · ") : null;
}

/** "Signed by Mozilla Corporation", "Not signed", or null while unknown. */
export function signatureLine(signature: Signature | undefined): string | null {
  if (!signature) return null;
  if (signature.state === "signed") return `Signed by ${signature.publisher}`;
  if (signature.state === "unsigned") return "Not signed";
  return null;
}
