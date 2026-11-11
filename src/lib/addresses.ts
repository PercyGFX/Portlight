// Small facts about IP addresses.

/** True for addresses that stay inside the home or office network: private, link-local and unique-local. */
export function isLocalAddress(ip: string): boolean {
  if (ip.includes(":")) {
    const first = parseInt(ip.split(":")[0] || "0", 16);
    // fc00::/7 (unique local) and fe80::/10 (link-local).
    return (first & 0xfe00) === 0xfc00 || (first & 0xffc0) === 0xfe80;
  }
  const [a, b] = ip.split(".").map(Number);
  if (a === 10) return true;
  if (a === 172 && b >= 16 && b <= 31) return true;
  if (a === 192 && b === 168) return true;
  if (a === 169 && b === 254) return true;
  return false;
}
