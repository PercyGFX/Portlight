// Pretend packet capture for the demo backend: invents real-looking packets (with real bytes) for whatever the user chose to capture, and filters them.

import type { Backend } from "./backend";
import type {
  CaptureInterface,
  CaptureStatus,
  CaptureSupport,
  CaptureTarget,
  ConnView,
  PacketDetail,
  PacketField,
  PacketLayer,
  PacketRow,
} from "./types";

export interface DemoWorld {
  /** Connections open right now in the demo monitor. */
  connections(): ConnView[];
  /** App id -> display name ("Firefox", "Windows system services", ...). */
  appName(id: string): string;
}

export type DemoCaptureMethods = Pick<
  Backend,
  | "captureSupport"
  | "captureStart"
  | "captureStop"
  | "captureClear"
  | "captureStatus"
  | "onCaptureStatus"
  | "captureSetFilter"
  | "captureRows"
  | "capturePacket"
  | "captureSave"
  | "captureExportCsv"
  | "captureExport"
  | "openNpcapDownload"
>;

// ---------- The pretend home network ----------

const PC_IP = "192.168.1.23";
const ROUTER_IP = "192.168.1.1";
const TV_IP = "192.168.1.40";
const MDNS_IP = "224.0.0.251";
const SSDP_IP = "239.255.255.250";
const PC_MAC = "3a:5f:1c:9e:42:17";
const ROUTER_MAC = "02:1d:7e:a0:3c:01";
const TV_MAC = "06:3b:22:51:9d:40";
const BROADCAST_MAC = "ff:ff:ff:ff:ff:ff";
const ZERO_MAC = "00:00:00:00:00:00";
const MDNS_MAC = "01:00:5e:00:00:fb";
const SSDP_MAC = "01:00:5e:7f:ff:fa";
const SYSTEM = "Windows system services";
const EVERYONE = "Everyone on your network";
const CARD = "Wi-Fi";

const LIMIT = 200_000;
const plural = (n: number, word: string) => `${n.toLocaleString("en-US")} ${word}${n === 1 ? "" : "s"}`;
const TICK_MS = 100;
const STATUS_MS = 250;

const INTERFACES: CaptureInterface[] = [
  {
    id: "\\Device\\NPF_{6A1C3E2F-8B4D-4E9A-9C21-5F7D0B3A1E44}",
    name: "Wi-Fi",
    description: "Intel(R) Wi-Fi 6 AX201 160MHz",
    addresses: [PC_IP, "fe80::3c5f:1cff:fe9e:4217"],
    active: true,
  },
  {
    id: "\\Device\\NPF_{0D9B2C61-47E3-4F18-A5B0-93C6E1D27F08}",
    name: "Ethernet",
    description: "Realtek PCIe GbE Family Controller",
    addresses: [],
    active: false,
  },
];

/** Protocol bits, so a packet's protocols are one number. */
const P = {
  eth: 1,
  arp: 2,
  ip: 4,
  icmp: 8,
  tcp: 16,
  udp: 32,
  dns: 64,
  tls: 128,
  http: 256,
  quic: 512,
  mdns: 1024,
  ntp: 2048,
  ssdp: 4096,
  ipv6: 8192,
  icmpv6: 16384,
  dhcp: 32768,
} as const;

const TCP_IP = P.eth | P.ip | P.tcp;
const UDP_IP = P.eth | P.ip | P.udp;

const SYN = 0x02;
const ACK = 0x10;
const PSH = 0x08;
const FIN = 0x01;
const RST = 0x04;

const PORTS: Record<number, [label: string, plain: string]> = {
  53: ["DNS", "name lookups"],
  80: ["HTTP", "unencrypted web traffic"],
  123: ["NTP", "clock sync"],
  443: ["HTTPS", "secure web traffic"],
  587: ["SMTP", "sending mail"],
  993: ["IMAPS", "reading mail securely"],
  1900: ["SSDP", "finding media devices"],
  4070: ["Streaming", "music streaming"],
  5353: ["mDNS", "finding devices on your network"],
  8080: ["HTTP alternate", "web traffic on an unusual port"],
};

const TLS_PORTS = new Set([443, 465, 853, 993, 995, 5223, 8883]);
const HTTP_PORTS = new Set([80, 8080]);

const ENCRYPTED =
  "The contents are encrypted. Portlight (and anyone else on the way) can see its size, not what is inside.";

// ---------- Small helpers ----------

type B = number[];

const u16 = (a: B, v: number) => void a.push((v >>> 8) & 255, v & 255);
const u32 = (a: B, v: number) => void a.push((v >>> 24) & 255, (v >>> 16) & 255, (v >>> 8) & 255, v & 255);
const ascii = (a: B, s: string) => {
  for (let i = 0; i < s.length; i++) a.push(s.charCodeAt(i) & 255);
};
const randomBytes = (a: B, n: number) => {
  for (let i = 0; i < n; i++) a.push((Math.random() * 256) | 0);
};
const append = (a: B, more: ArrayLike<number>) => {
  for (let i = 0; i < more.length; i++) a.push(more[i]);
};
const ipBytes = (ip: string) => ip.split(".").map(Number);
const macBytes = (mac: string) => mac.split(":").map((h) => parseInt(h, 16));
const rand = (lo: number, hi: number) => lo + Math.floor(Math.random() * (hi - lo + 1));
const rand16 = () => (Math.random() * 0x10000) | 0;
const rand32 = () => (Math.random() * 0x100000000) >>> 0;
const hex = (v: number, width: number) => v.toString(16).padStart(width, "0");
const hexBytes = (b: ArrayLike<number>) => Array.from(b, (x) => hex(x, 2)).join("");
const pick = <T,>(items: T[]): T => items[Math.floor(Math.random() * items.length)];

function rd16(b: Uint8Array, o: number) {
  return (b[o] << 8) | b[o + 1];
}
function rd32(b: Uint8Array, o: number) {
  return ((b[o] << 24) | (b[o + 1] << 16) | (b[o + 2] << 8) | b[o + 3]) >>> 0;
}
function text(b: Uint8Array, o: number, n: number) {
  let s = "";
  for (let i = 0; i < n; i++) s += String.fromCharCode(b[o + i]);
  return s;
}
function ipAt(b: Uint8Array, o: number) {
  return `${b[o]}.${b[o + 1]}.${b[o + 2]}.${b[o + 3]}`;
}
function macAt(b: Uint8Array, o: number) {
  return Array.from(b.subarray(o, o + 6), (x) => hex(x, 2)).join(":");
}

/** "512 bytes", "1.3 kB". */
function size(n: number) {
  return n < 1000 ? `${n} bytes` : `${(n / 1000).toFixed(1)} kB`;
}

function portLabel(port: number) {
  const known = PORTS[port];
  return known ? `${port} (${known[0]})` : String(port);
}

/** A stable made-up address for a name, from the documentation ranges. */
function addressFor(name: string) {
  let hash = 0;
  for (const ch of name) hash = (hash * 31 + ch.charCodeAt(0)) >>> 0;
  return `203.0.113.${(hash % 250) + 2}`;
}

/** Fills `n` bytes deterministically from a seed, so large payloads need not be stored. */
function fillPad(seed: number, out: Uint8Array, at: number, n: number) {
  let s = seed >>> 0 || 1;
  for (let i = 0; i < n; i++) {
    s = (s ^ (s << 13)) >>> 0;
    s = (s ^ (s >>> 17)) >>> 0;
    s = (s ^ (s << 5)) >>> 0;
    out[at + i] = s & 255;
  }
}

function field(
  label: string,
  value: string | number,
  start: number,
  length: number,
  explain: string | null = null,
  children: PacketField[] = [],
): PacketField {
  return { label, value: String(value), explain, start, length, children };
}

// ---------- Building frames ----------

function ethHeader(a: B, srcMac: string, dstMac: string, type: number) {
  append(a, macBytes(dstMac));
  append(a, macBytes(srcMac));
  u16(a, type);
}

/** Ethernet + IPv4 around a transport header (plus `pad` more payload bytes that are not stored). */
function ipv4Frame(
  srcMac: string,
  dstMac: string,
  src: string,
  dst: string,
  proto: number,
  ttl: number,
  id: number,
  l4: B,
  pad: number,
  dontFragment = true,
): Uint8Array {
  const a: B = [];
  ethHeader(a, srcMac, dstMac, 0x0800);
  a.push(0x45, 0x00);
  u16(a, 20 + l4.length + pad);
  u16(a, id);
  u16(a, dontFragment ? 0x4000 : 0);
  a.push(ttl, proto, 0, 0);
  append(a, ipBytes(src));
  append(a, ipBytes(dst));
  let sum = 0;
  for (let i = 14; i < 34; i += 2) sum += (a[i] << 8) | a[i + 1];
  while (sum > 0xffff) sum = (sum & 0xffff) + (sum >>> 16);
  const check = ~sum & 0xffff;
  a[24] = check >>> 8;
  a[25] = check & 255;
  append(a, l4);
  return Uint8Array.from(a);
}

function tcpHeader(sp: number, dp: number, seq: number, ack: number, flags: number, win: number): B {
  const a: B = [];
  u16(a, sp);
  u16(a, dp);
  u32(a, seq);
  u32(a, ack);
  // Opening packets carry the usual Windows options: MSS, window scale, SACK permitted.
  const options = flags & SYN ? [2, 4, 0x05, 0xb4, 1, 3, 3, 8, 1, 1, 4, 2] : [];
  a.push(((20 + options.length) / 4) << 4, flags);
  u16(a, win);
  u16(a, rand16());
  u16(a, 0);
  append(a, options);
  return a;
}

function udpHeader(sp: number, dp: number, payloadLength: number): B {
  const a: B = [];
  u16(a, sp);
  u16(a, dp);
  u16(a, 8 + payloadLength);
  u16(a, rand16());
  return a;
}

function dnsName(a: B, name: string) {
  for (const label of name.split(".")) {
    a.push(label.length);
    ascii(a, label);
  }
  a.push(0);
}

function dnsQuery(id: number, name: string, qtype: number, flags: number): B {
  const a: B = [];
  u16(a, id);
  u16(a, flags);
  u16(a, 1);
  u16(a, 0);
  u16(a, 0);
  u16(a, 0);
  dnsName(a, name);
  u16(a, qtype);
  u16(a, 1);
  return a;
}

function dnsAnswer(id: number, name: string, ip: string, ttl: number): B {
  const a: B = [];
  u16(a, id);
  u16(a, 0x8180);
  u16(a, 1);
  u16(a, 1);
  u16(a, 0);
  u16(a, 0);
  dnsName(a, name);
  u16(a, 1);
  u16(a, 1);
  u16(a, 0xc00c);
  u16(a, 1);
  u16(a, 1);
  u32(a, ttl);
  u16(a, 4);
  append(a, ipBytes(ip));
  return a;
}

const SUITES = [0x1301, 0x1302, 0x1303, 0xc02b, 0xc02f, 0xc02c, 0xc030, 0xcca9];
const SUITE_NAMES: Record<number, string> = {
  0x1301: "TLS_AES_128_GCM_SHA256",
  0x1302: "TLS_AES_256_GCM_SHA384",
  0x1303: "TLS_CHACHA20_POLY1305_SHA256",
  0xc02b: "TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256",
  0xc02f: "TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256",
  0xc02c: "TLS_ECDHE_ECDSA_WITH_AES_256_GCM_SHA384",
  0xc030: "TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384",
  0xcca9: "TLS_ECDHE_ECDSA_WITH_CHACHA20_POLY1305_SHA256",
};
const EXT_NAMES: Record<number, string> = {
  0: "server_name",
  10: "supported_groups",
  16: "application_layer_protocol_negotiation",
  43: "supported_versions",
  51: "key_share",
};

function handshake(type: number, body: B, recordVersion: number): B {
  const rec: B = [0x16];
  u16(rec, recordVersion);
  u16(rec, body.length + 4);
  rec.push(type, (body.length >>> 16) & 255, (body.length >>> 8) & 255, body.length & 255);
  append(rec, body);
  return rec;
}

function clientHello(sni: string | null): B {
  const ext: B = [];
  if (sni) {
    u16(ext, 0x0000);
    u16(ext, sni.length + 5);
    u16(ext, sni.length + 3);
    ext.push(0);
    u16(ext, sni.length);
    ascii(ext, sni);
  }
  u16(ext, 0x000a);
  u16(ext, 6);
  u16(ext, 4);
  u16(ext, 0x001d);
  u16(ext, 0x0017);
  u16(ext, 0x0010);
  u16(ext, 14);
  u16(ext, 12);
  ext.push(2);
  ascii(ext, "h2");
  ext.push(8);
  ascii(ext, "http/1.1");
  u16(ext, 0x002b);
  u16(ext, 5);
  ext.push(4);
  u16(ext, 0x0304);
  u16(ext, 0x0303);
  u16(ext, 0x0033);
  u16(ext, 38);
  u16(ext, 36);
  u16(ext, 0x001d);
  u16(ext, 32);
  randomBytes(ext, 32);

  const body: B = [];
  u16(body, 0x0303);
  randomBytes(body, 32);
  body.push(32);
  randomBytes(body, 32);
  u16(body, SUITES.length * 2);
  for (const s of SUITES) u16(body, s);
  body.push(1, 0);
  u16(body, ext.length);
  append(body, ext);
  return handshake(1, body, 0x0301);
}

/** Server Hello, then Change Cipher Spec, then the header of an encrypted record whose body is `pad`. */
function serverHelloFlight(pad: number): B {
  const ext: B = [];
  u16(ext, 0x002b);
  u16(ext, 2);
  u16(ext, 0x0304);
  u16(ext, 0x0033);
  u16(ext, 36);
  u16(ext, 0x001d);
  u16(ext, 32);
  randomBytes(ext, 32);
  const body: B = [];
  u16(body, 0x0303);
  randomBytes(body, 32);
  body.push(32);
  randomBytes(body, 32);
  u16(body, 0x1301);
  body.push(0);
  u16(body, ext.length);
  append(body, ext);
  const a = handshake(2, body, 0x0303);
  a.push(0x14, 0x03, 0x03, 0x00, 0x01, 0x01);
  appDataHeader(a, pad);
  return a;
}

function appDataHeader(a: B, length: number) {
  a.push(0x17, 0x03, 0x03);
  u16(a, length);
}

function quicLong(type: 0 | 2, dcid: B, scid: B, pn: number, pad: number): B {
  const a: B = [0xc3 | (type << 4)];
  u32(a, 1);
  a.push(dcid.length);
  append(a, dcid);
  a.push(scid.length);
  append(a, scid);
  if (type === 0) a.push(0);
  u16(a, 0x4000 | (4 + pad));
  u32(a, pn);
  return a;
}

function quicShort(dcid: B, pn: number): B {
  const a: B = [0x43];
  append(a, dcid);
  u32(a, pn);
  return a;
}

function ntpTimestamp(a: B, ms: number) {
  u32(a, Math.floor(ms / 1000) + 2208988800);
  u32(a, Math.floor(((ms % 1000) / 1000) * 0x100000000));
}

// ---------- What is stored per packet ----------

interface HttpMeta {
  host: string | null;
  method: string | null;
  uri: string | null;
  code: number | null;
}

/** Everything the list and the filter need; the full bytes are rebuilt only for the details view. */
interface Draft {
  proto: string;
  mask: number;
  src: string;
  dst: string;
  sp: number | null;
  dp: number | null;
  sn: string | null;
  dn: string | null;
  esrc: string;
  edst: string;
  app: string | null;
  dir: "out" | "in" | null;
  flags: number;
  l4len: number;
  qname: string | null;
  dnsResp: boolean;
  sni: string | null;
  http: HttpMeta | null;
  info: string;
  summary: string;
}

interface Pkt extends Draft {
  n: number;
  t: number;
  len: number;
  /** Where the stored head bytes start in the pool, and how many. */
  off: number;
  hl: number;
  /** The rest of the frame is `len - hl` bytes made from this seed. */
  seed: number;
}

function draft(d: Partial<Draft> & Pick<Draft, "proto" | "mask" | "src" | "dst" | "info" | "summary">): Draft {
  return {
    sp: null,
    dp: null,
    sn: null,
    dn: null,
    esrc: PC_MAC,
    edst: ROUTER_MAC,
    app: null,
    dir: null,
    flags: 0,
    l4len: 0,
    qname: null,
    dnsResp: false,
    sni: null,
    http: null,
    ...d,
  };
}

/** Packet bytes live in big shared blocks instead of one small array each. */
class BytePool {
  static CHUNK = 1 << 22;
  private chunks: Uint8Array[] = [];
  private used = 0;

  store(bytes: Uint8Array): number {
    if (!this.chunks.length || this.used + bytes.length > BytePool.CHUNK) {
      this.chunks.push(new Uint8Array(BytePool.CHUNK));
      this.used = 0;
    }
    const index = this.chunks.length - 1;
    this.chunks[index].set(bytes, this.used);
    const off = index * BytePool.CHUNK + this.used;
    this.used += bytes.length;
    return off;
  }

  load(off: number, length: number): Uint8Array {
    const chunk = this.chunks[Math.floor(off / BytePool.CHUNK)];
    const at = off % BytePool.CHUNK;
    return chunk.subarray(at, at + length);
  }

  clear() {
    this.chunks = [];
    this.used = 0;
  }
}

// ---------- Conversations ----------

type Style = "tls" | "http" | "tcp" | "quic" | "udp" | "dns" | "ntp";

type Step =
  | "syn"
  | "synack"
  | "ack3"
  | "ch"
  | "sh"
  | "cfin"
  | "dataOut"
  | "dataIn"
  | "ackIn"
  | "ackOut"
  | "finOut"
  | "finIn"
  | "lastAck"
  | "rstIn"
  | "httpReq"
  | "httpResp"
  | "qInitOut"
  | "qInitIn"
  | "qHsOut"
  | "qShortOut"
  | "qShortIn"
  | "udpOut"
  | "udpIn"
  | "dnsX"
  | "ntpX";

interface Flow {
  key: string;
  style: Style;
  tcp: boolean;
  appId: string | null;
  app: string | null;
  lport: number;
  rip: string;
  rport: number;
  host: string | null;
  pending: Step[];
  /** The opening steps have been queued. */
  started: boolean;
  closing: boolean;
  dead: boolean;
  inWorld: boolean;
  synthetic: boolean;
  dnsDone: boolean;
  isnOut: number;
  isnIn: number;
  relOut: number;
  relIn: number;
  cidOut: B;
  cidIn: B;
  pnOut: number;
  pnIn: number;
  uri: string;
}

const PAGES = ["/", "/index.html", "/news/today", "/static/app.js", "/images/logo.png", "/api/headlines?page=2"];

function typeOfUri(uri: string) {
  if (uri.endsWith(".js")) return "application/javascript";
  if (uri.endsWith(".png")) return "image/png";
  if (uri.startsWith("/api")) return "application/json";
  return "text/html";
}

const MDNS_SERVICES: Array<[string, string]> = [
  ["_airplay._tcp.local", "AirPlay (screen and music sharing)"],
  ["_googlecast._tcp.local", "Chromecast (casting to a TV)"],
  ["_spotify-connect._tcp.local", "Spotify Connect (music on speakers)"],
];

function param(name: string): string | null {
  try {
    return new URLSearchParams(window.location.search).get(name);
  } catch {
    return null;
  }
}

// ---------- The engine ----------

export function createDemoCapture(world: DemoWorld): DemoCaptureMethods {
  const pool = new BytePool();
  let pkts: Pkt[] = [];
  let shownIdx: number[] = [];
  let matcher: ((p: Pkt) => boolean) | null = null;
  let flows = new Map<string, Flow>();
  const knownHosts = new Map<string, string>();
  const listeners = new Set<(status: CaptureStatus) => void>();
  let genTimer: number | null = null;
  let statusTimer: number | null = null;
  let lastT = 0;
  let tickBase = 0;
  let ipId = rand16();
  let bursting = false;
  let burstUsed = false;
  let firstSync = true;

  const status: CaptureStatus = {
    state: "idle",
    target: null,
    startedAt: null,
    stoppedAt: null,
    packets: 0,
    shown: 0,
    bytes: 0,
    dropped: 0,
    filter: "",
    filterError: null,
    limitReached: false,
    error: null,
    stopAfter: { packets: null, seconds: null, megabytes: null },
    stopReason: null,
  };

  const current = (): CaptureStatus => ({ ...status, packets: pkts.length, shown: shownIdx.length });
  const emitStatus = () => {
    const s = current();
    listeners.forEach((listener) => listener(s));
  };

  function support(): CaptureSupport {
    const mode = param("npcap");
    if (mode === "missing") return { available: false, npcapVersion: null, interfaces: [], problem: "Npcap is not installed." };
    if (mode === "admin")
      return { available: false, npcapVersion: "1.80", interfaces: [], problem: "Npcap is set to administrators only." };
    return { available: true, npcapVersion: "1.80", interfaces: INTERFACES.map((i) => ({ ...i, addresses: [...i.addresses] })), problem: null };
  }

  function nameOf(ip: string): string | null {
    if (ip === PC_IP) return "This PC";
    if (ip === ROUTER_IP) return "Your router";
    if (ip === MDNS_IP || ip === SSDP_IP) return EVERYONE;
    return knownHosts.get(ip) ?? null;
  }

  // ----- Which packets belong to the capture -----

  function targetAppName(t: Extract<CaptureTarget, { kind: "app" }>) {
    return world.appName(t.appId) || t.name;
  }

  function flowRelevant(f: Flow): boolean {
    const t = status.target;
    if (!t) return false;
    switch (t.kind) {
      case "all":
        return true;
      case "app":
        return f.appId === t.appId || f.app === targetAppName(t);
      case "host":
        return f.rip === t.ip;
      case "domain":
        return f.host === t.name;
      case "connection":
        return f.tcp === (t.protocol === "tcp") && f.lport === t.localPort && f.rip === t.remoteIp && f.rport === t.remotePort;
    }
  }

  function inTarget(d: Draft): boolean {
    const t = status.target;
    if (!t) return false;
    switch (t.kind) {
      case "all":
        return true;
      case "app":
        return d.app === targetAppName(t);
      case "host":
        return d.src === t.ip || d.dst === t.ip;
      case "domain":
        return d.sn === t.name || d.dn === t.name || d.qname === t.name;
      case "connection": {
        const proto = t.protocol === "tcp" ? P.tcp : P.udp;
        if (!(d.mask & proto)) return false;
        return (
          (d.src === t.remoteIp && d.sp === t.remotePort && d.dp === t.localPort) ||
          (d.dst === t.remoteIp && d.dp === t.remotePort && d.sp === t.localPort)
        );
      }
    }
  }

  // ----- Storing packets -----

  function emit(head: Uint8Array, pad: number, d: Draft) {
    if (status.state !== "running" || pkts.length >= LIMIT) return;
    if (!inTarget(d)) return;
    lastT = Math.max(lastT + 0.00001 + Math.random() * 0.012, tickBase);
    const p = d as Pkt;
    p.n = pkts.length + 1;
    p.t = lastT;
    p.len = head.length + pad;
    p.off = pool.store(head);
    p.hl = head.length;
    p.seed = rand32();
    pkts.push(p);
    status.bytes += p.len;
    if (!matcher || matcher(p)) shownIdx.push(pkts.length - 1);
  }

  function emitUdp(
    src: string,
    dst: string,
    esrc: string,
    edst: string,
    sp: number,
    dp: number,
    payload: B,
    pad: number,
    ttl: number,
    d: Omit<Partial<Draft>, "src" | "dst"> & Pick<Draft, "proto" | "mask" | "info" | "summary">,
  ) {
    const l4 = udpHeader(sp, dp, payload.length + pad);
    append(l4, payload);
    const bytes = ipv4Frame(esrc, edst, src, dst, 17, ttl, src === PC_IP ? ipId++ & 0xffff : rand16(), l4, pad, false);
    emit(
      bytes,
      pad,
      draft({ src, dst, sp, dp, esrc, edst, sn: nameOf(src), dn: nameOf(dst), dir: src === PC_IP ? "out" : "in", l4len: payload.length + pad, ...d }),
    );
  }

  // ----- Conversations: one step at a time -----

  function who(f: Flow) {
    return f.app ?? "This PC";
  }
  function peer(f: Flow) {
    return f.host ?? nameOf(f.rip) ?? f.rip;
  }

  function newFlow(
    key: string,
    tcp: boolean,
    rport: number,
    rip: string,
    lport: number,
    appId: string | null,
    app: string | null,
    host: string | null,
  ): Flow {
    const style: Style = tcp
      ? HTTP_PORTS.has(rport)
        ? "http"
        : TLS_PORTS.has(rport) || rport === 587
          ? "tls"
          : "tcp"
      : rport === 443
        ? "quic"
        : rport === 53
          ? "dns"
          : rport === 123
            ? "ntp"
            : "udp";
    const cid = () => {
      const a: B = [];
      randomBytes(a, 8);
      return a;
    };
    return {
      key,
      style,
      tcp,
      appId,
      app,
      lport,
      rip,
      rport,
      host,
      pending: [],
      started: false,
      closing: false,
      dead: false,
      inWorld: false,
      synthetic: false,
      dnsDone: false,
      isnOut: rand32(),
      isnIn: rand32(),
      relOut: 0,
      relIn: 0,
      cidOut: cid(),
      cidIn: cid(),
      pnOut: 0,
      pnIn: 0,
      uri: "/",
    };
  }

  function restart(f: Flow) {
    if (!(f.inWorld || f.synthetic) || f.closing) {
      f.dead = true;
      return;
    }
    f.started = false;
    f.dnsDone = Math.random() < 0.5;
    f.isnOut = rand32();
    f.isnIn = rand32();
    f.relOut = 0;
    f.relIn = 0;
    f.pnOut = 0;
    f.pnIn = 0;
    f.pending = [];
  }

  /** Queues the next few steps of a conversation. */
  function refill(f: Flow) {
    if (!f.started) {
      f.started = true;
      if (f.host && !f.dnsDone) {
        f.dnsDone = true;
        dnsLookup(f.host, f.rip);
      }
      if (f.style === "tls") f.pending = ["syn", "synack", "ack3", "ch", "ackIn", "sh", "ackOut", "cfin"];
      else if (f.style === "http" || f.style === "tcp") f.pending = ["syn", "synack", "ack3"];
      else if (f.style === "quic") f.pending = ["qInitOut", "qInitIn", "qInitIn", "qHsOut"];
      if (f.pending.length) return;
    }
    const r = Math.random();
    switch (f.style) {
      case "tls":
        if (r < 0.004) f.pending = ["rstIn"];
        else if (r < 0.014) f.pending = ["finOut", "finIn", "lastAck"];
        else if (r < 0.5) f.pending = ["dataOut", "ackIn", "dataIn", "dataIn", "ackOut"];
        else if (r < 0.8) f.pending = ["dataIn", "ackOut"];
        else f.pending = ["dataOut", "ackIn"];
        break;
      case "http":
        f.pending = r < 0.03 ? ["finOut", "finIn", "lastAck"] : ["httpReq", "ackIn", "httpResp", "ackOut"];
        break;
      case "tcp":
        f.pending = r < 0.02 ? ["finOut", "finIn", "lastAck"] : ["dataOut", "ackIn", "dataIn", "ackOut"];
        break;
      case "quic":
        f.pending = r < 0.5 ? ["qShortOut", "qShortIn", "qShortIn", "qShortOut"] : ["qShortIn", "qShortIn", "qShortIn", "qShortOut"];
        break;
      case "udp":
        f.pending = ["udpOut", "udpIn"];
        break;
      case "dns":
        f.pending = ["dnsX"];
        break;
      case "ntp":
        f.pending = ["ntpX"];
        break;
    }
  }

  function step(f: Flow) {
    if (!f.pending.length) refill(f);
    const next = f.pending.shift();
    if (next) run(f, next);
  }

  /** One TCP segment in either direction; keeps the sequence numbers right. */
  function tcpSegment(
    f: Flow,
    dir: "out" | "in",
    flags: number,
    payload: B,
    pad: number,
    over?: Partial<Draft> & { summary?: string },
  ) {
    const out = dir === "out";
    const n = payload.length + pad;
    const relSeq = out ? f.relOut : f.relIn;
    const relAck = out ? f.relIn : f.relOut;
    const seq = ((out ? f.isnOut : f.isnIn) + relSeq) >>> 0;
    const ack = flags & ACK ? ((out ? f.isnIn : f.isnOut) + relAck) >>> 0 : 0;
    const win = flags & SYN ? (out ? 64240 : 65535) : out ? rand(1020, 1030) : rand(500, 520);
    const sp = out ? f.lport : f.rport;
    const dp = out ? f.rport : f.lport;
    const l4 = tcpHeader(sp, dp, seq, ack, flags, win);
    append(l4, payload);
    const src = out ? PC_IP : f.rip;
    const dst = out ? f.rip : PC_IP;
    const bytes = ipv4Frame(out ? PC_MAC : ROUTER_MAC, out ? ROUTER_MAC : PC_MAC, src, dst, 6, out ? 128 : rand(50, 58), out ? ipId++ & 0xffff : rand16(), l4, pad);

    const consumed = n + (flags & (SYN | FIN) ? 1 : 0);
    if (out) f.relOut += consumed;
    else f.relIn += consumed;

    const names = ["FIN", "SYN", "RST", "PSH", "ACK"].filter((_, i) => flags & [FIN, SYN, RST, PSH, ACK][i]);
    let info = `${sp} → ${dp} [${names.join(", ")}] Seq=${relSeq}`;
    if (flags & ACK) info += ` Ack=${relAck}`;
    info += ` Win=${win} Len=${n}`;
    if (flags & SYN) info += " MSS=1460 WS=256 SACK_PERM";

    const me = who(f);
    const them = peer(f);
    let summary: string;
    if (flags & RST) summary = out ? `${me} cuts the connection to ${them} off abruptly` : `${them} cuts the connection off abruptly`;
    else if (flags & SYN) summary = flags & ACK ? `${them} agrees to open the connection` : `${me} asks ${them} to open a connection`;
    else if (flags & FIN) summary = out ? `${me} says it is finished with ${them}` : `${them} closes its side too`;
    else if (n > 0) summary = out ? `${me} sends ${size(n)} to ${them}` : `${them} sends ${size(n)} to ${me}`;
    else summary = out ? `${me} confirms it got the data from ${them}` : `${them} confirms it got the data`;

    emit(
      bytes,
      pad,
      draft({
        proto: "TCP",
        mask: TCP_IP,
        src,
        dst,
        sp,
        dp,
        sn: out ? "This PC" : them,
        dn: out ? them : "This PC",
        esrc: out ? PC_MAC : ROUTER_MAC,
        edst: out ? ROUTER_MAC : PC_MAC,
        app: f.app,
        dir,
        flags,
        l4len: n,
        info,
        summary,
        ...over,
      }),
    );
  }

  function tlsData(f: Flow, dir: "out" | "in", pad: number) {
    const head: B = [];
    appDataHeader(head, pad);
    const me = who(f);
    const them = peer(f);
    tcpSegment(f, dir, PSH | ACK, head, pad, {
      proto: "TLS",
      mask: TCP_IP | P.tls,
      info: "Application Data",
      summary: dir === "out" ? `Encrypted data from ${me} to ${them} (${size(pad)})` : `Encrypted data from ${them} to ${me} (${size(pad)})`,
    });
  }

  function quicPacket(f: Flow, dir: "out" | "in", head: B, pad: number, info: string, summary: string) {
    const out = dir === "out";
    const l4 = udpHeader(out ? f.lport : f.rport, out ? f.rport : f.lport, head.length + pad);
    append(l4, head);
    const src = out ? PC_IP : f.rip;
    const dst = out ? f.rip : PC_IP;
    const bytes = ipv4Frame(out ? PC_MAC : ROUTER_MAC, out ? ROUTER_MAC : PC_MAC, src, dst, 17, out ? 128 : rand(50, 58), out ? ipId++ & 0xffff : rand16(), l4, pad, false);
    emit(
      bytes,
      pad,
      draft({
        proto: "QUIC",
        mask: UDP_IP | P.quic,
        src,
        dst,
        sp: out ? f.lport : f.rport,
        dp: out ? f.rport : f.lport,
        sn: out ? "This PC" : peer(f),
        dn: out ? peer(f) : "This PC",
        esrc: out ? PC_MAC : ROUTER_MAC,
        edst: out ? ROUTER_MAC : PC_MAC,
        app: f.app,
        dir,
        l4len: head.length + pad,
        info,
        summary,
      }),
    );
  }

  function run(f: Flow, s: Step) {
    const me = who(f);
    const them = peer(f);
    switch (s) {
      case "syn":
        return tcpSegment(f, "out", SYN, [], 0);
      case "synack":
        return tcpSegment(f, "in", SYN | ACK, [], 0);
      case "ack3":
        return tcpSegment(f, "out", ACK, [], 0, { summary: `${me} confirms: the connection to ${them} is open` });
      case "ch":
        return tcpSegment(f, "out", PSH | ACK, clientHello(f.host), 0, {
          proto: "TLS",
          mask: TCP_IP | P.tls,
          sni: f.host,
          info: f.host ? `Client Hello (SNI=${f.host})` : "Client Hello",
          summary: `${me} starts a secure (encrypted) session with ${them}`,
        });
      case "sh": {
        const pad = rand(900, 1300);
        return tcpSegment(f, "in", PSH | ACK, serverHelloFlight(pad), pad, {
          proto: "TLS",
          mask: TCP_IP | P.tls,
          info: "Server Hello, Change Cipher Spec, Application Data",
          summary: `${them} agrees to the secure session and sends its certificate, already encrypted`,
        });
      }
      case "cfin": {
        const head: B = [0x14, 0x03, 0x03, 0x00, 0x01, 0x01];
        appDataHeader(head, 53);
        return tcpSegment(f, "out", PSH | ACK, head, 53, {
          proto: "TLS",
          mask: TCP_IP | P.tls,
          info: "Change Cipher Spec, Application Data",
          summary: `${me} finishes setting up encryption with ${them}`,
        });
      }
      case "dataOut":
        if (f.style === "tls") return tlsData(f, "out", rand(90, 700));
        return tcpSegment(f, "out", PSH | ACK, [], rand(40, 600), { mask: TCP_IP });
      case "dataIn":
        if (f.style === "tls") return tlsData(f, "in", rand(250, 1400));
        return tcpSegment(f, "in", PSH | ACK, [], rand(200, 1400));
      case "ackIn":
        return tcpSegment(f, "in", ACK, [], 0);
      case "ackOut":
        return tcpSegment(f, "out", ACK, [], 0);
      case "finOut":
        return tcpSegment(f, "out", FIN | ACK, [], 0);
      case "finIn":
        return tcpSegment(f, "in", FIN | ACK, [], 0);
      case "lastAck":
        tcpSegment(f, "out", ACK, [], 0, { summary: `The connection between ${me} and ${them} is closed` });
        return restart(f);
      case "rstIn":
        tcpSegment(f, "in", RST | ACK, [], 0);
        return restart(f);
      case "httpReq": {
        const upload = f.rport === 8080;
        f.uri = upload ? "/v2/upload" : pick(PAGES);
        const head: B = [];
        const pad = upload ? rand(600, 1200) : 0;
        const method = upload ? "POST" : "GET";
        let msg = `${method} ${f.uri} HTTP/1.1\r\nHost: ${f.host ?? f.rip}\r\nUser-Agent: Mozilla/5.0 (Windows NT 10.0; Win64; x64)\r\nAccept: */*\r\n`;
        if (upload) msg += `Content-Type: application/octet-stream\r\nContent-Length: ${pad}\r\n`;
        msg += "Connection: keep-alive\r\n\r\n";
        ascii(head, msg);
        return tcpSegment(f, "out", PSH | ACK, head, pad, {
          proto: "HTTP",
          mask: TCP_IP | P.http,
          http: { host: f.host ?? f.rip, method, uri: f.uri, code: null },
          info: `${method} ${f.uri} HTTP/1.1`,
          summary: upload
            ? `${me} uploads ${size(pad)} to ${them} without encryption`
            : `${me} asks ${them} for ${f.uri} (not encrypted)`,
        });
      }
      case "httpResp": {
        const upload = f.rport === 8080;
        const head: B = [];
        const pad = upload ? 0 : rand(300, 1150);
        const type = upload ? "application/json" : typeOfUri(f.uri);
        let msg = upload ? "HTTP/1.1 201 Created\r\n" : "HTTP/1.1 200 OK\r\n";
        msg += `Content-Type: ${type}\r\n`;
        if (!upload) msg += `Content-Encoding: gzip\r\nContent-Length: ${pad}\r\n`;
        else msg += "Content-Length: 0\r\n";
        msg += "Server: nginx\r\n\r\n";
        ascii(head, msg);
        const code = upload ? 201 : 200;
        return tcpSegment(f, "in", PSH | ACK, head, pad, {
          proto: "HTTP",
          mask: TCP_IP | P.http,
          http: { host: f.host ?? f.rip, method: null, uri: f.uri, code },
          info: `HTTP/1.1 ${code} ${upload ? "Created" : "OK"}  (${type})`,
          summary: upload
            ? `${them} confirms the upload arrived`
            : `${them} sends back ${f.uri} (${size(pad)}, not encrypted)`,
        });
      }
      case "qInitOut": {
        const pad = 1250 - 30;
        return quicPacket(
          f,
          "out",
          quicLong(0, f.cidOut, f.cidIn, f.pnOut++, pad),
          pad,
          `Initial, DCID=${hexBytes(f.cidOut)}, SCID=${hexBytes(f.cidIn)}, PKN: ${f.pnOut - 1}, CRYPTO, PADDING`,
          `${me} starts a QUIC (fast, encrypted) connection with ${them}`,
        );
      }
      case "qInitIn": {
        const pad = rand(1100, 1200);
        return quicPacket(
          f,
          "in",
          quicLong(0, f.cidIn, f.cidOut, f.pnIn++, pad),
          pad,
          `Initial, DCID=${hexBytes(f.cidIn)}, SCID=${hexBytes(f.cidOut)}, PKN: ${f.pnIn - 1}, ACK, CRYPTO`,
          `${them} answers the QUIC connection and sends its encrypted certificate`,
        );
      }
      case "qHsOut": {
        const pad = rand(50, 70);
        return quicPacket(
          f,
          "out",
          quicLong(2, f.cidOut, f.cidIn, f.pnOut++, pad),
          pad,
          `Handshake, DCID=${hexBytes(f.cidOut)}, SCID=${hexBytes(f.cidIn)}`,
          `${me} finishes setting up encryption with ${them}`,
        );
      }
      case "qShortOut": {
        const pad = rand(30, 600);
        return quicPacket(f, "out", quicShort(f.cidOut, f.pnOut++), pad, `Protected Payload (KP0), DCID=${hexBytes(f.cidOut)}`, `Encrypted QUIC data from ${me} to ${them} (${size(pad)})`);
      }
      case "qShortIn": {
        const pad = rand(200, 1350);
        return quicPacket(f, "in", quicShort(f.cidIn, f.pnIn++), pad, `Protected Payload (KP0), DCID=${hexBytes(f.cidIn)}`, `Encrypted QUIC data from ${them} to ${me} (${size(pad)})`);
      }
      case "udpOut":
      case "udpIn": {
        const out = s === "udpOut";
        const pad = out ? rand(40, 400) : rand(80, 1200);
        const sp = out ? f.lport : f.rport;
        const dp = out ? f.rport : f.lport;
        return emitUdp(out ? PC_IP : f.rip, out ? f.rip : PC_IP, out ? PC_MAC : ROUTER_MAC, out ? ROUTER_MAC : PC_MAC, sp, dp, [], pad, out ? 128 : 54, {
          proto: "UDP",
          mask: UDP_IP,
          app: f.app,
          sn: out ? "This PC" : them,
          dn: out ? them : "This PC",
          info: `${sp} → ${dp} Len=${pad}`,
          summary: out ? `${me} sends ${size(pad)} to ${them} over UDP` : `${them} sends ${size(pad)} to ${me} over UDP`,
        });
      }
      case "dnsX": {
        const names = [...new Set(knownHosts.values())];
        const name = names.length ? pick(names) : "example.com";
        const answer = [...knownHosts.entries()].find(([, h]) => h === name)?.[0] ?? addressFor(name);
        return dnsLookup(name, answer, f.rip, f.lport, f.app ?? SYSTEM);
      }
      case "ntpX":
        return ntpExchange(f.rip, f.host ?? nameOf(f.rip), f.lport, f.app ?? SYSTEM);
    }
  }

  // ----- Traffic that is not part of an app's connection -----

  function dnsLookup(name: string, answer: string, server = ROUTER_IP, sport = rand(49152, 65000), app = SYSTEM) {
    const id = rand16();
    const serverName = nameOf(server) ?? server;
    const asker = app === SYSTEM ? "Windows" : app;
    emitUdp(PC_IP, server, PC_MAC, ROUTER_MAC, sport, 53, dnsQuery(id, name, 1, 0x0100), 0, 128, {
      proto: "DNS",
      mask: UDP_IP | P.dns,
      app,
      qname: name,
      info: `Standard query 0x${hex(id, 4)} A ${name}`,
      summary: `${asker} looks up the address of ${name}`,
    });
    emitUdp(server, PC_IP, ROUTER_MAC, PC_MAC, 53, sport, dnsAnswer(id, name, answer, rand(60, 300)), 0, 64, {
      proto: "DNS",
      mask: UDP_IP | P.dns,
      app,
      qname: name,
      dnsResp: true,
      info: `Standard query response 0x${hex(id, 4)} A ${name} A ${answer}`,
      summary: server === ROUTER_IP ? `The router answers: ${name} is at ${answer}` : `${serverName} answers: ${name} is at ${answer}`,
    });
  }

  function ntpExchange(server: string, name: string | null, sport: number, app: string) {
    const label = name ?? server;
    const now = Date.now();
    const ask: B = [0x1b, 0, 0, 0];
    for (let i = 0; i < 36; i++) ask.push(0);
    ntpTimestamp(ask, now);
    emitUdp(PC_IP, server, PC_MAC, ROUTER_MAC, sport, 123, ask, 0, 128, {
      proto: "NTP",
      mask: UDP_IP | P.ntp,
      app,
      dn: label,
      info: "NTP Version 3, client",
      summary: `${app === SYSTEM ? "Windows" : app} asks ${label} for the exact time`,
    });
    const reply: B = [0x1c, 2, 0x11, 0xe9];
    u32(reply, 0x00000210);
    u32(reply, 0x00000a3c);
    append(reply, [192, 0, 2, 7]);
    ntpTimestamp(reply, now - 40_000);
    append(reply, ask.slice(40, 48));
    ntpTimestamp(reply, now + 12);
    ntpTimestamp(reply, now + 13);
    emitUdp(server, PC_IP, ROUTER_MAC, PC_MAC, 123, sport, reply, 0, 54, {
      proto: "NTP",
      mask: UDP_IP | P.ntp,
      app,
      sn: label,
      info: "NTP Version 3, server",
      summary: `${label} sends back the exact time`,
    });
  }

  function arpFrame(esrc: string, edst: string, op: number, sMac: string, sIp: string, tMac: string, tIp: string) {
    const a: B = [];
    ethHeader(a, esrc, edst, 0x0806);
    u16(a, 1);
    u16(a, 0x0800);
    a.push(6, 4);
    u16(a, op);
    append(a, macBytes(sMac));
    append(a, ipBytes(sIp));
    append(a, macBytes(tMac));
    append(a, ipBytes(tIp));
    return Uint8Array.from(a);
  }

  function arpPair() {
    const fromTv = Math.random() < 0.35;
    const askerMac = fromTv ? TV_MAC : PC_MAC;
    const askerIp = fromTv ? TV_IP : PC_IP;
    const wantedIp = fromTv ? PC_IP : ROUTER_IP;
    const ownerMac = fromTv ? PC_MAC : ROUTER_MAC;
    emit(arpFrame(askerMac, BROADCAST_MAC, 1, askerMac, askerIp, ZERO_MAC, wantedIp), 0, draft({
      proto: "ARP",
      mask: P.eth | P.arp,
      src: askerMac,
      dst: BROADCAST_MAC,
      sn: fromTv ? null : "This PC",
      dn: EVERYONE,
      esrc: askerMac,
      edst: BROADCAST_MAC,
      dir: fromTv ? "in" : "out",
      info: `Who has ${wantedIp}? Tell ${askerIp}`,
      summary: fromTv
        ? `A device on your network (${TV_IP}) asks who has ${PC_IP}, this PC's address`
        : `This PC asks who on the local network has ${ROUTER_IP}`,
    }));
    emit(arpFrame(ownerMac, askerMac, 2, ownerMac, wantedIp, askerMac, askerIp), 0, draft({
      proto: "ARP",
      mask: P.eth | P.arp,
      src: ownerMac,
      dst: askerMac,
      sn: fromTv ? "This PC" : "Your router",
      dn: fromTv ? null : "This PC",
      esrc: ownerMac,
      edst: askerMac,
      dir: fromTv ? "out" : "in",
      info: `${wantedIp} is at ${ownerMac}`,
      summary: fromTv ? `This PC answers: ${PC_IP} is me` : `The router answers: it has ${ROUTER_IP}`,
    }));
  }

  let pingSeq = 1;
  function pingPair() {
    const seq = pingSeq++;
    const make = (type: number, src: string, dst: string, ttl: number) => {
      const l4: B = [type, 0];
      u16(l4, rand16());
      u16(l4, 1);
      u16(l4, seq);
      ascii(l4, "abcdefghijklmnopqrstuvwabcdefghi");
      return ipv4Frame(src === PC_IP ? PC_MAC : ROUTER_MAC, src === PC_IP ? ROUTER_MAC : PC_MAC, src, dst, 1, ttl, src === PC_IP ? ipId++ & 0xffff : rand16(), l4, 0, false);
    };
    const swapped = ((seq & 255) << 8) | (seq >>> 8);
    emit(make(8, PC_IP, ROUTER_IP, 128), 0, draft({
      proto: "ICMP",
      mask: P.eth | P.ip | P.icmp,
      src: PC_IP,
      dst: ROUTER_IP,
      sn: "This PC",
      dn: "Your router",
      dir: "out",
      l4len: 40,
      info: `Echo (ping) request  id=0x0001, seq=${seq}/${swapped}, ttl=128`,
      summary: "This PC pings your router to check it is reachable",
    }));
    emit(make(0, ROUTER_IP, PC_IP, 64), 0, draft({
      proto: "ICMP",
      mask: P.eth | P.ip | P.icmp,
      src: ROUTER_IP,
      dst: PC_IP,
      sn: "Your router",
      dn: "This PC",
      esrc: ROUTER_MAC,
      edst: PC_MAC,
      dir: "in",
      l4len: 40,
      info: `Echo (ping) reply    id=0x0001, seq=${seq}/${swapped}, ttl=64`,
      summary: "Your router answers the ping",
    }));
  }

  function mdnsQuery() {
    const [service, plain] = pick(MDNS_SERVICES);
    emitUdp(TV_IP, MDNS_IP, TV_MAC, MDNS_MAC, 5353, 5353, dnsQuery(0, service, 12, 0), 0, 255, {
      proto: "MDNS",
      mask: UDP_IP | P.dns | P.mdns,
      qname: service,
      sn: null,
      info: `Standard query 0x0000 PTR ${service}, "QM" question`,
      summary: `A device on your network asks who offers ${plain}`,
    });
  }

  function ssdpSearch() {
    const head: B = [];
    ascii(
      head,
      'M-SEARCH * HTTP/1.1\r\nHOST: 239.255.255.250:1900\r\nMAN: "ssdp:discover"\r\nMX: 1\r\nST: urn:dial-multiscreen-org:service:dial:1\r\n\r\n',
    );
    emitUdp(PC_IP, SSDP_IP, PC_MAC, SSDP_MAC, rand(50000, 60000), 1900, head, 0, 4, {
      proto: "SSDP",
      mask: UDP_IP | P.ssdp,
      app: SYSTEM,
      info: "M-SEARCH * HTTP/1.1",
      summary: "Windows looks for media devices (like smart TVs and speakers) on your network",
    });
  }

  function timeServer(): [string, string] {
    for (const [ip, host] of knownHosts) if (host.includes("time")) return [ip, host];
    knownHosts.set("192.0.2.123", "time.windows.example");
    return ["192.0.2.123", "time.windows.example"];
  }

  function noise() {
    const r = () => Math.random();
    if (r() < 0.03 && knownHosts.size) {
      const [ip, host] = pick([...knownHosts.entries()]);
      dnsLookup(host, ip);
    }
    if (r() < 0.008) arpPair();
    if (r() < 0.012) mdnsQuery();
    if (r() < 0.005) ssdpSearch();
    if (r() < 0.004) pingPair();
    if (r() < 0.004) {
      const [ip, host] = timeServer();
      ntpExchange(ip, host, 123, SYSTEM);
    }
  }

  // ----- Keeping up with the pretend monitor -----

  function sync() {
    const seen = new Set<string>();
    for (const c of world.connections()) {
      if (c.host) knownHosts.set(c.remoteIp, c.host);
      const key = `${c.protocol}:${c.localPort}:${c.remoteIp}:${c.remotePort}`;
      seen.add(key);
      if (flows.has(key)) continue;
      const host = c.host ?? knownHosts.get(c.remoteIp) ?? null;
      const f = newFlow(key, c.protocol === "tcp", c.remotePort, c.remoteIp, c.localPort, c.appId, world.appName(c.appId), host);
      f.inWorld = true;
      // Old connections are joined half-way; new ones (and a few old ones, so the
      // first screen shows how connections open) start from the beginning.
      const fresh = !bursting && status.startedAt !== null && c.openedAt >= status.startedAt - 1500;
      if (!fresh && !(firstSync && Math.random() < 0.25)) {
        f.started = true;
        f.dnsDone = true;
      }
      flows.set(key, f);
    }
    firstSync = false;
    for (const f of flows.values()) {
      if (!f.inWorld || seen.has(f.key) || f.closing) continue;
      f.inWorld = false;
      f.closing = true;
      if (f.tcp && f.started) f.pending = ["finOut", "finIn", "lastAck"];
      else f.dead = true;
    }
  }

  function syntheticFlow(t: CaptureTarget): Flow | null {
    const conns = world.connections();
    let f: Flow;
    switch (t.kind) {
      case "all":
        return null;
      case "app": {
        const name = targetAppName(t);
        const host = name === SYSTEM ? "settings.windows.example" : `api.${name.toLowerCase().replace(/[^a-z0-9]+/g, "")}.example`;
        const ip = addressFor(host);
        f = newFlow(`syn:app:${t.appId}`, true, 443, ip, rand(49152, 65000), t.appId, name, host);
        break;
      }
      case "host": {
        const conn = conns.find((c) => c.remoteIp === t.ip);
        const host = t.name ?? knownHosts.get(t.ip) ?? null;
        f = newFlow(`syn:host:${t.ip}`, true, conn?.remotePort ?? 443, t.ip, rand(49152, 65000), conn?.appId ?? null, conn ? world.appName(conn.appId) : null, host);
        break;
      }
      case "domain": {
        const conn = conns.find((c) => c.host === t.name);
        const ip = conn?.remoteIp ?? [...knownHosts.entries()].find(([, h]) => h === t.name)?.[0] ?? addressFor(t.name);
        f = newFlow(`syn:domain:${t.name}`, true, conn?.remotePort ?? 443, ip, rand(49152, 65000), conn?.appId ?? null, conn ? world.appName(conn.appId) : null, t.name);
        break;
      }
      case "connection": {
        const conn = conns.find((c) => c.remoteIp === t.remoteIp && c.remotePort === t.remotePort && c.localPort === t.localPort);
        f = newFlow(
          `syn:conn:${t.protocol}:${t.localPort}:${t.remoteIp}:${t.remotePort}`,
          t.protocol === "tcp",
          t.remotePort,
          t.remoteIp,
          t.localPort,
          conn?.appId ?? null,
          t.appName,
          t.host,
        );
        // The connection is already open, so join it half-way.
        f.started = true;
        f.dnsDone = true;
        break;
      }
    }
    f.synthetic = true;
    if (f.host) knownHosts.set(f.rip, f.host);
    return f;
  }

  function poisson(mean: number) {
    const limit = Math.exp(-mean);
    let k = 0;
    let p = Math.random();
    while (p > limit) {
      k++;
      p *= Math.random();
    }
    return k;
  }

  /** Moves the pretend network on by one tick. `relMs` is time since the capture started. */
  function tick(relMs: number, mean: number) {
    const target = status.target;
    if (!target || status.state !== "running") return;
    tickBase = Math.max(0, (relMs - TICK_MS) / 1000);
    sync();

    // Connections that just closed say goodbye straight away.
    for (const f of flows.values()) {
      if (!f.closing || f.dead) continue;
      if (flowRelevant(f)) while (f.pending.length && !f.dead) step(f);
      f.dead = true;
    }
    for (const [key, f] of flows) if (f.dead) flows.delete(key);

    let relevant = [...flows.values()].filter((f) => !f.closing && flowRelevant(f));
    if (!relevant.length) {
      const f = syntheticFlow(target);
      if (f) {
        flows.set(f.key, f);
        relevant = [f];
      }
    }
    const steps = poisson(target.kind === "all" ? mean : mean * 0.6);
    for (let i = 0; i < steps && relevant.length; i++) {
      const f = pick(relevant);
      if (!f.dead) step(f);
    }
    noise();

    if (pkts.length >= LIMIT) stopCapture(true);
    else checkStopAfter(relMs);
  }

  /** Stops when one of the "stop automatically after" limits is reached. */
  function checkStopAfter(relMs: number) {
    const { packets, seconds, megabytes } = status.stopAfter;
    let reason: string | null = null;
    if (seconds !== null && relMs >= seconds * 1000) reason = `Stopped after ${plural(seconds, "second")}, as you asked.`;
    else if (packets !== null && pkts.length >= packets) reason = `Stopped after ${plural(packets, "packet")}, as you asked.`;
    else if (megabytes !== null && status.bytes >= megabytes * 1_000_000) reason = `Stopped after ${megabytes} MB, as you asked.`;
    if (reason) {
      status.stopReason = reason;
      stopCapture(false);
    }
  }

  function stopTimers() {
    if (genTimer !== null) window.clearInterval(genTimer);
    if (statusTimer !== null) window.clearInterval(statusTimer);
    genTimer = null;
    statusTimer = null;
  }

  function stopCapture(limit: boolean) {
    stopTimers();
    status.state = "stopped";
    status.stoppedAt = Date.now();
    status.limitReached = limit;
    if (!bursting) emitStatus();
  }

  function reset() {
    stopTimers();
    pkts = [];
    shownIdx = [];
    pool.clear();
    flows = new Map();
    lastT = 0;
    tickBase = 0;
    firstSync = true;
    Object.assign(status, {
      state: "idle",
      target: null,
      startedAt: null,
      stoppedAt: null,
      bytes: 0,
      dropped: 0,
      limitReached: false,
      error: null,
      stopReason: null,
    });
  }

  function burst(n: number) {
    bursting = true;
    let relMs = 0;
    while (status.state === "running" && pkts.length < n) {
      relMs += TICK_MS;
      tick(relMs, 10);
    }
    bursting = false;
    status.startedAt = Date.now() - relMs;
    if (status.state === "stopped") status.stoppedAt = Date.now();
  }

  // ----- Display filter -----

  function applyFilter(text: string) {
    const compiled = compileFilter(text);
    matcher = compiled;
    status.filter = text.trim();
    status.filterError = null;
    shownIdx = [];
    for (let i = 0; i < pkts.length; i++) if (!compiled || compiled(pkts[i])) shownIdx.push(i);
  }

  // ----- Rows and details -----

  function row(p: Pkt): PacketRow {
    return {
      number: p.n,
      time: p.t,
      source: p.src,
      destination: p.dst,
      sourceName: p.sn,
      destinationName: p.dn,
      sourcePort: p.sp,
      destinationPort: p.dp,
      protocol: p.proto,
      length: p.len,
      info: p.info,
      summary: p.summary,
      app: p.app,
      direction: p.dir,
    };
  }

  function bytesOf(p: Pkt): Uint8Array {
    const out = new Uint8Array(p.len);
    out.set(pool.load(p.off, p.hl), 0);
    fillPad(p.seed, out, p.hl, p.len - p.hl);
    return out;
  }

  function fileName(ext: string) {
    const d = new Date();
    const pad2 = (v: number) => String(v).padStart(2, "0");
    return `C:\\Users\\you\\Documents\\Portlight\\capture-${d.getFullYear()}-${pad2(d.getMonth() + 1)}-${pad2(d.getDate())}-${pad2(d.getHours())}${pad2(d.getMinutes())}.${ext}`;
  }

  const delay = (ms: number) => new Promise((resolve) => window.setTimeout(resolve, ms));

  return {
    captureSupport: async () => support(),

    captureStart: async (opts) => {
      const s = support();
      if (!s.available) throw s.problem ?? "Packet capture is not available on this PC.";
      if (opts.interfaces.length && !opts.interfaces.some((id) => s.interfaces.find((i) => i.id === id)?.active)) {
        throw 'That network card is not connected, so there is nothing to capture on it. Pick Wi-Fi or "All active network cards".';
      }
      const bpf = opts.captureFilter.trim();
      if (/==|&&|\|\||\btls\b|\bdns\.|\bip\.addr\b/.test(bpf)) {
        throw 'That looks like a search filter, not a capture filter. Capture filters use Wireshark\'s capture-filter syntax, like "tcp port 443" or "host 192.168.1.1". Or leave it empty and use the search box.';
      }
      reset();
      status.state = "running";
      status.target = opts.target;
      status.stopAfter = opts.stopAfter ?? { packets: null, seconds: null, megabytes: null };
      status.startedAt = Date.now();

      const wanted = Number(param("burst"));
      if (wanted > 0 && !burstUsed) {
        burstUsed = true;
        burst(Math.min(wanted, LIMIT));
      }
      if (status.state === "running") {
        genTimer = window.setInterval(() => tick(Date.now() - (status.startedAt ?? Date.now()), 2.2), TICK_MS);
        statusTimer = window.setInterval(emitStatus, STATUS_MS);
      }
      emitStatus();
      return current();
    },

    captureStop: async () => {
      if (status.state === "running") stopCapture(false);
      return current();
    },

    captureClear: async () => {
      reset();
      emitStatus();
      return current();
    },

    captureStatus: async () => current(),

    onCaptureStatus: async (onStatus) => {
      listeners.add(onStatus);
      return () => void listeners.delete(onStatus);
    },

    captureSetFilter: async (filter) => {
      try {
        applyFilter(filter);
      } catch (error) {
        if (!(error instanceof FilterError)) throw error;
        status.filterError = error.message;
      }
      emitStatus();
      return current();
    },

    captureRows: async (start, count) => {
      await Promise.resolve();
      const from = Math.max(0, Math.floor(start));
      const to = Math.min(shownIdx.length, from + Math.max(0, Math.floor(count)));
      const rows: PacketRow[] = [];
      for (let i = from; i < to; i++) rows.push(row(pkts[shownIdx[i]]));
      return rows;
    },

    capturePacket: async (number) => {
      const p = pkts[number - 1];
      if (!p) throw `Packet ${number} is not in this capture.`;
      const bytes = bytesOf(p);
      const timestamp = (status.startedAt ?? Date.now()) + p.t * 1000;
      const detail: PacketDetail = {
        number: p.n,
        time: p.t,
        timestamp,
        length: p.len,
        capturedLength: p.len,
        interfaceName: CARD,
        app: p.app,
        layers: decode(p, bytes, timestamp),
        bytes: Array.from(bytes),
      };
      return detail;
    },

    captureSave: async () => {
      if (!pkts.length) throw "There are no packets to save yet.";
      await delay(400);
      return fileName("pcapng");
    },

    captureExportCsv: async () => {
      if (!pkts.length) throw "There are no packets to export yet.";
      await delay(300);
      return fileName("csv");
    },

    captureExport: async (format, selection) => {
      if (!pkts.length) throw "There are no packets to export yet.";
      if (selection.kind === "shown" && !shownIdx.length) throw "No packets match the search, so there is nothing to export.";
      await delay(350);
      const ext = format === "text" ? "txt" : format;
      return fileName(ext);
    },

    openNpcapDownload: async () => {
      window.open("https://npcap.com/#download", "_blank", "noopener");
    },
  };
}

// ---------- Explaining a packet, layer by layer ----------

function decode(p: Pkt, b: Uint8Array, timestamp: number): PacketLayer[] {
  const layers: PacketLayer[] = [];
  const d = new Date(timestamp);
  const ms = String(Math.floor(timestamp % 1000)).padStart(3, "0");

  const frame: PacketLayer = {
    id: "frame",
    title: `Frame ${p.n}`,
    summary: `${p.len} bytes on the wire, captured on ${CARD}`,
    explain: "The packet as it arrived at your network card, before any layer is unpacked.",
    start: 0,
    length: b.length,
    fields: [
      field("Arrival time", `${d.toLocaleDateString()} ${d.toLocaleTimeString()}.${ms}`, 0, 0),
      field("Time since capture started", `${p.t.toFixed(6)} seconds`, 0, 0),
      field("Length on the wire", `${p.len} bytes`, 0, 0),
      field("Captured length", `${b.length} bytes`, 0, 0, "Portlight keeps the whole packet."),
      field("Network card", CARD, 0, 0),
      field("App", p.app ?? "Not known", 0, 0, "Portlight matches packets to apps using the connection list Windows keeps."),
    ],
  };
  layers.push(frame);

  const macName = (mac: string) =>
    mac === PC_MAC ? " (This PC)" : mac === ROUTER_MAC ? " (Your router)" : mac === BROADCAST_MAC ? " (everyone: broadcast)" : mac.startsWith("01:00:5e") ? " (a group: multicast)" : "";
  const dstMac = macAt(b, 0);
  const srcMac = macAt(b, 6);
  const etherType = rd16(b, 12);
  layers.push({
    id: "eth",
    title: "Ethernet II",
    summary: `From ${srcMac}${macName(srcMac)} to ${dstMac}${macName(dstMac)}`,
    explain:
      "The local delivery label: which device on your own network (Wi-Fi or cable) the packet is handed to next. For internet traffic that is usually your router.",
    start: 0,
    length: 14,
    fields: [
      field("Destination", `${dstMac}${macName(dstMac)}`, 0, 6, "The hardware address of the device that should pick this up."),
      field("Source", `${srcMac}${macName(srcMac)}`, 6, 6, "The hardware address of the device that sent it."),
      field("Type", etherType === 0x0806 ? "ARP (0x0806)" : "IPv4 (0x0800)", 12, 2, "What kind of packet is inside."),
    ],
  });

  if (etherType === 0x0806) {
    layers.push(decodeArp(b));
  } else if (etherType === 0x0800) {
    const ihl = (b[14] & 0x0f) * 4;
    const total = rd16(b, 16);
    const proto = b[23];
    const end = Math.min(b.length, 14 + total);
    const l4 = 14 + ihl;
    layers.push(decodeIp(b, ihl, total, proto));
    if (proto === 1) layers.push(decodeIcmp(b, l4, end));
    else if (proto === 6) {
      const tcp = decodeTcp(b, l4, end);
      layers.push(tcp.layer);
      if (tcp.payload < end) {
        if (p.mask & P.tls) layers.push(decodeTls(b, tcp.payload, end));
        else if (p.mask & P.http) layers.push(decodeText(b, tcp.payload, end, "http"));
        else layers.push(decodeData(tcp.payload, end));
      }
    } else if (proto === 17) {
      layers.push(decodeUdp(b, l4));
      const payload = l4 + 8;
      if (payload < end) {
        if (p.mask & (P.dns | P.mdns)) layers.push(decodeDns(b, payload, end, (p.mask & P.mdns) !== 0));
        else if (p.mask & P.quic) layers.push(decodeQuic(b, payload, end));
        else if (p.mask & P.ntp) layers.push(decodeNtp(b, payload));
        else if (p.mask & P.ssdp) layers.push(decodeText(b, payload, end, "ssdp"));
        else layers.push(decodeData(payload, end));
      }
    }
  }

  frame.fields.push(field("Protocols in frame", layers.slice(1).map((l) => l.id).join(":"), 0, 0));
  return layers;
}

function hostLabel(ip: string) {
  if (ip === PC_IP) return `${ip} (This PC)`;
  if (ip === ROUTER_IP) return `${ip} (Your router)`;
  if (ip === MDNS_IP || ip === SSDP_IP) return `${ip} (${EVERYONE.toLowerCase()})`;
  return ip;
}

function decodeArp(b: Uint8Array): PacketLayer {
  const op = rd16(b, 20);
  const senderIp = ipAt(b, 28);
  const targetIp = ipAt(b, 38);
  return {
    id: "arp",
    title: "Address Resolution Protocol",
    summary: op === 1 ? `Who has ${targetIp}? Tell ${senderIp}` : `${senderIp} is at ${macAt(b, 22)}`,
    explain:
      "Finds which device on your local network owns an address, so packets can be handed to it. It never leaves your home network.",
    start: 14,
    length: 28,
    fields: [
      field("Hardware type", `Ethernet (${rd16(b, 14)})`, 14, 2),
      field("Protocol type", `IPv4 (0x${hex(rd16(b, 16), 4)})`, 16, 2),
      field("Hardware size", b[18], 18, 1),
      field("Protocol size", b[19], 19, 1),
      field("Opcode", op === 1 ? "request (1)" : "reply (2)", 20, 2, op === 1 ? "A question to everyone." : "An answer to one device."),
      field("Sender hardware address", macAt(b, 22), 22, 6),
      field("Sender IP address", hostLabel(senderIp), 28, 4),
      field("Target hardware address", macAt(b, 32), 32, 6, op === 1 ? "All zeros: this is what is being asked for." : null),
      field("Target IP address", hostLabel(targetIp), 38, 4),
    ],
  };
}

function decodeIp(b: Uint8Array, ihl: number, total: number, proto: number): PacketLayer {
  const src = ipAt(b, 26);
  const dst = ipAt(b, 30);
  const flags = b[20];
  const protoName = proto === 6 ? "TCP (6)" : proto === 17 ? "UDP (17)" : proto === 1 ? "ICMP (1)" : String(proto);
  return {
    id: "ip",
    title: "Internet Protocol version 4",
    summary: `From ${src} to ${dst}`,
    explain:
      "The internet address label: which computer the packet comes from and which one it is going to. Routers along the way read only this part.",
    start: 14,
    length: ihl,
    fields: [
      field("Version", b[14] >> 4, 14, 1, "IPv4, the classic internet address format."),
      field("Header length", `${ihl} bytes`, 14, 1),
      field("Differentiated services", `0x${hex(b[15], 2)}`, 15, 1, "Priority hints. Almost always 0."),
      field("Total length", `${total} bytes`, 16, 2, "This layer plus everything inside it."),
      field("Identification", `0x${hex(rd16(b, 18), 4)} (${rd16(b, 18)})`, 18, 2, "Helps put the pieces back together if the packet is split up on the way."),
      field("Flags", `0x${hex(flags >> 5, 1)}${flags & 0x40 ? ", Don't fragment" : ""}`, 20, 1, null, [
        field("Don't fragment", flags & 0x40 ? "Set" : "Not set", 20, 1, "Asks routers not to split this packet into smaller pieces."),
        field("More fragments", flags & 0x20 ? "Set" : "Not set", 20, 1, "Set when more pieces of the same packet follow."),
      ]),
      field("Fragment offset", rd16(b, 20) & 0x1fff, 20, 2),
      field("Time to live", b[22], 22, 1, "How many more routers the packet may pass through before it is thrown away. Stops packets from looping forever."),
      field("Protocol", protoName, 23, 1, "What is inside this layer."),
      field("Header checksum", `0x${hex(rd16(b, 24), 4)}`, 24, 2, "A quick sum to spot damage on the way."),
      field("Source address", hostLabel(src), 26, 4, "Where the packet comes from."),
      field("Destination address", hostLabel(dst), 30, 4, "Where the packet is going."),
    ],
  };
}

function portExplain(port: number, other: number) {
  const known = PORTS[port];
  if (known) return `The service: ${known[1]}.`;
  if (port >= 49152 || (PORTS[other] && port > 1024)) return "A temporary number picked for this one conversation.";
  return null;
}

function decodeTcp(b: Uint8Array, o: number, end: number): { layer: PacketLayer; payload: number } {
  const sp = rd16(b, o);
  const dp = rd16(b, o + 2);
  const hl = (b[o + 12] >> 4) * 4;
  const flags = b[o + 13];
  const n = Math.max(0, end - (o + hl));
  const flagDefs: Array<[number, string, string]> = [
    [SYN, "SYN", "Asks to open a connection."],
    [ACK, "ACK", "Confirms data that has arrived."],
    [PSH, "PSH", "Asks the other side to hand the data to the app now."],
    [FIN, "FIN", "This side has finished sending."],
    [RST, "RST", "Cuts the connection off at once."],
  ];
  const set = flagDefs.filter(([bit]) => flags & bit).map(([, name]) => name);

  const options: PacketField[] = [];
  for (let p = o + 20; p < o + hl; ) {
    const kind = b[p];
    if (kind === 1) {
      options.push(field("No-operation", "padding", p, 1));
      p += 1;
      continue;
    }
    const len = Math.max(2, b[p + 1]);
    if (kind === 2) options.push(field("Maximum segment size", `${rd16(b, p + 2)} bytes`, p, len, "The biggest piece of data this side wants in one packet."));
    else if (kind === 3) options.push(field("Window scale", `${b[p + 2]} (multiply by ${1 << b[p + 2]})`, p, len, "Lets the window grow past 64 kB."));
    else if (kind === 4) options.push(field("SACK permitted", "Yes", p, len, "Lets the other side say exactly which pieces went missing."));
    else options.push(field(`Option ${kind}`, `${len} bytes`, p, len));
    p += len;
  }

  const fields = [
    field("Source port", portLabel(sp), o, 2, portExplain(sp, dp)),
    field("Destination port", portLabel(dp), o + 2, 2, portExplain(dp, sp)),
    field("Sequence number", rd32(b, o + 4), o + 4, 4, "Counts the bytes sent, so the other side can put them in order and notice gaps."),
    field("Acknowledgment number", flags & ACK ? rd32(b, o + 8) : 0, o + 8, 4, "Tells the other side how much has arrived safely."),
    field("Header length", `${hl} bytes`, o + 12, 1),
    field(
      "Flags",
      `0x${hex(flags, 3)} (${set.join(", ")})`,
      o + 12,
      2,
      "Short signals that steer the conversation.",
      flagDefs.map(([bit, name, explain]) => field(name, flags & bit ? "Set" : "Not set", o + 13, 1, explain)),
    ),
    field("Window", rd16(b, o + 14), o + 14, 2, "How much more data this side can take in right now."),
    field("Checksum", `0x${hex(rd16(b, o + 16), 4)} (not checked)`, o + 16, 2, "A sum to spot damage. Windows often fills it in later, on the network card."),
    field("Urgent pointer", rd16(b, o + 18), o + 18, 2),
  ];
  if (options.length) fields.push(field("Options", `${hl - 20} bytes`, o + 20, hl - 20, null, options));
  if (n > 0) fields.push(field("TCP payload", `${n} bytes`, o + hl, n));

  return {
    payload: o + hl,
    layer: {
      id: "tcp",
      title: "Transmission Control Protocol",
      summary: `Port ${sp} → ${dp}, ${set.join(", ")}, ${n} bytes of data`,
      explain:
        "TCP keeps a reliable two-way conversation between two programs: it numbers the data, resends anything lost and says when the conversation opens and closes.",
      start: o,
      length: hl,
      fields,
    },
  };
}

function decodeUdp(b: Uint8Array, o: number): PacketLayer {
  const sp = rd16(b, o);
  const dp = rd16(b, o + 2);
  return {
    id: "udp",
    title: "User Datagram Protocol",
    summary: `Port ${sp} → ${dp}, ${rd16(b, o + 4) - 8} bytes of data`,
    explain:
      "UDP sends single messages with no set-up and no delivery guarantee. It is quick, so it is used for name lookups, video calls, games and QUIC.",
    start: o,
    length: 8,
    fields: [
      field("Source port", portLabel(sp), o, 2, portExplain(sp, dp)),
      field("Destination port", portLabel(dp), o + 2, 2, portExplain(dp, sp)),
      field("Length", `${rd16(b, o + 4)} bytes`, o + 4, 2, "This layer plus its data."),
      field("Checksum", `0x${hex(rd16(b, o + 6), 4)} (not checked)`, o + 6, 2),
    ],
  };
}

function decodeIcmp(b: Uint8Array, o: number, end: number): PacketLayer {
  const type = b[o];
  const name = type === 8 ? "Echo (ping) request" : type === 0 ? "Echo (ping) reply" : `Type ${type}`;
  return {
    id: "icmp",
    title: "Internet Control Message Protocol",
    summary: `${name}, sequence ${rd16(b, o + 6)}`,
    explain: "Network housekeeping messages: \"are you there?\" pings, and \"could not deliver\" warnings.",
    start: o,
    length: end - o,
    fields: [
      field("Type", `${type} (${name})`, o, 1),
      field("Code", b[o + 1], o + 1, 1),
      field("Checksum", `0x${hex(rd16(b, o + 2), 4)}`, o + 2, 2),
      field("Identifier", `0x${hex(rd16(b, o + 4), 4)}`, o + 4, 2, "Tells pings from different programs apart."),
      field("Sequence number", rd16(b, o + 6), o + 6, 2, "Counts the pings, so lost ones show up."),
      field("Data", `${end - o - 8} bytes ("${text(b, o + 8, Math.min(12, end - o - 8))}…")`, o + 8, end - o - 8, "Filler. Windows always sends the alphabet."),
    ],
  };
}

const DNS_TYPES: Record<number, string> = { 1: "A (IPv4 address)", 12: "PTR (service list)", 28: "AAAA (IPv6 address)", 5: "CNAME (alias)" };

/** Reads a DNS name, following compression pointers relative to the message start. */
function readName(b: Uint8Array, base: number, p: number): { name: string; end: number } {
  const labels: string[] = [];
  let end = -1;
  for (let guard = 0; guard < 40 && p < b.length; guard++) {
    const len = b[p];
    if (len === 0) {
      if (end < 0) end = p + 1;
      break;
    }
    if ((len & 0xc0) === 0xc0) {
      if (end < 0) end = p + 2;
      p = base + (((len & 0x3f) << 8) | b[p + 1]);
      continue;
    }
    labels.push(text(b, p + 1, len));
    p += 1 + len;
  }
  return { name: labels.join("."), end: end < 0 ? p : end };
}

function decodeDns(b: Uint8Array, o: number, end: number, mdns: boolean): PacketLayer {
  const id = rd16(b, o);
  const flags = rd16(b, o + 2);
  const response = (flags & 0x8000) !== 0;
  const qd = rd16(b, o + 4);
  const an = rd16(b, o + 6);

  const flagKids = [
    field("Response", response ? "Message is a response" : "Message is a query", o + 2, 1),
    field("Opcode", "Standard query (0)", o + 2, 1),
  ];
  if (!mdns) flagKids.push(field("Recursion desired", flags & 0x0100 ? "Do the whole lookup for me" : "No", o + 2, 1, "Asks the server to find the answer itself, asking other servers if needed."));
  if (response) {
    flagKids.push(field("Recursion available", flags & 0x0080 ? "Yes" : "No", o + 3, 1));
    flagKids.push(field("Reply code", (flags & 0x0f) === 0 ? "No error (0)" : `Error (${flags & 0x0f})`, o + 3, 1));
  }

  let p = o + 12;
  const queries: PacketField[] = [];
  let firstName = "";
  for (let i = 0; i < qd && p < end; i++) {
    const { name, end: after } = readName(b, o, p);
    if (!firstName) firstName = name;
    const qtype = rd16(b, after);
    const qclass = rd16(b, after + 2);
    queries.push(
      field(name, `${DNS_TYPES[qtype] ?? qtype}, class IN`, p, after + 4 - p, null, [
        field("Name", name, p, after - p, "The name being looked up."),
        field("Type", DNS_TYPES[qtype] ?? String(qtype), after, 2, qtype === 1 ? "Asks for an IPv4 address." : null),
        field("Class", (qclass & 0x7fff) === 1 ? (mdns ? 'IN (internet), "QM" question' : "IN (internet)") : String(qclass), after + 2, 2),
      ]),
    );
    p = after + 4;
  }
  const queriesStart = o + 12;
  const queriesEnd = p;

  const answers: PacketField[] = [];
  let firstAnswer = "";
  for (let i = 0; i < an && p + 10 <= end; i++) {
    const { name, end: after } = readName(b, o, p);
    const type = rd16(b, after);
    const ttl = rd32(b, after + 4);
    const rdlen = rd16(b, after + 8);
    const data = after + 10;
    const value = type === 1 && rdlen === 4 ? ipAt(b, data) : `${rdlen} bytes`;
    if (!firstAnswer) firstAnswer = value;
    answers.push(
      field(name, `${DNS_TYPES[type] ?? type}: ${value}`, p, data + rdlen - p, null, [
        field("Name", name, p, after - p, after - p === 2 ? "A pointer back to the name in the question, to save space." : null),
        field("Type", DNS_TYPES[type] ?? String(type), after, 2),
        field("Class", "IN (internet)", after + 2, 2),
        field("Time to live", `${ttl} seconds`, after + 4, 4, "How long this answer may be remembered before asking again."),
        field("Data length", rdlen, after + 8, 2),
        field("Address", value, data, rdlen, "The answer: the address to connect to."),
      ]),
    );
    p = data + rdlen;
  }

  const fields = [
    field("Transaction ID", `0x${hex(id, 4)}`, o, 2, "Matches each answer to its question."),
    field("Flags", `0x${hex(flags, 4)} ${response ? "Standard query response" : "Standard query"}`, o + 2, 2, null, flagKids),
    field("Questions", qd, o + 4, 2),
    field("Answer RRs", an, o + 6, 2),
    field("Authority RRs", rd16(b, o + 8), o + 8, 2),
    field("Additional RRs", rd16(b, o + 10), o + 10, 2),
    field("Queries", qd, queriesStart, queriesEnd - queriesStart, "The question being asked.", queries),
  ];
  if (answers.length) fields.push(field("Answers", an, queriesEnd, p - queriesEnd, "The answer from the server.", answers));

  return {
    id: mdns ? "mdns" : "dns",
    title: mdns ? "Multicast Domain Name System" : "Domain Name System",
    summary: mdns
      ? `Asks everyone nearby: who offers ${firstName}?`
      : response
        ? `Answer: ${firstName} is at ${firstAnswer}`
        : `Question: what is the address of ${firstName}?`,
    explain: mdns
      ? "Multicast DNS lets devices on your home network find each other by name, without a server. It never leaves your home network."
      : "The internet's phone book: turns a name like github.com into an address computers can use.",
    start: o,
    length: end - o,
    fields,
  };
}

const TLS_TYPES: Record<number, string> = { 20: "Change Cipher Spec", 21: "Alert", 22: "Handshake", 23: "Application Data" };

function decodeTls(b: Uint8Array, o: number, end: number): PacketLayer {
  const fields: PacketField[] = [];
  const names: string[] = [];
  let sni: string | null = null;
  let encrypted = 0;
  for (let r = o; r + 5 <= end; ) {
    const type = b[r];
    const version = rd16(b, r + 1);
    const len = rd16(b, r + 3);
    const kids = [
      field("Content type", `${TLS_TYPES[type] ?? "Unknown"} (${type})`, r, 1),
      field("Version", `TLS 1.${version === 0x0301 ? 0 : 2} (0x${hex(version, 4)})`, r + 1, 2, "Records always show an old version number for compatibility. The real version is agreed inside."),
      field("Length", `${len} bytes`, r + 3, 2),
    ];
    let label = TLS_TYPES[type] ?? "Unknown";
    if (type === 22) {
      const hsType = b[r + 5];
      const hsLen = (b[r + 6] << 16) | (b[r + 7] << 8) | b[r + 8];
      const hsName = hsType === 1 ? "Client Hello" : hsType === 2 ? "Server Hello" : `Handshake ${hsType}`;
      label = hsName;
      kids.push(field("Handshake type", `${hsName} (${hsType})`, r + 5, 1));
      kids.push(field("Handshake length", `${hsLen} bytes`, r + 6, 3));
      const found = decodeHello(b, r + 9, Math.min(end, r + 9 + hsLen), hsType === 1, kids);
      if (found) sni = found;
    } else if (type === 20) {
      kids.push(field("Change cipher spec message", "1", r + 5, 1, "A leftover signal from older TLS versions. It means \"encryption starts now\"."));
    } else if (type === 23) {
      encrypted += len;
      kids.push(field("Encrypted application data", `${len} bytes`, r + 5, Math.min(len, end - r - 5), ENCRYPTED));
    }
    names.push(label);
    fields.push(field(`Record: ${label}`, `${len} bytes`, r, Math.min(5 + len, end - r), null, kids));
    r += 5 + len;
  }
  const summary = sni
    ? `Client Hello for ${sni}`
    : names[0] === "Server Hello"
      ? "Server Hello: encryption agreed (TLS 1.3)"
      : names.length === 1 && names[0] === "Application Data"
        ? `Encrypted data, ${size(encrypted)}`
        : names.join(", ");
  return {
    id: "tls",
    title: "Transport Layer Security",
    summary,
    explain:
      "TLS is the encryption behind HTTPS. After a short greeting where both sides agree on keys, everything is scrambled: you can see who is talking and how much, but not what they say.",
    start: o,
    length: end - o,
    fields,
  };
}

/** Client or Server Hello body. Returns the server name when there is one. */
function decodeHello(b: Uint8Array, p: number, end: number, client: boolean, out: PacketField[]): string | null {
  let sni: string | null = null;
  out.push(field("Version", "TLS 1.2 (0x0303)", p, 2, "Kept at 1.2 for old middleboxes. TLS 1.3 is agreed in an extension."));
  p += 2;
  out.push(field("Random", `${hexBytes(b.subarray(p, p + 8))}…`, p, 32, "32 random bytes that make every session's keys unique."));
  p += 32;
  const sid = b[p];
  out.push(field("Session ID length", sid, p, 1));
  out.push(field("Session ID", `${hexBytes(b.subarray(p + 1, p + 9))}…`, p + 1, sid));
  p += 1 + sid;
  if (client) {
    const csLen = rd16(b, p);
    const suites: PacketField[] = [];
    for (let i = 0; i < csLen; i += 2) {
      const s = rd16(b, p + 2 + i);
      suites.push(field("Cipher suite", `${SUITE_NAMES[s] ?? "Unknown"} (0x${hex(s, 4)})`, p + 2 + i, 2));
    }
    out.push(field("Cipher suites", `${csLen / 2} offered`, p, 2 + csLen, "The encryption methods the app can use, best first.", suites));
    p += 2 + csLen;
    const compLen = b[p];
    out.push(field("Compression methods", "none", p, 1 + compLen));
    p += 1 + compLen;
  } else {
    const s = rd16(b, p);
    out.push(field("Cipher suite", `${SUITE_NAMES[s] ?? "Unknown"} (0x${hex(s, 4)})`, p, 2, "The encryption method the server picked."));
    p += 2;
    out.push(field("Compression method", "none", p, 1));
    p += 1;
  }
  const extLen = rd16(b, p);
  const extStart = p;
  const exts: PacketField[] = [];
  for (let e = p + 2; e + 4 <= Math.min(end, p + 2 + extLen); ) {
    const type = rd16(b, e);
    const len = rd16(b, e + 2);
    const name = EXT_NAMES[type] ?? `unknown (${type})`;
    const kids = [field("Type", `${name} (${type})`, e, 2), field("Length", len, e + 2, 2)];
    let value = `${len} bytes`;
    let explain: string | null = null;
    if (type === 0) {
      const nameLen = rd16(b, e + 7);
      sni = text(b, e + 9, nameLen);
      value = sni;
      explain = "Which site the app wants.";
      kids.push(field("Server name list length", rd16(b, e + 4), e + 4, 2));
      kids.push(field("Server name type", "host_name (0)", e + 6, 1));
      kids.push(field("Server name length", nameLen, e + 7, 2));
      kids.push(
        field(
          "Server name (SNI)",
          sni,
          e + 9,
          nameLen,
          "The site name the app asks for. It is sent before encryption starts, which is how Portlight (and your network) can see which site this is.",
        ),
      );
    } else if (type === 43) {
      value = client ? "TLS 1.3, TLS 1.2" : "TLS 1.3";
      explain = client ? "The TLS versions the app can speak." : "The version both sides will use.";
    } else if (type === 16) {
      value = "h2, http/1.1";
      explain = "Which web protocol to speak once encrypted: HTTP/2 or HTTP/1.1.";
    } else if (type === 10) {
      value = "x25519, secp256r1";
      explain = "The kinds of key exchange the app supports.";
    } else if (type === 51) {
      value = "x25519";
      explain = "This side's half of the shared secret key.";
    }
    exts.push(field(`Extension: ${name}`, value, e, 4 + len, explain, kids));
    e += 4 + len;
  }
  out.push(field("Extensions", `${exts.length}`, extStart, 2 + extLen, "Extra settings and wishes.", exts));
  return sni;
}

function decodeText(b: Uint8Array, o: number, end: number, kind: "http" | "ssdp"): PacketLayer {
  const fields: PacketField[] = [];
  let p = o;
  let first = "";
  let bodyAt = end;
  while (p < end) {
    let q = p;
    while (q + 1 < end && !(b[q] === 13 && b[q + 1] === 10)) q++;
    if (q + 1 >= end) break;
    const line = text(b, p, q - p);
    if (!line) {
      bodyAt = q + 2;
      break;
    }
    if (!first) {
      first = line;
      const parts = line.split(" ");
      const kids: PacketField[] = [];
      let at = p;
      const isResponse = line.startsWith("HTTP/");
      const labels = isResponse ? ["Version", "Status code", "Reason"] : ["Method", "Address (URI)", "Version"];
      const values = isResponse ? [parts[0], parts[1], parts.slice(2).join(" ")] : parts;
      values.forEach((value, i) => {
        if (!value) return;
        kids.push(field(labels[i] ?? "Part", value, at, value.length));
        at += value.length + 1;
      });
      fields.push(field(isResponse ? "Status line" : "Request line", line, p, q + 2 - p, null, kids));
    } else {
      const colon = line.indexOf(":");
      const name = colon > 0 ? line.slice(0, colon) : line;
      const value = colon > 0 ? line.slice(colon + 1).trim() : "";
      fields.push(field(name, value, p, q + 2 - p, headerExplain(name)));
    }
    p = q + 2;
  }
  if (bodyAt < end) {
    fields.push(field("Body (gzip-compressed)", `${end - bodyAt} bytes`, bodyAt, end - bodyAt, "The page itself, packed to save space. It is not encrypted, so anyone on the way could unpack it."));
  }
  return kind === "http"
    ? {
        id: "http",
        title: "Hypertext Transfer Protocol",
        summary: first,
        explain: "HTTP is how web pages are asked for and sent. This one is not encrypted, so anyone on the way could read it.",
        start: o,
        length: end - o,
        fields,
      }
    : {
        id: "ssdp",
        title: "Simple Service Discovery Protocol",
        summary: first,
        explain: "How devices on your home network (TVs, speakers, printers) announce themselves and find each other.",
        start: o,
        length: end - o,
        fields,
      };
}

function headerExplain(name: string): string | null {
  switch (name.toLowerCase()) {
    case "host":
      return "The site being asked.";
    case "user-agent":
      return "The app introduces itself.";
    case "content-type":
      return "What kind of content this is.";
    case "content-length":
      return "How big the content is.";
    case "content-encoding":
      return "How the content was packed.";
    case "st":
      return "The kind of device being looked for.";
    default:
      return null;
  }
}

function decodeQuic(b: Uint8Array, o: number, end: number): PacketLayer {
  const first = b[o];
  const fields: PacketField[] = [];
  let summary: string;
  if (first & 0x80) {
    const type = (first >> 4) & 3;
    const typeName = ["Initial", "0-RTT", "Handshake", "Retry"][type];
    fields.push(
      field(`First byte`, `0x${hex(first, 2)} (long header, ${typeName})`, o, 1, null, [
        field("Header form", "Long header (1)", o, 1, "Long headers are used while the connection is being set up."),
        field("Fixed bit", "1", o, 1),
        field("Packet type", `${typeName} (${type})`, o, 1),
      ]),
    );
    fields.push(field("Version", `1 (0x${hex(rd32(b, o + 1), 8)})`, o + 1, 4));
    let p = o + 5;
    const dl = b[p];
    fields.push(field("Destination connection ID length", dl, p, 1));
    fields.push(field("Destination connection ID", hexBytes(b.subarray(p + 1, p + 1 + dl)), p + 1, dl, "A label for the connection that stays the same even if your network changes."));
    p += 1 + dl;
    const sl = b[p];
    fields.push(field("Source connection ID length", sl, p, 1));
    fields.push(field("Source connection ID", hexBytes(b.subarray(p + 1, p + 1 + sl)), p + 1, sl));
    p += 1 + sl;
    if (type === 0) {
      fields.push(field("Token length", b[p], p, 1));
      p += 1 + b[p];
    }
    fields.push(field("Length", rd16(b, p) & 0x3fff, p, 2));
    p += 2;
    fields.push(field("Packet number", `${rd32(b, p)} (protected)`, p, 4));
    p += 4;
    fields.push(field("Protected payload", `${end - p} bytes`, p, end - p, ENCRYPTED));
    summary = `${typeName} packet, connection ${hexBytes(b.subarray(o + 6, o + 6 + dl))}`;
  } else {
    fields.push(field("First byte", `0x${hex(first, 2)} (short header)`, o, 1, "Short headers are used once the connection is set up."));
    fields.push(field("Destination connection ID", hexBytes(b.subarray(o + 1, o + 9)), o + 1, 8));
    fields.push(field("Packet number", `${rd32(b, o + 9)} (protected)`, o + 9, 4));
    fields.push(field("Protected payload", `${end - o - 13} bytes`, o + 13, end - o - 13, ENCRYPTED));
    summary = `Encrypted data, ${size(end - o - 13)}`;
  }
  return {
    id: "quic",
    title: "QUIC",
    summary,
    explain:
      "QUIC is a newer, faster way to carry encrypted web traffic over UDP (it powers HTTP/3). Almost everything, even most of the header, is encrypted.",
    start: o,
    length: end - o,
    fields,
  };
}

function ntpTime(b: Uint8Array, o: number) {
  const seconds = rd32(b, o);
  if (!seconds) return "(not set)";
  return new Date((seconds - 2208988800) * 1000 + (rd32(b, o + 4) / 0x100000000) * 1000).toISOString().replace("T", " ").replace("Z", " UTC");
}

function decodeNtp(b: Uint8Array, o: number): PacketLayer {
  const first = b[o];
  const mode = first & 7;
  const modeName = mode === 3 ? "client" : mode === 4 ? "server" : String(mode);
  return {
    id: "ntp",
    title: "Network Time Protocol",
    summary: mode === 3 ? "Asks for the exact time" : `The exact time: ${ntpTime(b, o + 40)}`,
    explain: "Keeps your PC's clock right by asking a time server.",
    start: o,
    length: 48,
    fields: [
      field("Flags", `0x${hex(first, 2)}`, o, 1, null, [
        field("Leap indicator", first >> 6, o, 1, "Warns of a leap second. 0 means none."),
        field("Version", (first >> 3) & 7, o, 1),
        field("Mode", `${modeName} (${mode})`, o, 1),
      ]),
      field("Stratum", b[o + 1], o + 1, 1, "How many steps this clock is from an atomic clock."),
      field("Poll", b[o + 2], o + 2, 1),
      field("Precision", b[o + 3], o + 3, 1),
      field("Root delay", `0x${hex(rd32(b, o + 4), 8)}`, o + 4, 4),
      field("Root dispersion", `0x${hex(rd32(b, o + 8), 8)}`, o + 8, 4),
      field("Reference ID", ipAt(b, o + 12), o + 12, 4),
      field("Reference time", ntpTime(b, o + 16), o + 16, 8),
      field("Origin time", ntpTime(b, o + 24), o + 24, 8),
      field("Receive time", ntpTime(b, o + 32), o + 32, 8),
      field("Transmit time", ntpTime(b, o + 40), o + 40, 8, "When this message was sent."),
    ],
  };
}

function decodeData(o: number, end: number): PacketLayer {
  return {
    id: "data",
    title: "Data",
    summary: `${end - o} bytes`,
    explain: "Bytes Portlight does not know how to read. They may be encrypted or in an app's own format.",
    start: o,
    length: end - o,
    fields: [field("Data", `${end - o} bytes`, o, end - o)],
  };
}

// ---------- The display filter language ----------

class FilterError extends Error {}

const fail = (message: string): never => {
  throw new FilterError(message);
};

type FieldType = "num" | "ip" | "str" | "bool" | "host";
type Value = string | number;

interface FieldDef {
  type: FieldType;
  get(p: Pkt): Value[];
}

const has = (p: Pkt, bit: number) => (p.mask & bit) !== 0;
const ipOnly = (p: Pkt, v: string) => (has(p, P.ip) ? [v] : []);
const flag = (bit: number) => (p: Pkt) => (has(p, P.tcp) ? [p.flags & bit ? 1 : 0] : []);
const ports = (bit: number, which: "both" | "src" | "dst") => (p: Pkt): Value[] => {
  if (!has(p, bit) || p.sp === null || p.dp === null) return [];
  return which === "both" ? [p.sp, p.dp] : which === "src" ? [p.sp] : [p.dp];
};
const opt = (v: string | number | null | undefined): Value[] => (v === null || v === undefined ? [] : [v]);

const FIELDS: Record<string, FieldDef> = {
  "frame.number": { type: "num", get: (p) => [p.n] },
  "frame.len": { type: "num", get: (p) => [p.len] },
  "eth.src": { type: "str", get: (p) => [p.esrc] },
  "eth.dst": { type: "str", get: (p) => [p.edst] },
  "eth.addr": { type: "str", get: (p) => [p.esrc, p.edst] },
  "ip.src": { type: "ip", get: (p) => ipOnly(p, p.src) },
  "ip.dst": { type: "ip", get: (p) => ipOnly(p, p.dst) },
  "ip.addr": { type: "ip", get: (p) => (has(p, P.ip) ? [p.src, p.dst] : []) },
  "ipv6.src": { type: "str", get: () => [] },
  "ipv6.dst": { type: "str", get: () => [] },
  "ipv6.addr": { type: "str", get: () => [] },
  "tcp.port": { type: "num", get: ports(P.tcp, "both") },
  "tcp.srcport": { type: "num", get: ports(P.tcp, "src") },
  "tcp.dstport": { type: "num", get: ports(P.tcp, "dst") },
  "tcp.len": { type: "num", get: (p) => (has(p, P.tcp) ? [p.l4len] : []) },
  "tcp.flags.syn": { type: "bool", get: flag(SYN) },
  "tcp.flags.ack": { type: "bool", get: flag(ACK) },
  "tcp.flags.fin": { type: "bool", get: flag(FIN) },
  "tcp.flags.reset": { type: "bool", get: flag(RST) },
  "tcp.flags.push": { type: "bool", get: flag(PSH) },
  "udp.port": { type: "num", get: ports(P.udp, "both") },
  "udp.srcport": { type: "num", get: ports(P.udp, "src") },
  "udp.dstport": { type: "num", get: ports(P.udp, "dst") },
  "dns.qry.name": { type: "str", get: (p) => opt(has(p, P.dns) ? p.qname : null) },
  "dns.flags.response": { type: "bool", get: (p) => (has(p, P.dns) ? [p.dnsResp ? 1 : 0] : []) },
  "tls.handshake.extensions_server_name": { type: "str", get: (p) => opt(p.sni) },
  "tls.sni": { type: "str", get: (p) => opt(p.sni) },
  "http.host": { type: "str", get: (p) => opt(p.http?.host) },
  "http.request.method": { type: "str", get: (p) => opt(p.http?.method) },
  "http.request.uri": { type: "str", get: (p) => opt(p.http?.method ? p.http.uri : null) },
  "http.response.code": { type: "num", get: (p) => opt(p.http?.code) },
  app: { type: "str", get: (p) => opt(p.app) },
  port: { type: "num", get: (p) => (has(p, P.tcp | P.udp) && p.sp !== null && p.dp !== null ? [p.sp, p.dp] : []) },
  host: {
    type: "host",
    get: (p) => [p.src, p.dst, p.sn, p.dn, p.qname, p.sni, p.http?.host].filter((v): v is string => !!v),
  },
};

const PROTOCOLS: Record<string, number> = {
  eth: P.eth,
  arp: P.arp,
  ip: P.ip,
  ipv6: P.ipv6,
  icmp: P.icmp,
  icmpv6: P.icmpv6,
  tcp: P.tcp,
  udp: P.udp,
  dns: P.dns,
  tls: P.tls,
  http: P.http,
  quic: P.quic,
  dhcp: P.dhcp,
  mdns: P.mdns,
  ntp: P.ntp,
  ssdp: P.ssdp,
  frame: P.eth,
};

const OPS: Record<string, string> = {
  "==": "==",
  eq: "==",
  "!=": "!=",
  ne: "!=",
  ">": ">",
  gt: ">",
  "<": "<",
  lt: "<",
  ">=": ">=",
  ge: ">=",
  "<=": "<=",
  le: "<=",
  contains: "contains",
};

interface Token {
  kind: "word" | "str" | "op" | "lp" | "rp" | "and" | "or" | "not" | "end";
  text: string;
  pos: number;
}

function tokenize(input: string): Token[] {
  const tokens: Token[] = [];
  let i = 0;
  while (i < input.length) {
    const ch = input[i];
    const pos = i + 1;
    if (/\s/.test(ch)) {
      i++;
    } else if (ch === "(") {
      tokens.push({ kind: "lp", text: ch, pos });
      i++;
    } else if (ch === ")") {
      tokens.push({ kind: "rp", text: ch, pos });
      i++;
    } else if (ch === '"' || ch === "'") {
      let j = i + 1;
      let value = "";
      while (j < input.length && input[j] !== ch) {
        if (input[j] === "\\" && j + 1 < input.length) j++;
        value += input[j];
        j++;
      }
      if (j >= input.length) fail(`The quote at character ${pos} is never closed.`);
      tokens.push({ kind: "str", text: value, pos });
      i = j + 1;
    } else if (input.startsWith("&&", i) || ch === "&") {
      const len = input.startsWith("&&", i) ? 2 : 1;
      tokens.push({ kind: "and", text: input.slice(i, i + len), pos });
      i += len;
    } else if (input.startsWith("||", i) || ch === "|") {
      const len = input.startsWith("||", i) ? 2 : 1;
      tokens.push({ kind: "or", text: input.slice(i, i + len), pos });
      i += len;
    } else if (/^(==|!=|>=|<=)/.test(input.slice(i, i + 2))) {
      tokens.push({ kind: "op", text: input.slice(i, i + 2), pos });
      i += 2;
    } else if (ch === ">" || ch === "<") {
      tokens.push({ kind: "op", text: ch, pos });
      i++;
    } else if (ch === "=") {
      // A single "=" is a common slip; read it as "==".
      tokens.push({ kind: "op", text: "==", pos });
      i++;
    } else if (ch === "!") {
      tokens.push({ kind: "not", text: ch, pos });
      i++;
    } else {
      let j = i;
      while (j < input.length && !/[\s()"'&|=!<>]/.test(input[j])) j++;
      const word = input.slice(i, j);
      const lower = word.toLowerCase();
      if (lower === "and") tokens.push({ kind: "and", text: word, pos });
      else if (lower === "or") tokens.push({ kind: "or", text: word, pos });
      else if (lower === "not") tokens.push({ kind: "not", text: word, pos });
      else if (OPS[lower] && lower !== "==") tokens.push({ kind: "op", text: lower, pos });
      else tokens.push({ kind: "word", text: word, pos });
      i = j;
    }
  }
  tokens.push({ kind: "end", text: "", pos: input.length + 1 });
  return tokens;
}

function distance(a: string, b: string) {
  const row = Array.from({ length: b.length + 1 }, (_, i) => i);
  for (let i = 1; i <= a.length; i++) {
    let prev = row[0];
    row[0] = i;
    for (let j = 1; j <= b.length; j++) {
      const keep = row[j];
      row[j] = Math.min(row[j] + 1, row[j - 1] + 1, prev + (a[i - 1] === b[j - 1] ? 0 : 1));
      prev = keep;
    }
  }
  return row[b.length];
}

function suggestion(word: string): string | null {
  let best: string | null = null;
  let bestScore = Infinity;
  for (const name of [...Object.keys(FIELDS), ...Object.keys(PROTOCOLS)]) {
    const score = distance(word, name);
    if (score < bestScore) {
      best = name;
      bestScore = score;
    }
  }
  return best && bestScore <= (word.length > 8 ? 3 : 2) ? best : null;
}

function unknownField(tok: Token): never {
  const lower = tok.text.toLowerCase();
  const guess = suggestion(lower);
  return fail(`"${tok.text}" (at character ${tok.pos}) is not a field I know.${guess ? ` Did you mean ${guess}?` : ""}`);
}

function parseCidr(text: string): { base: number; mask: number } | null {
  const m = /^(\d{1,3})\.(\d{1,3})\.(\d{1,3})\.(\d{1,3})(?:\/(\d{1,2}))?$/.exec(text.trim());
  if (!m) return null;
  const parts = m.slice(1, 5).map(Number);
  if (parts.some((x) => x > 255)) return null;
  const bits = m[5] === undefined ? 32 : Number(m[5]);
  if (bits > 32) return null;
  const mask = bits === 0 ? 0 : (0xffffffff << (32 - bits)) >>> 0;
  const base = (((parts[0] << 24) | (parts[1] << 16) | (parts[2] << 8) | parts[3]) >>> 0) & mask;
  return { base: base >>> 0, mask };
}

function ipNumber(text: string): number | null {
  const parts = text.split(".");
  if (parts.length !== 4) return null;
  const n = parts.map(Number);
  if (n.some((x) => !Number.isInteger(x) || x < 0 || x > 255)) return null;
  return ((n[0] << 24) | (n[1] << 16) | (n[2] << 8) | n[3]) >>> 0;
}

function inCidr(value: Value, cidr: { base: number; mask: number }) {
  const n = typeof value === "string" ? ipNumber(value) : null;
  return n !== null && ((n & cidr.mask) >>> 0) === cidr.base;
}

function compare(name: string, def: FieldDef, op: string, valueTok: Token): (p: Pkt) => boolean {
  const raw = valueTok.text;
  const at = `(at character ${valueTok.pos})`;
  switch (def.type) {
    case "num": {
      if (op === "contains") fail(`"contains" works on text, but ${name} is a number ${at}. Try ${name} == ${raw}.`);
      const v = /^0x[0-9a-f]+$/i.test(raw) ? parseInt(raw, 16) : Number(raw);
      if (!Number.isFinite(v) || raw.trim() === "") fail(`"${raw}" is not a number ${at}. ${name} needs a number, like ${name} == 443.`);
      return (p) => {
        const xs = def.get(p) as number[];
        if (!xs.length) return false;
        switch (op) {
          case "==":
            return xs.some((x) => x === v);
          case "!=":
            return xs.every((x) => x !== v);
          case ">":
            return xs.some((x) => x > v);
          case "<":
            return xs.some((x) => x < v);
          case ">=":
            return xs.some((x) => x >= v);
          default:
            return xs.some((x) => x <= v);
        }
      };
    }
    case "bool": {
      const lower = raw.toLowerCase();
      const v = ["1", "true", "yes", "set"].includes(lower) ? 1 : ["0", "false", "no"].includes(lower) ? 0 : -1;
      if (v < 0) fail(`${name} is either 1 (set) or 0 (not set), not "${raw}" ${at}.`);
      if (op !== "==" && op !== "!=") fail(`${name} can only be compared with == or != ${at}.`);
      return (p) => {
        const xs = def.get(p);
        if (!xs.length) return false;
        const equal = xs.some((x) => x === v);
        return op === "==" ? equal : !equal;
      };
    }
    case "ip": {
      if (op === "contains") {
        const needle = raw.toLowerCase();
        return (p) => def.get(p).some((x) => String(x).includes(needle));
      }
      if (op !== "==" && op !== "!=") fail(`Addresses can only be compared with ==, != or contains ${at}.`);
      const cidr = parseCidr(raw);
      if (!cidr) fail(`"${raw}" is not an IP address ${at}. To match a name, use host == "${raw}".`);
      const range = cidr as { base: number; mask: number };
      return (p) => {
        const xs = def.get(p);
        if (!xs.length) return false;
        const hit = xs.some((x) => inCidr(x, range));
        return op === "==" ? hit : !hit;
      };
    }
    case "str":
    case "host": {
      if (op !== "==" && op !== "!=" && op !== "contains") fail(`"${op}" does not work on text ${at}. Use ==, != or contains.`);
      const needle = raw.toLowerCase();
      const cidr = def.type === "host" ? parseCidr(raw) : null;
      return (p) => {
        const xs = def.get(p).map((x) => String(x).toLowerCase());
        if (!xs.length) return false;
        if (op === "contains") return xs.some((x) => x.includes(needle));
        const hit = cidr ? xs.some((x) => inCidr(x, cidr)) : xs.some((x) => x === needle);
        return op === "==" ? hit : !hit;
      };
    }
  }
}

function textSearch(needle: string): (p: Pkt) => boolean {
  const lower = needle.toLowerCase();
  return (p) =>
    `${p.src} ${p.dst} ${p.sn ?? ""} ${p.dn ?? ""} ${p.proto} ${p.info} ${p.summary} ${p.app ?? ""}`.toLowerCase().includes(lower);
}

/** Turns filter text into a test for each packet; null means "show everything". Throws FilterError. */
function compileFilter(input: string): ((p: Pkt) => boolean) | null {
  if (!input.trim()) return null;
  const tokens = tokenize(input);
  let i = 0;
  const peek = () => tokens[i];
  const next = () => tokens[i++];
  const startsTerm = (t: Token) => t.kind === "word" || t.kind === "str" || t.kind === "lp" || t.kind === "not";

  function parseOr(): (p: Pkt) => boolean {
    let left = parseAnd();
    while (peek().kind === "or") {
      const op = next();
      if (!startsTerm(peek())) fail(`Expected something after "${op.text}" (at character ${op.pos}).`);
      const a = left;
      const b = parseAnd();
      left = (p) => a(p) || b(p);
    }
    return left;
  }

  function parseAnd(): (p: Pkt) => boolean {
    let left = parseNot();
    for (;;) {
      const t = peek();
      if (t.kind === "and") {
        next();
        if (!startsTerm(peek())) fail(`Expected something after "${t.text}" (at character ${t.pos}).`);
      } else if (!startsTerm(t)) break;
      // Two terms side by side ("github tls") both have to match.
      const a = left;
      const b = parseNot();
      left = (p) => a(p) && b(p);
    }
    return left;
  }

  function parseNot(): (p: Pkt) => boolean {
    if (peek().kind === "not") {
      const t = next();
      if (!startsTerm(peek())) fail(`Expected something after "${t.text}" (at character ${t.pos}).`);
      const inner = parseNot();
      return (p) => !inner(p);
    }
    return parsePrimary();
  }

  function parsePrimary(): (p: Pkt) => boolean {
    const t = next();
    switch (t.kind) {
      case "lp": {
        if (peek().kind === "rp") fail(`The brackets at character ${t.pos} are empty.`);
        const inner = parseOr();
        if (peek().kind !== "rp") fail(`Missing ")" to close the "(" at character ${t.pos}.`);
        next();
        return inner;
      }
      case "rp":
        return fail(`There is a ")" at character ${t.pos} with no "(" before it.`);
      case "op":
        return fail(`"${t.text}" at character ${t.pos} needs a field before it, for example tcp.port == 443.`);
      case "end":
        return fail("The filter ends too soon.");
      case "and":
      case "or":
        return fail(`"${t.text}" at character ${t.pos} needs something on both sides, for example tcp and dns.`);
      case "not":
        return parseNot();
      case "str":
        return textSearch(t.text);
      case "word": {
        const lower = t.text.toLowerCase();
        const def = FIELDS[lower];
        if (def) {
          if (peek().kind === "op") {
            const opTok = next();
            const value = peek();
            if (value.kind !== "word" && value.kind !== "str") fail(`Expected a value after "${opTok.text}" (at character ${opTok.pos}).`);
            next();
            return compare(lower, def, OPS[opTok.text] ?? opTok.text, value);
          }
          if (def.type === "bool") return (p) => def.get(p).some((x) => x === 1);
          return (p) => def.get(p).length > 0;
        }
        if (PROTOCOLS[lower] !== undefined) {
          const op = peek();
          if (op.kind === "op")
            fail(`"${t.text}" is a protocol name, so it cannot be compared with "${op.text}" (at character ${op.pos}). Use it alone, like ${lower}, or pick a field, like tcp.port == 443.`);
          const bit = PROTOCOLS[lower];
          return (p) => (p.mask & bit) !== 0;
        }
        // "tcp.prot" looks like a field and is probably a typo; "github.com" is just text.
        const prefix = lower.split(".")[0];
        if (lower.includes(".") && PROTOCOLS[prefix] !== undefined && ipNumber(lower) === null) unknownField(t);
        if (peek().kind === "op") unknownField(t);
        return textSearch(t.text);
      }
    }
  }

  const result = parseOr();
  const rest = peek();
  if (rest.kind === "rp") fail(`There is a ")" at character ${rest.pos} with no "(" before it.`);
  if (rest.kind !== "end") fail(`I did not understand "${rest.text}" at character ${rest.pos}.`);
  return result;
}
