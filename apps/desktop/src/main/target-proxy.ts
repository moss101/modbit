/**
 * The target-policy proxy (PX-120). Every request a browser session's page
 * makes — a navigation, a redirect hop, a sub-frame, a script, a `fetch` —
 * goes through a forward proxy the host runs on the loopback interface for
 * that session (`session.setProxy`, loopback included). The proxy resolves
 * the destination itself, applies the address-class policy to the
 * addresses it got, and connects to the address it checked: the check and
 * the connection are one step, so a name that resolves publicly when asked
 * and privately when connected to cannot slip between them, and a redirect
 * to a private address is refused at the hop whatever the page believes.
 *
 * The page never sees the proxy and the model never reaches it. A refusal
 * is recorded (who, where, why) and answered to the page as a plain 403 /
 * failed tunnel; the host turns the record into the typed refusal the Core
 * hears (`TARGET_NOT_ALLOWED`).
 */
import dns from "node:dns";
import http from "node:http";
import net from "node:net";
import { addressAllowed, type AddressClass, type TargetRule, type TargetVerdict } from "./browser-policy.ts";

export interface Denial {
  /** The request as the page made it: scheme, host and port (https tunnels show host:port only). */
  host: string;
  port: number;
  /** The full URL, when the proxy saw it (plain HTTP). */
  url: string;
  class: AddressClass;
  address: string;
  reason: string;
  atMs: number;
}

export type Resolver = (host: string) => Promise<{ address: string; family: number }[]>;

/** The system resolver, with the names that are loopback by definition (RFC 6761) answered locally. */
export const systemResolver: Resolver = async (host) => {
  const h = host.toLowerCase().replace(/\.$/, "");
  if (h === "localhost" || h.endsWith(".localhost")) {
    return [
      { address: "127.0.0.1", family: 4 },
      { address: "::1", family: 6 },
    ];
  }
  const r = await dns.promises.lookup(h, { all: true, verbatim: true });
  return r.map((a) => ({ address: a.address, family: a.family }));
};

/**
 * The system resolver with static answers for chosen names (`name=address`,
 * comma-separated): how a split-horizon setup, or a test, makes a name
 * resolve somewhere. The override changes what a name resolves to, never
 * what the policy says about the address it resolves to.
 */
export function resolverWithOverrides(spec: string | undefined, base: Resolver = systemResolver): Resolver {
  const table = new Map<string, { address: string; family: number }[]>();
  for (const part of (spec ?? "").split(",")) {
    const [name, address] = part.split("=").map((s) => s.trim());
    if (!name || !address || net.isIP(address) === 0) continue;
    const list = table.get(name.toLowerCase()) ?? [];
    list.push({ address, family: net.isIP(address) });
    table.set(name.toLowerCase(), list);
  }
  if (table.size === 0) return base;
  return async (host) => table.get(host.toLowerCase().replace(/\.$/, "")) ?? base(host);
}

export interface ProxyOptions {
  rules: () => TargetRule[];
  resolve?: Resolver;
  onDenied: (d: Denial) => void;
  /** Called with every host:port connected to (the test's witness that a check and a connection were one step). */
  onConnect?: (host: string, address: string, port: number) => void;
}

const HOP_BY_HOP = ["connection", "proxy-connection", "proxy-authorization", "proxy-authenticate", "keep-alive", "te", "trailer", "upgrade"];

export class TargetProxy {
  private readonly server: http.Server;
  private port = 0;
  readonly denials: Denial[] = [];
  private readonly resolve: Resolver;
  private readonly opts: ProxyOptions;

  constructor(opts: ProxyOptions) {
    this.opts = opts;
    this.resolve = opts.resolve ?? systemResolver;
    this.server = http.createServer((req, res) => void this.forward(req, res));
    this.server.on("connect", (req, socket, head) => void this.tunnel(req, socket as net.Socket, head));
    this.server.on("clientError", (_e, socket) => socket.destroy());
  }

  async listen(): Promise<number> {
    await new Promise<void>((ok, bad) => {
      this.server.once("error", bad);
      this.server.listen(0, "127.0.0.1", () => ok());
    });
    this.port = (this.server.address() as net.AddressInfo).port;
    return this.port;
  }

  get listening(): number {
    return this.port;
  }

  close(): void {
    this.server.close();
    this.server.closeAllConnections?.();
  }

  /** Resolve `host` and pick the address to connect to: the first the policy allows. Denied when none is. */
  async pin(host: string, port: number, url = ""): Promise<{ ok: true; address: string; family: number } | { ok: false; denial?: Denial }> {
    const bare = host.startsWith("[") ? host.slice(1, -1) : host;
    let addrs: { address: string; family: number }[];
    try {
      addrs = net.isIP(bare) ? [{ address: bare, family: net.isIP(bare) }] : await this.resolve(bare);
    } catch {
      // A name that does not resolve is a failed request, not a policy refusal.
      return { ok: false };
    }
    if (addrs.length === 0) return { ok: false };
    const rules = this.opts.rules();
    let first: TargetVerdict | null = null;
    for (const a of addrs) {
      const v = addressAllowed(rules, bare, port, a.address);
      if (v.ok) return { ok: true, address: a.address, family: a.family };
      first ??= v;
    }
    const v = first as Exclude<TargetVerdict, { ok: true }>;
    return { ok: false, denial: this.deny(host, port, v.address ?? addrs[0]!.address, v.class, url, v.reason) };
  }

  private deny(host: string, port: number, address: string, cls: AddressClass, url: string, reason: string): Denial {
    const d: Denial = { host, port, url, class: cls, address, reason, atMs: Date.now() };
    this.denials.push(d);
    if (this.denials.length > 200) this.denials.splice(0, this.denials.length - 200);
    this.opts.onDenied(d);
    return d;
  }

  private async forward(req: http.IncomingMessage, res: http.ServerResponse): Promise<void> {
    let u: URL;
    try {
      u = new URL(req.url ?? "");
    } catch {
      res.writeHead(400).end();
      return;
    }
    if (u.protocol !== "http:") {
      res.writeHead(400).end();
      return;
    }
    const port = Number(u.port) || 80;
    const pinned = await this.pin(u.hostname, port, u.toString());
    if (!pinned.ok) {
      if (!pinned.denial) {
        res.writeHead(502, { "content-type": "text/plain" }).end("the destination did not resolve");
        return;
      }
      res.writeHead(403, { "content-type": "text/plain; charset=utf-8", "x-modbit-blocked": "TARGET_NOT_ALLOWED" });
      res.end("blocked by the browser policy (TARGET_NOT_ALLOWED)");
      return;
    }
    this.opts.onConnect?.(u.hostname, pinned.address, port);
    const headers: http.OutgoingHttpHeaders = { ...req.headers };
    for (const h of HOP_BY_HOP) delete headers[h];
    headers.host = u.host;
    const upstream = http.request({ host: pinned.address, family: pinned.family, port, method: req.method, path: `${u.pathname}${u.search}`, headers, agent: false }, (r) => {
      const out: http.OutgoingHttpHeaders = { ...r.headers };
      for (const h of HOP_BY_HOP) delete out[h];
      res.writeHead(r.statusCode ?? 502, out);
      r.pipe(res);
    });
    upstream.on("error", () => {
      if (!res.headersSent) res.writeHead(502, { "content-type": "text/plain" });
      res.end("the destination could not be reached");
    });
    res.on("close", () => upstream.destroy());
    req.pipe(upstream);
  }

  private async tunnel(req: http.IncomingMessage, client: net.Socket, head: Buffer): Promise<void> {
    const target = req.url ?? "";
    const m = /^(\[[^\]]+\]|[^:]+):(\d{1,5})$/.exec(target);
    if (!m) {
      client.end("HTTP/1.1 400 Bad Request\r\n\r\n");
      return;
    }
    const host = m[1]!;
    const port = Number(m[2]);
    const pinned = await this.pin(host, port);
    if (!pinned.ok) {
      client.end(pinned.denial ? "HTTP/1.1 403 Forbidden\r\nx-modbit-blocked: TARGET_NOT_ALLOWED\r\nContent-Length: 0\r\n\r\n" : "HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\n\r\n");
      return;
    }
    this.opts.onConnect?.(host, pinned.address, port);
    const upstream = net.connect({ host: pinned.address, port, family: pinned.family });
    upstream.once("connect", () => {
      client.write("HTTP/1.1 200 Connection Established\r\n\r\n");
      if (head.length > 0) upstream.write(head);
      upstream.pipe(client);
      client.pipe(upstream);
    });
    const end = () => {
      client.destroy();
      upstream.destroy();
    };
    upstream.once("error", () => {
      if (!client.destroyed) client.end("HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\n\r\n");
      end();
    });
    client.once("error", end);
    client.once("close", end);
  }
}
