// Turns port numbers into words a non-expert can read.

import type { FlowEvent } from "./types";

interface Service {
  /** Short tag, e.g. "HTTPS". */
  label: string;
  /** Fits the sentence "Started ___ with example.com". */
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
  853: { label: "DNS-TLS", plain: "a private name lookup" },
  993: { label: "IMAPS", plain: "reading mail" },
  995: { label: "POP3S", plain: "reading mail" },
  1883: { label: "MQTT", plain: "device messaging" },
  3389: { label: "RDP", plain: "a remote desktop session" },
  5222: { label: "XMPP", plain: "chat messaging" },
  5228: { label: "PUSH", plain: "push notifications" },
  8080: { label: "HTTP-ALT", plain: "web traffic on a non-standard port" },
  8883: { label: "MQTTS", plain: "device messaging" },
};

export function serviceOf(port: number): Service {
  return SERVICES[port] ?? { label: `PORT ${port}`, plain: `a connection on port ${port}` };
}

/** One plain-English sentence for an event. */
export function describe(event: FlowEvent, destination: string): string {
  const service = serviceOf(event.remotePort);
  return event.kind === "opened"
    ? `Started ${service.plain} with ${destination}`
    : `Finished talking to ${destination}`;
}
