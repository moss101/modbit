/**
 * The browser host's policy (PX-073, PX-120): where a session may go, which
 * pages a tool may act on, and which DevTools-protocol methods nothing may
 * call. Pure logic with no Electron in it, so it is the same code the host
 * runs and the unit tests exercise.
 *
 * Three rules, all fail-closed:
 *  - A destination on this machine or its local network (loopback, private
 *    ranges, link-local, carrier-grade NAT, unique-local, multicast, the
 *    cloud metadata endpoints) is refused unless the person's policy names
 *    the host or the address. The check is made on the address the
 *    connection is made to, not on the name in the URL (`target-proxy.ts`
 *    pins it), so a name that resolves publicly at check time and privately
 *    at connect time is caught.
 *  - Every tool call looks at the page it would act on: `file:` is never
 *    allowed; with an allow-list configured a page outside it is refused;
 *    an origin that cannot be determined is refused.
 *  - A raw DevTools-protocol method on the deny list is refused before any
 *    command is sent; only the host's own structural dispatch may use
 *    `Input` and `Target`.
 */
import { readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { dirname } from "node:path";

/** The version of the policy bundle format. */
export const POLICY_VERSION = 1;

export type AddressClass = "public" | "loopback" | "private" | "link_local" | "metadata" | "carrier_grade" | "unique_local" | "multicast" | "unspecified" | "reserved";

// ---- addresses ----

function parseV4(s: string): number[] | null {
  const m = /^(\d{1,3})\.(\d{1,3})\.(\d{1,3})\.(\d{1,3})$/.exec(s);
  if (!m) return null;
  const parts = [m[1]!, m[2]!, m[3]!, m[4]!].map(Number);
  return parts.every((n) => n >= 0 && n <= 255) ? parts : null;
}

/** Eight 16-bit groups of an IPv6 address (zone ids and an embedded IPv4 tail handled), or null. */
function parseV6(input: string): number[] | null {
  let s = input.trim();
  if (s.startsWith("[") && s.endsWith("]")) s = s.slice(1, -1);
  const zone = s.indexOf("%");
  if (zone >= 0) s = s.slice(0, zone);
  if (!s.includes(":")) return null;
  let tail: number[] = [];
  const lastColon = s.lastIndexOf(":");
  const maybeV4 = parseV4(s.slice(lastColon + 1));
  if (maybeV4) {
    tail = [(maybeV4[0]! << 8) | maybeV4[1]!, (maybeV4[2]! << 8) | maybeV4[3]!];
    s = s.slice(0, lastColon + 1) + "0:0";
  }
  const halves = s.split("::");
  if (halves.length > 2) return null;
  const parseGroups = (g: string): number[] | null => {
    if (g === "") return [];
    const out: number[] = [];
    for (const part of g.split(":")) {
      if (!/^[0-9a-fA-F]{1,4}$/.test(part)) return null;
      out.push(parseInt(part, 16));
    }
    return out;
  };
  const head = parseGroups(halves[0]!);
  const rest = halves.length === 2 ? parseGroups(halves[1]!) : [];
  if (!head || !rest) return null;
  let groups: number[];
  if (halves.length === 2) {
    const missing = 8 - head.length - rest.length;
    if (missing < 1) return null;
    groups = [...head, ...new Array<number>(missing).fill(0), ...rest];
  } else {
    groups = head;
  }
  if (groups.length !== 8) return null;
  if (tail.length === 2) {
    groups[6] = tail[0]!;
    groups[7] = tail[1]!;
  }
  return groups;
}

function classifyV4(p: number[]): AddressClass {
  const [a, b, c] = [p[0]!, p[1]!, p[2]!];
  if (a === 0) return "unspecified";
  if (a === 10) return "private";
  if (a === 127) return "loopback";
  if (a === 169 && b === 254) return p[2] === 169 && p[3] === 254 ? "metadata" : "link_local";
  if (a === 172 && b >= 16 && b <= 31) return "private";
  if (a === 192 && b === 168) return "private";
  if (a === 100 && b >= 64 && b <= 127) return "carrier_grade";
  if (a === 192 && b === 0 && (c === 0 || c === 2)) return "reserved";
  if (a === 198 && (b === 18 || b === 19)) return "reserved";
  if (a === 198 && b === 51 && c === 100) return "reserved";
  if (a === 203 && b === 0 && c === 113) return "reserved";
  if (a >= 224 && a <= 239) return "multicast";
  if (a >= 240) return "reserved";
  return "public";
}

/** The class of an IP address (IPv4, IPv6, IPv4-mapped and NAT64/6to4-embedded IPv4 included). Not an address: `reserved` (fail closed). */
export function classifyIp(ip: string): AddressClass {
  const v4 = parseV4(ip.trim());
  if (v4) return classifyV4(v4);
  const g = parseV6(ip);
  if (!g) return "reserved";
  const embedded = (hi: number, lo: number) => classifyV4([hi >> 8, hi & 255, lo >> 8, lo & 255]);
  if (g.every((x, i) => (i < 7 ? x === 0 : x === 0))) return "unspecified";
  if (g.slice(0, 7).every((x) => x === 0) && g[7] === 1) return "loopback";
  // ::ffff:a.b.c.d (IPv4-mapped) and ::a.b.c.d (deprecated IPv4-compatible).
  if (g.slice(0, 5).every((x) => x === 0) && (g[5] === 0xffff || g[5] === 0)) return embedded(g[6]!, g[7]!);
  // 64:ff9b::/96 (NAT64) and 2002::/16 (6to4) carry an IPv4 address.
  if (g[0] === 0x64 && g[1] === 0xff9b && g.slice(2, 6).every((x) => x === 0)) return embedded(g[6]!, g[7]!);
  if (g[0] === 0x2002) return embedded(g[1]!, g[2]!);
  if (g[0] === 0xfd00 && g[1] === 0x0ec2 && g.slice(2, 7).every((x) => x === 0) && g[7] === 0x254) return "metadata";
  if ((g[0]! & 0xfe00) === 0xfc00) return "unique_local";
  if ((g[0]! & 0xffc0) === 0xfe80) return "link_local";
  if ((g[0]! & 0xff00) === 0xff00) return "multicast";
  if (g[0] === 0x2001 && g[1] === 0x0db8) return "reserved";
  return "public";
}

/** Hostnames that are the local machine or a metadata service by name. */
const METADATA_NAMES = new Set(["metadata", "metadata.google.internal", "metadata.goog", "instance-data", "instance-data.ec2.internal", "metadata.azure.com", "metadata.tencentyun.com"]);

/** The class a hostname has by name alone, before any resolution. `public` = the name says nothing. */
export function classifyName(host: string): AddressClass {
  const h = host.toLowerCase().replace(/\.$/, "");
  if (h === "localhost" || h.endsWith(".localhost")) return "loopback";
  if (METADATA_NAMES.has(h)) return "metadata";
  if (h.endsWith(".internal") || h.endsWith(".local") || h.endsWith(".lan") || h.endsWith(".home.arpa")) return "private";
  return "public";
}

/** Whether a URL host (as `URL.hostname` prints it) is an IP literal. */
export function isIpLiteral(host: string): boolean {
  const h = host.startsWith("[") ? host.slice(1, -1) : host;
  return parseV4(h) !== null || (h.includes(":") && parseV6(h) !== null);
}

// ---- the target policy ----

export interface TargetRule {
  /** The text the rule was written as. */
  source: string;
  kind: "host" | "suffix" | "ip" | "cidr";
  host?: string;
  suffix?: string;
  ip?: string;
  base?: number[];
  bits?: number;
  v6?: boolean;
  port?: number;
}

function splitHostPort(s: string): { host: string; port?: number } {
  const bracket = /^\[([^\]]+)\](?::(\d{1,5}))?$/.exec(s);
  if (bracket) return { host: bracket[1]!, ...(bracket[2] ? { port: Number(bracket[2]) } : {}) };
  const colons = (s.match(/:/g) ?? []).length;
  if (colons === 1) {
    const [h, p] = s.split(":");
    if (/^\d{1,5}$/.test(p ?? "")) return { host: h!, port: Number(p) };
  }
  return { host: s };
}

/** Parse one rule: `host`, `host:port`, `*.suffix`, `ip`, `ip:port`, `[v6]:port`, `cidr`. Null = not a rule (ignored, fail closed). */
export function parseRule(source: string): TargetRule | null {
  const text = source.trim();
  if (text === "" || text.length > 200 || /\s/.test(text) || text.includes("/") && !/^[0-9a-fA-F:.]+\/\d{1,3}$/.test(text)) return null;
  const slash = text.indexOf("/");
  if (slash >= 0) {
    const base = text.slice(0, slash);
    const bits = Number(text.slice(slash + 1));
    const v4 = parseV4(base);
    if (v4 && bits >= 0 && bits <= 32) return { source: text, kind: "cidr", base: v4, bits, v6: false };
    const v6 = parseV6(base);
    if (v6 && bits >= 0 && bits <= 128) return { source: text, kind: "cidr", base: v6, bits, v6: true };
    return null;
  }
  const { host, port } = splitHostPort(text);
  if (port !== undefined && (port < 1 || port > 65535)) return null;
  if (host.startsWith("*.")) {
    const suffix = host.slice(1).toLowerCase();
    return suffix.length > 2 ? { source: text, kind: "suffix", suffix, ...(port ? { port } : {}) } : null;
  }
  if (isIpLiteral(host)) return { source: text, kind: "ip", ip: normalizeIp(host), ...(port ? { port } : {}) };
  if (!/^[a-z0-9.-]+$/i.test(host)) return null;
  return { source: text, kind: "host", host: host.toLowerCase().replace(/\.$/, ""), ...(port ? { port } : {}) };
}

function normalizeIp(ip: string): string {
  const v4 = parseV4(ip.replace(/^\[|\]$/g, ""));
  if (v4) return v4.join(".");
  const g = parseV6(ip);
  return g ? g.map((x) => x.toString(16)).join(":") : ip;
}

function cidrContains(rule: TargetRule, ip: string): boolean {
  const bitsOf = (g: number[], width: number): string => g.map((x) => x.toString(2).padStart(width, "0")).join("");
  const v4 = parseV4(ip);
  if (v4 && !rule.v6) return bitsOf(v4, 8).slice(0, rule.bits) === bitsOf(rule.base!, 8).slice(0, rule.bits);
  const v6 = parseV6(ip);
  if (v6 && rule.v6) return bitsOf(v6, 16).slice(0, rule.bits) === bitsOf(rule.base!, 16).slice(0, rule.bits);
  return false;
}

/** The person's policy for where the sessions may go and which pages tools may act on. */
export interface BrowserPolicy {
  version: number;
  /** Hosts, addresses and ranges the sessions may reach although they are local. */
  allowTargets: string[];
  /** Origins or host patterns a tool may act on; empty = any http(s) page. */
  allowOrigins: string[];
}

export const EMPTY_POLICY: BrowserPolicy = { version: POLICY_VERSION, allowTargets: [], allowOrigins: [] };

/** Parse a policy bundle (a file, an environment value): anything that is not a valid rule is dropped, never widened. */
export function parsePolicy(json: unknown): BrowserPolicy {
  const o = (json && typeof json === "object" ? json : {}) as Record<string, unknown>;
  const list = (v: unknown): string[] => (Array.isArray(v) ? v.filter((x): x is string => typeof x === "string").slice(0, 200) : []);
  return {
    version: POLICY_VERSION,
    allowTargets: list(o.allow_targets ?? o.allowTargets).filter((r) => parseRule(r) !== null),
    allowOrigins: list(o.allow_origins ?? o.allowOrigins).filter((r) => parseOriginRule(r) !== null),
  };
}

/** A comma-separated list from the environment. */
export function listFromEnv(v: string | undefined): string[] {
  return (v ?? "").split(",").map((s) => s.trim()).filter((s) => s !== "");
}

export class PolicyStore {
  private current: BrowserPolicy;
  private rules: TargetRule[];
  private origins: OriginRule[];
  private readonly file: string | null;
  constructor(file: string | null, env: { targets?: string | undefined; origins?: string | undefined } = {}) {
    this.file = file;
    let base: BrowserPolicy = EMPTY_POLICY;
    if (file) {
      try {
        base = parsePolicy(JSON.parse(readFileSync(file, "utf8")));
      } catch {
        // no file, or not JSON: the empty policy (nothing local is reachable)
      }
    }
    const merged: BrowserPolicy = {
      version: POLICY_VERSION,
      allowTargets: [...new Set([...base.allowTargets, ...parsePolicy({ allow_targets: listFromEnv(env.targets) }).allowTargets])],
      allowOrigins: [...new Set([...base.allowOrigins, ...parsePolicy({ allow_origins: listFromEnv(env.origins) }).allowOrigins])],
    };
    this.current = merged;
    this.rules = merged.allowTargets.map((r) => parseRule(r)!).filter(Boolean);
    this.origins = merged.allowOrigins.map((r) => parseOriginRule(r)!).filter(Boolean);
  }
  get(): BrowserPolicy {
    return { ...this.current, allowTargets: [...this.current.allowTargets], allowOrigins: [...this.current.allowOrigins] };
  }
  targetRules(): TargetRule[] {
    return this.rules;
  }
  originRules(): OriginRule[] {
    return this.origins;
  }
  /** The person changes the policy (only the shell's own IPC reaches this; the Core and the model cannot). */
  set(next: { allowTargets?: string[]; allowOrigins?: string[] }): BrowserPolicy {
    const p = parsePolicy({ allow_targets: next.allowTargets ?? this.current.allowTargets, allow_origins: next.allowOrigins ?? this.current.allowOrigins });
    this.current = p;
    this.rules = p.allowTargets.map((r) => parseRule(r)!).filter(Boolean);
    this.origins = p.allowOrigins.map((r) => parseOriginRule(r)!).filter(Boolean);
    if (this.file) {
      try {
        mkdirSync(dirname(this.file), { recursive: true });
        writeFileSync(this.file, JSON.stringify({ version: POLICY_VERSION, allow_targets: p.allowTargets, allow_origins: p.allowOrigins }, null, 2));
      } catch {
        // the policy still holds for this run
      }
    }
    return this.get();
  }
}

/** What a destination check decides. */
export type TargetVerdict = { ok: true } | { ok: false; code: "TARGET_NOT_ALLOWED"; class: AddressClass; reason: string; address?: string };

function ruleAllowsName(rules: TargetRule[], host: string, port: number): boolean {
  const h = host.toLowerCase().replace(/\.$/, "");
  return rules.some((r) => {
    if (r.port !== undefined && r.port !== port) return false;
    if (r.kind === "host") return r.host === h;
    if (r.kind === "suffix") return h.endsWith(r.suffix!);
    return false;
  });
}

function ruleAllowsAddress(rules: TargetRule[], ip: string, port: number): boolean {
  const n = normalizeIp(ip);
  return rules.some((r) => {
    if (r.port !== undefined && r.port !== port) return false;
    if (r.kind === "ip") return r.ip === n;
    if (r.kind === "cidr") return cidrContains(r, n);
    return false;
  });
}

/** Whether a connection to `address` (for URL host `host`) is allowed: public, or named by the policy. */
export function addressAllowed(rules: TargetRule[], host: string, port: number, address: string): TargetVerdict {
  const cls = classifyIp(address);
  if (cls === "public") return { ok: true };
  if (ruleAllowsName(rules, host, port) || ruleAllowsAddress(rules, address, port)) return { ok: true };
  return { ok: false, code: "TARGET_NOT_ALLOWED", class: cls, address, reason: `${host} resolves to ${address}, a ${describeClass(cls)} address that the browser policy does not name` };
}

/**
 * The check a URL passes before anything is sent and before a redirect hop
 * is followed, on what the URL says alone (a literal address, a name that is
 * the local machine or a metadata service). The names that need resolving
 * are checked again, on the address connected to, by the proxy.
 */
export function checkUrlLiteral(rules: TargetRule[], url: string): TargetVerdict {
  let u: URL;
  try {
    u = new URL(url);
  } catch {
    return { ok: false, code: "TARGET_NOT_ALLOWED", class: "reserved", reason: "the URL cannot be parsed" };
  }
  const host = u.hostname.startsWith("[") ? u.hostname.slice(1, -1) : u.hostname;
  const port = Number(u.port) || (u.protocol === "https:" ? 443 : 80);
  if (isIpLiteral(u.hostname)) {
    const v = addressAllowed(rules, host, port, host);
    return v.ok ? v : { ...v, reason: `${host} is a ${describeClass(v.class)} address that the browser policy does not name` };
  }
  const cls = classifyName(host);
  if (cls !== "public" && !ruleAllowsName(rules, host, port)) {
    return { ok: false, code: "TARGET_NOT_ALLOWED", class: cls, reason: `${host} is a ${describeClass(cls)} name that the browser policy does not name` };
  }
  return { ok: true };
}

export function describeClass(c: AddressClass): string {
  switch (c) {
    case "loopback":
      return "loopback (this machine)";
    case "private":
      return "private-network";
    case "link_local":
      return "link-local";
    case "metadata":
      return "cloud-metadata";
    case "carrier_grade":
      return "carrier-grade NAT";
    case "unique_local":
      return "unique-local (private IPv6)";
    case "multicast":
      return "multicast";
    case "unspecified":
      return "unspecified";
    case "reserved":
      return "reserved";
    default:
      return "public";
  }
}

// ---- the origin policy ----

export interface OriginRule {
  source: string;
  scheme?: string;
  host: string;
  suffix: boolean;
  port?: number;
}

/** `https://app.test`, `http://127.0.0.1:3000`, `app.test`, `*.example.com`. */
export function parseOriginRule(source: string): OriginRule | null {
  const text = source.trim();
  if (text === "" || text.length > 200 || /\s/.test(text)) return null;
  const m = /^(?:(https?):\/\/)?(\*\.)?([^/:\s]+|\[[0-9a-fA-F:.]+\])(?::(\d{1,5}))?\/?$/.exec(text);
  if (!m) return null;
  const host = m[3]!.toLowerCase();
  if (m[2] && host.length < 2) return null;
  return { source: text, ...(m[1] ? { scheme: m[1] } : {}), host: host.startsWith("[") ? host.slice(1, -1) : host, suffix: m[2] !== undefined, ...(m[4] ? { port: Number(m[4]) } : {}) };
}

export type OriginVerdict = { ok: true; origin: string } | { ok: false; code: "ORIGIN_NOT_ALLOWED" | "FILE_ORIGIN"; reason: string; allowed: string[] };

/**
 * The per-call gate: may a tool act on the page at `pageUrl`? `about:blank`
 * is the empty start page (nothing to act on, nothing to refuse). `file:`
 * never. Another scheme, or a URL that cannot be read, is an origin that
 * cannot be determined: refused. With an allow-list, the page must match.
 */
export function checkOrigin(rules: OriginRule[], allowed: string[], pageUrl: string): OriginVerdict {
  const url = pageUrl.trim();
  if (url === "" || url === "about:blank") return { ok: true, origin: "about:blank" };
  if (/^file:/i.test(url)) return { ok: false, code: "FILE_ORIGIN", reason: "the page is a file: document", allowed };
  let u: URL;
  try {
    u = new URL(url);
  } catch {
    return { ok: false, code: "ORIGIN_NOT_ALLOWED", reason: "the page's origin cannot be determined", allowed };
  }
  if (u.protocol !== "http:" && u.protocol !== "https:") {
    return { ok: false, code: "ORIGIN_NOT_ALLOWED", reason: `the page's origin (${u.protocol}) is not an http(s) origin`, allowed };
  }
  if (rules.length === 0) return { ok: true, origin: u.origin };
  const host = (u.hostname.startsWith("[") ? u.hostname.slice(1, -1) : u.hostname).toLowerCase();
  const port = Number(u.port) || (u.protocol === "https:" ? 443 : 80);
  const hit = rules.some((r) => {
    if (r.scheme && `${r.scheme}:` !== u.protocol) return false;
    if (r.port !== undefined && r.port !== port) return false;
    return r.suffix ? host.endsWith(`.${r.host}`) : host === r.host;
  });
  return hit ? { ok: true, origin: u.origin } : { ok: false, code: "ORIGIN_NOT_ALLOWED", reason: `${u.origin} is not an origin the browser policy allows`, allowed };
}

// ---- the DevTools-protocol deny list ----

/** Mirrors `modbit_browser::refusal` (a test compares the two). */
export const CDP_DENIED_DOMAINS = ["Browser", "Input", "Storage", "SystemInfo", "Target", "Tethering"];
export const CDP_DENIED_METHODS = [
  "Network.getCookies",
  "Network.getAllCookies",
  "Network.setCookie",
  "Network.setCookies",
  "Network.deleteCookies",
  "Network.clearBrowserCookies",
  "Network.clearBrowserCache",
  "Network.setCacheDisabled",
  "Network.getResponseBody",
  "Network.loadNetworkResource",
  "DOM.setFileInputFiles",
  "Page.navigate",
  "Page.navigateToHistoryEntry",
  "Page.getNavigationHistory",
  "Page.resetNavigationHistory",
  "Page.reload",
  "Page.setDownloadBehavior",
  "Page.addScriptToEvaluateOnLoad",
  "Fetch.enable",
  "Fetch.fulfillRequest",
  "Fetch.continueRequest",
  "Emulation.setUserAgentOverride",
];

/** What only the host's own structural dispatch may call although its domain is denied. */
export const CDP_STRUCTURAL_ONLY = ["Input.dispatchMouseEvent", "Input.dispatchKeyEvent", "Input.insertText", "Target.setAutoAttach"];

export function cdpDenied(method: string): boolean {
  const domain = method.split(".")[0] ?? "";
  return CDP_DENIED_DOMAINS.includes(domain) || CDP_DENIED_METHODS.includes(method);
}

export class CdpDenied extends Error {
  readonly code = "CDP_DENIED";
  readonly method: string;
  constructor(method: string) {
    super(`the DevTools-protocol method ${method} is on the host's deny list`);
    this.method = method;
  }
}

/** Throws `CdpDenied` for a method nothing may call; a structural method passes only when `structural`. */
export function guardCdp(method: string, structural: boolean): void {
  if (!cdpDenied(method)) return;
  if (structural && CDP_STRUCTURAL_ONLY.includes(method)) return;
  throw new CdpDenied(method);
}

// ---- redaction ----

const SECRET_PARAMS = new Set(["token", "access_token", "id_token", "refresh_token", "api_key", "apikey", "key", "secret", "client_secret", "password", "passwd", "pwd", "auth", "authorization", "session", "sessionid", "sid", "code", "sig", "signature", "x-amz-signature", "x-amz-credential", "jwt", "bearer", "otp", "csrf", "xsrf"]);

function secretParam(name: string): boolean {
  const n = name.toLowerCase();
  return SECRET_PARAMS.has(n) || n.endsWith("_token") || n.endsWith("-token") || n.endsWith("_secret") || n.endsWith("_key") || n.endsWith("password");
}

export const REDACTED = "[REDACTED]";

/** A URL for the Core: userinfo and secret-shaped query values replaced, bounded. */
export function redactUrl(url: string): string {
  let s = url.trim();
  const m = /^([a-z][a-z0-9+.-]*:\/\/)([^/?#]*)(.*)$/i.exec(s);
  if (m && m[2]!.includes("@")) s = `${m[1]}${REDACTED}@${m[2]!.slice(m[2]!.lastIndexOf("@") + 1)}${m[3]}`;
  const pairs = (part: string) =>
    part
      .split("&")
      .map((p) => {
        const i = p.indexOf("=");
        return i > 0 && secretParam(p.slice(0, i)) && i < p.length - 1 ? `${p.slice(0, i)}=${REDACTED}` : p;
      })
      .join("&");
  const hash = s.indexOf("#");
  let frag = hash >= 0 ? s.slice(hash + 1) : null;
  let base = hash >= 0 ? s.slice(0, hash) : s;
  const q = base.indexOf("?");
  if (q >= 0) base = `${base.slice(0, q)}?${pairs(base.slice(q + 1))}`;
  if (frag !== null && frag.includes("=")) frag = pairs(frag);
  s = frag !== null ? `${base}#${frag}` : base;
  return s.length > 300 ? `${s.slice(0, 300)}…` : s;
}

function looksSecret(token: string): boolean {
  const t = token.replace(/^[^A-Za-z0-9\-_.+/=]+|[^A-Za-z0-9\-_.+/=]+$/g, "");
  if (t.length < 12) return false;
  const lower = t.toLowerCase();
  if (["sk-", "sk_live", "sk_test", "pk_live", "ghp_", "gho_", "ghu_", "ghs_", "ghr_", "github_pat_", "xoxb-", "xoxp-", "xoxa-", "xoxs-", "glpat-", "ya29."].some((p) => lower.startsWith(p))) return true;
  if (/^(AKIA|ASIA)[A-Z0-9]{16}$/.test(t) || (t.startsWith("AIza") && t.length === 39)) return true;
  if (t.startsWith("eyJ") && (t.match(/\./g) ?? []).length === 2) return true;
  if (t.length >= 32 && !/[\s/.]/.test(t)) {
    const digits = (t.match(/\d/g) ?? []).length;
    const letters = (t.match(/[A-Za-z]/g) ?? []).length;
    if (digits >= 4 && letters >= 8) return true;
    if (t.length >= 40 && /^[0-9a-fA-F]+$/.test(t)) return true;
  }
  return false;
}

/** A console line for the Core: bounded; credentials, secret-shaped tokens and `secrets` removed. */
export function redactText(text: string, secrets: readonly string[] = [], max = 500): string {
  let out = text;
  for (const s of secrets) if (s.length >= 4) out = out.split(s).join(REDACTED);
  const words: string[] = [];
  let next = false;
  for (const w of out.split(" ")) {
    if (next && w !== "") {
      next = false;
      words.push(REDACTED);
      continue;
    }
    const lower = w.toLowerCase();
    if (lower === "bearer" || lower === "basic" || lower === "token:" || lower === "authorization:") {
      next = true;
      words.push(w);
      continue;
    }
    if (w.includes("://")) {
      const i = w.indexOf("http");
      words.push(i >= 0 ? w.slice(0, i) + redactUrl(w.slice(i)) : redactUrl(w));
      continue;
    }
    const eq = w.indexOf("=");
    const co = w.indexOf(":");
    const strip = (k: string) => k.replace(/^[^A-Za-z0-9_-]+|[^A-Za-z0-9_-]+$/g, "");
    if (eq > 0 && eq < w.length - 1 && secretParam(strip(w.slice(0, eq)))) {
      words.push(`${w.slice(0, eq)}=${REDACTED}`);
      continue;
    }
    if (co > 0 && co < w.length - 1 && !w.slice(co + 1).startsWith("//") && secretParam(strip(w.slice(0, co)))) {
      words.push(`${w.slice(0, co)}:${REDACTED}`);
      continue;
    }
    words.push(looksSecret(w) ? REDACTED : w);
  }
  const joined = words.join(" ");
  return joined.length > max ? `${joined.slice(0, max)}…` : joined;
}
