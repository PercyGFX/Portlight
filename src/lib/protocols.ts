// Groups packet protocols into a few families, each with its own colour and a plain description.

export type Family = "web" | "names" | "tcp" | "udp" | "local" | "other";

const FAMILIES: Record<string, Family> = {
  TLS: "web",
  HTTP: "web",
  HTTPS: "web",
  QUIC: "web",
  DNS: "names",
  MDNS: "names",
  LLMNR: "names",
  NBNS: "names",
  TCP: "tcp",
  UDP: "udp",
  NTP: "udp",
  ARP: "local",
  ICMP: "local",
  ICMPV6: "local",
  DHCP: "local",
  SSDP: "local",
};

export function familyOf(protocol: string): Family {
  return FAMILIES[protocol.toUpperCase()] ?? "other";
}

/** One line about a protocol, for the tag's tooltip. */
const ABOUT: Record<string, string> = {
  TLS: "TLS: the encryption that puts the S in HTTPS. The contents stay private.",
  HTTP: "HTTP: unencrypted web traffic. Anyone on the way can read it.",
  QUIC: "QUIC: the newer, faster way browsers fetch secure web pages. Encrypted.",
  DNS: "DNS: looking up a site name to find its address.",
  MDNS: "mDNS: devices on your home network announcing their names.",
  TCP: "TCP: opens a reliable connection and keeps data in order.",
  UDP: "UDP: quick messages without a connection, used for calls, games and streaming.",
  NTP: "NTP: setting the clock from a time server.",
  ARP: "ARP: finding which device on your home network has an address.",
  ICMP: "ICMP: network status messages, like ping.",
  DHCP: "DHCP: your PC getting its address from the router.",
  SSDP: "SSDP: devices on your home network looking for each other.",
};

export function aboutProtocol(protocol: string): string {
  return ABOUT[protocol.toUpperCase()] ?? `${protocol}: a protocol Portlight shows without a plain explanation yet.`;
}

/** Legend for the conversation strip. */
export const FAMILY_LABELS: Array<[Family, string]> = [
  ["web", "Web"],
  ["names", "Name lookups"],
  ["tcp", "Connection control"],
  ["udp", "Other UDP"],
  ["local", "Home network"],
];
