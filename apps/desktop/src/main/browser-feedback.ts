/**
 * What the page did, kept for the agent to read (PX-121): bounded ring
 * buffers of the console messages and network requests of one browser
 * session, fed by the DevTools events of the page and of its out-of-process
 * frames. Everything is redacted here (URL credentials, secret-shaped query
 * values and tokens, values the credential broker holds) before it is kept,
 * and again in the Core before the model reads it. Request headers and
 * bodies are never recorded. The text is the page's: untrusted.
 */
import { redactText, redactUrl } from "./browser-policy.ts";

export interface ConsoleEntry {
  seq: number;
  level: "log" | "info" | "warning" | "error" | "debug";
  source: "console" | "exception" | "log";
  text: string;
  url?: string;
  line?: number;
  at_ms: number;
}

export interface NetworkEntry {
  seq: number;
  method: string;
  url: string;
  status: number;
  resource_type: string;
  failed?: string;
  duration_ms?: number;
  at_ms: number;
}

/** A bounded buffer of sequence-numbered entries; a read says how many fell off before it. */
export class Ring<T extends { seq: number }> {
  private items: T[] = [];
  private next = 1;
  private evicted = 0;
  private readonly cap: number;
  constructor(cap: number) {
    this.cap = cap;
  }

  push(make: (seq: number) => T): T {
    const item = make(this.next++);
    this.items.push(item);
    if (this.items.length > this.cap) {
      this.items.shift();
      this.evicted += 1;
    }
    return item;
  }

  /** Entries with a sequence number above `since`, at most `limit`, and what was lost since. */
  read(since: number, limit: number): { entries: T[]; nextSeq: number; dropped: number } {
    const all = this.items.filter((i) => i.seq > since);
    const entries = all.slice(0, limit);
    // Entries between `since` and the oldest kept one are gone.
    const oldest = this.items[0]?.seq ?? this.next;
    const dropped = Math.max(0, oldest - 1 - since);
    return { entries, nextSeq: entries.length > 0 ? entries[entries.length - 1]!.seq : Math.max(since, this.next - 1), dropped };
  }

  clear(): void {
    this.items = [];
  }
}

const LEVELS: Record<string, ConsoleEntry["level"]> = { log: "log", debug: "debug", info: "info", error: "error", warning: "warning", warn: "warning", assert: "error", trace: "debug", dir: "log", table: "log", count: "log", timeEnd: "log" };

interface RemoteObject {
  type?: string;
  subtype?: string;
  value?: unknown;
  description?: string;
  unserializableValue?: string;
}

function show(o: RemoteObject): string {
  if (o.value !== undefined) return typeof o.value === "string" ? o.value : JSON.stringify(o.value);
  return o.description ?? o.unserializableValue ?? o.type ?? "";
}

export class PageFeedback {
  readonly console = new Ring<ConsoleEntry>(200);
  readonly network = new Ring<NetworkEntry>(300);
  private readonly open = new Map<string, NetworkEntry>();
  private readonly started = new Map<string, number>();
  private active = 0;
  private lastActivity = Date.now();
  /** Hosts the target proxy refused, so a failed tunnel reads as what it was. */
  blocked: (host: string) => string | null = () => null;
  secrets: () => readonly string[] = () => [];

  /** Requests started and not finished. */
  get inflight(): number {
    return this.active;
  }

  quietMs(now = Date.now()): number {
    return Math.max(0, now - this.lastActivity);
  }

  touch(): void {
    this.lastActivity = Date.now();
  }

  private text(s: string): string {
    return redactText(s, this.secrets(), 500);
  }

  addConsole(level: ConsoleEntry["level"], source: ConsoleEntry["source"], text: string, url?: string, line?: number): void {
    this.console.push((seq) => ({ seq, level, source, text: this.text(text), ...(url ? { url: redactUrl(url) } : {}), ...(line !== undefined ? { line } : {}), at_ms: Date.now() }));
  }

  /** One DevTools event of the page (or of a frame's session). */
  ingest(method: string, params: Record<string, unknown> | undefined, sessionId?: string): void {
    const p = (params ?? {}) as Record<string, any>;
    switch (method) {
      case "Runtime.consoleAPICalled": {
        const level = LEVELS[String(p.type)] ?? "log";
        const text = ((p.args ?? []) as RemoteObject[]).map(show).join(" ");
        const frame = p.stackTrace?.callFrames?.[0] as { url?: string; lineNumber?: number } | undefined;
        this.addConsole(level, "console", text, frame?.url, frame?.lineNumber !== undefined ? frame.lineNumber + 1 : undefined);
        break;
      }
      case "Runtime.exceptionThrown": {
        const d = p.exceptionDetails as { text?: string; exception?: RemoteObject; url?: string; lineNumber?: number } | undefined;
        const text = d?.exception?.description ?? d?.exception?.value ?? d?.text ?? "uncaught exception";
        this.addConsole("error", "exception", String(text), d?.url, d?.lineNumber !== undefined ? d.lineNumber + 1 : undefined);
        break;
      }
      case "Log.entryAdded": {
        const e = p.entry as { level?: string; text?: string; url?: string; lineNumber?: number } | undefined;
        if (e) this.addConsole(LEVELS[String(e.level)] ?? "log", "log", e.text ?? "", e.url, e.lineNumber !== undefined ? e.lineNumber + 1 : undefined);
        break;
      }
      case "Network.requestWillBeSent": {
        const key = `${sessionId ?? ""}:${String(p.requestId)}`;
        const redirected = p.redirectResponse as { status?: number } | undefined;
        const prev = this.open.get(key);
        if (prev && redirected) {
          prev.status = redirected.status ?? prev.status;
          prev.duration_ms = Date.now() - (this.started.get(key) ?? Date.now());
          this.active = Math.max(0, this.active - 1);
        }
        const req = p.request as { url?: string; method?: string } | undefined;
        const entry = this.network.push((seq) => ({ seq, method: String(req?.method ?? "GET"), url: redactUrl(String(req?.url ?? "")), status: 0, resource_type: String(p.type ?? ""), at_ms: Date.now() }));
        this.open.set(key, entry);
        this.started.set(key, Date.now());
        this.active += 1;
        this.touch();
        break;
      }
      case "Network.responseReceived": {
        const e = this.open.get(`${sessionId ?? ""}:${String(p.requestId)}`);
        const r = p.response as { status?: number; headers?: Record<string, string> } | undefined;
        if (e && r) {
          e.status = r.status ?? 0;
          const blocked = Object.entries(r.headers ?? {}).find(([k]) => k.toLowerCase() === "x-modbit-blocked");
          if (blocked) e.failed = `blocked: ${blocked[1]}`;
        }
        break;
      }
      case "Network.loadingFinished": {
        const key = `${sessionId ?? ""}:${String(p.requestId)}`;
        const e = this.open.get(key);
        if (e) {
          e.duration_ms = Date.now() - (this.started.get(key) ?? Date.now());
          this.open.delete(key);
          this.started.delete(key);
          this.active = Math.max(0, this.active - 1);
        }
        this.touch();
        break;
      }
      case "Network.loadingFailed": {
        const key = `${sessionId ?? ""}:${String(p.requestId)}`;
        const e = this.open.get(key);
        if (e) {
          const err = String(p.errorText ?? "failed");
          let host = "";
          try {
            host = new URL(e.url.replace("[REDACTED]@", "")).host;
          } catch {
            // a URL that does not parse has no host to look up
          }
          const why = this.blocked(host);
          e.failed = p.blockedReason ? `blocked: ${String(p.blockedReason)}` : why && /TUNNEL|ABORTED|FAILED/.test(err) ? `blocked: ${why}` : p.canceled ? "canceled" : err;
          e.duration_ms = Date.now() - (this.started.get(key) ?? Date.now());
          this.open.delete(key);
          this.started.delete(key);
          this.active = Math.max(0, this.active - 1);
        }
        this.touch();
        break;
      }
      default:
        break;
    }
  }
}
