// Turns port numbers into words a non-expert can read.

import type { FlowEvent } from "./types";

interface Service {
  /** Short tag, e.g. "HTTPS". */
  label: string;
  /** Fits the sentence "Started ___". */
  plain: string;
}

const SERVICES: Record<number, Service> = {
  21: { label: "FTP", plain: "a file transfer" },
  22: { label: "SSH", plain: "a secure remote session" },
  25: { label: "SMTP", plain: "sending mail" },
  53: { label: "DNS", plain: "a name lookup" },
  80: { label: "HTTP", plain: "unencrypted web traffic" },
  123: { label: "NTP", plain: "a clock sync" },
  143: { label: "IMAP", plain: "reading mail" },
  443: { label: "HTTPS", plain: "secure web traffic" },
  465: { label: "SMTPS", plain: "sending mail" },
  587: { label: "SMTP", plain: "sending mail" },
  853: { label: "DNS over TLS", plain: "a private name lookup" },
  993: { label: "IMAPS", plain: "reading mail" },
  995: { label: "POP3S", plain: "reading mail" },
  1883: { label: "MQTT", plain: "device messaging" },
  1935: { label: "RTMP streaming", plain: "live video streaming" },
  3389: { label: "Remote Desktop", plain: "a remote desktop session" },
  3478: { label: "Calls (STUN)", plain: "setting up a call" },
  5222: { label: "XMPP", plain: "chat messaging" },
  5228: { label: "Google Play / FCM", plain: "push notifications" },
  5353: { label: "mDNS", plain: "looking for devices on the home network" },
  7844: { label: "Cloudflare Tunnel", plain: "a Cloudflare Tunnel connection" },
  8080: { label: "HTTP-ALT", plain: "web traffic on an unusual port" },
  8883: { label: "MQTTS", plain: "device messaging" },
  19302: { label: "Calls (STUN)", plain: "setting up a call" },
};

export function serviceOf(port: number): Service {
  return SERVICES[port] ?? { label: `Port ${port}`, plain: `a connection on port ${port}` };
}

/** A few plain words for an event, e.g. "Started secure web traffic". */
export function describe(event: FlowEvent): string {
  const service = serviceOf(event.remotePort);
  return `${event.kind === "opened" ? "Started" : "Finished"} ${service.plain}`;
}
