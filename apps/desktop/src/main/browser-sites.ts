/**
 * Site decisions for the browser pane (PX-216, DR-PX-2026-11-11-021 MWS-19): which origins the agent may navigate to.
 *
 * A navigation by the agent to an origin the person has not decided asks in the pane. The person answers one of three ways:
 *  - Allow once permits that one navigation and nothing after it (a grant held in memory for the session, used up by the
 *    navigation, never written down);
 *  - Always allow and Deny are remembered here, in the desktop's own file, and listed in Settings where each can be removed
 *    (the origin then asks again).
 *
 * The destination policy (`browser-policy.ts`) is applied before any of this: a target it forbids is refused with no ask, and a
 * decision here never widens it. A decision belongs to the origin the card showed (`scheme://host[:port]`), never to a host or a
 * parent domain. Only the shell's own IPC reaches the store; the Core and the model have no path to it.
 *
 * Startup entries (`MODBIT_BROWSER_ALLOW_SITES`, a comma-separated list; `scheme://host:*` names every port of a host) are the
 * person's standing allow for a managed or test setup. They cannot be removed from Settings, and a Deny recorded by the person
 * wins over them.
 */
import { readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { dirname } from "node:path";

export type SiteDecision = "allow" | "deny";

export interface SiteEntry {
  /** `scheme://host[:port]`, exactly as a URL prints its origin. */
  origin: string;
  decision: SiteDecision;
  decidedAtMs: number;
  /** `person` entries are listed and removable; `startup` entries come from the environment and are not. */
  source: "person" | "startup";
}

/** How long an Allow once stays usable when the agent does not come back for it. */
export const ALLOW_ONCE_MS = 10 * 60_000;
/** The most entries the person's file holds (the list is a person's, not a log). */
export const MAX_SITE_ENTRIES = 500;

/** The origin of an http(s) URL as the URL prints it, or null (not http(s), not a URL, an opaque origin). */
export function siteOrigin(url: string): string | null {
  try {
    const u = new URL(url);
    if (u.protocol !== "http:" && u.protocol !== "https:") return null;
    if (u.hostname === "") return null;
    return u.origin;
  } catch {
    return null;
  }
}

/** A stored or startup origin pattern in its canonical form (`scheme://host[:port]`, or `scheme://host:*`), or null. */
export function canonicalOrigin(text: string, allowAnyPort = false): string | null {
  const t = text.trim();
  if (allowAnyPort && /^https?:\/\/[^/\s]+:\*$/i.test(t)) {
    const base = siteOrigin(t.slice(0, -2));
    return base === null ? null : `${base}:*`;
  }
  if (t.length === 0 || t.length > 300) return null;
  const o = siteOrigin(t);
  // Only an origin written as an origin (an optional trailing slash aside): a path, query or fragment is not one.
  return o !== null && sameOriginText(o, t.replace(/\/$/, "")) ? o : null;
}

function sameOriginText(a: string, b: string): boolean {
  return a.toLowerCase() === b.toLowerCase();
}

export class SiteStore {
  private entries: SiteEntry[] = [];
  private readonly startup: SiteEntry[];
  private readonly file: string | null;

  constructor(file: string | null, startup: string[] = []) {
    this.file = file;
    this.startup = startup
      .map((s) => canonicalOrigin(s, true))
      .filter((o): o is string => o !== null)
      .map((origin) => ({ origin, decision: "allow" as const, decidedAtMs: 0, source: "startup" as const }));
    if (file) {
      try {
        const j = JSON.parse(readFileSync(file, "utf8")) as { sites?: unknown };
        if (Array.isArray(j.sites)) {
          for (const raw of j.sites.slice(0, MAX_SITE_ENTRIES)) {
            const e = raw as Partial<SiteEntry> | null;
            if (!e || typeof e.origin !== "string" || (e.decision !== "allow" && e.decision !== "deny")) continue;
            const origin = siteOrigin(e.origin);
            if (origin === null || !sameOriginText(origin, e.origin)) continue;
            if (this.entries.some((x) => x.origin === origin)) continue;
            this.entries.push({ origin, decision: e.decision, decidedAtMs: typeof e.decidedAtMs === "number" ? e.decidedAtMs : 0, source: "person" });
          }
        }
      } catch {
        // none yet, or not JSON: no decisions (every origin asks)
      }
    }
  }

  /** What the person decided for this origin, or a startup allow that names it; undefined = nothing decided. A person's Deny wins. */
  get(origin: string): SiteEntry | undefined {
    const person = this.entries.find((e) => e.origin === origin);
    if (person) return person;
    return this.startup.find((e) => (e.origin.endsWith(":*") ? startsWithAnyPort(origin, e.origin.slice(0, -2)) : e.origin === origin));
  }

  /** The entries Settings lists: the person's, then the startup ones. */
  list(): SiteEntry[] {
    return [...this.entries.map((e) => ({ ...e })), ...this.startup.map((e) => ({ ...e }))];
  }

  /** Record the person's decision for the origin (replacing an earlier one). */
  set(origin: string, decision: SiteDecision, nowMs: number): SiteEntry {
    const o = siteOrigin(origin);
    if (o === null) throw new Error("BAD_ARGUMENT: origin must be an http(s) origin");
    const entry: SiteEntry = { origin: o, decision, decidedAtMs: nowMs, source: "person" };
    this.entries = [...this.entries.filter((e) => e.origin !== o), entry].slice(-MAX_SITE_ENTRIES);
    this.save();
    return { ...entry };
  }

  /** Forget the person's decision for the origin: it asks again. A startup entry is not the person's to remove here. */
  remove(origin: string): boolean {
    const before = this.entries.length;
    this.entries = this.entries.filter((e) => e.origin !== origin);
    const removed = this.entries.length !== before;
    if (removed) this.save();
    return removed;
  }

  private save(): void {
    if (!this.file) return;
    try {
      mkdirSync(dirname(this.file), { recursive: true });
      writeFileSync(this.file, JSON.stringify({ version: 1, sites: this.entries.map(({ origin, decision, decidedAtMs }) => ({ origin, decision, decidedAtMs })) }, null, 2));
    } catch {
      // the decisions still hold for this run
    }
  }
}

/** `http://127.0.0.1:41233` is under the startup pattern `http://127.0.0.1` (written `http://127.0.0.1:*`). */
function startsWithAnyPort(origin: string, base: string): boolean {
  if (!origin.startsWith(base)) return false;
  const rest = origin.slice(base.length);
  return rest === "" || /^:\d{1,5}$/.test(rest);
}
