/**
 * Certificate trust for the browser sessions (PX-073): a certificate error
 * holds the request for up to 60 seconds while the person decides — they see
 * the issuer, the subject, the validity and the fingerprint — and Reject or
 * Trust is final for that request. A Trust is remembered for the workspace
 * (host, port and exact certificate fingerprint, so a different certificate
 * on the same host asks again), can be listed, and can be cleared. A hold
 * that runs out is a Reject. Nothing about this reaches the page or the
 * model: a refused certificate is a failed load the agent is told about, in
 * words that say it is the person's to decide.
 */
import { readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { dirname } from "node:path";

/** How long a certificate error holds a request for the person. */
export const CERT_HOLD_MS = 60_000;

export interface CertInfo {
  id: string;
  /** `host:port` of the request. */
  hostPort: string;
  url: string;
  error: string;
  issuer: string;
  subject: string;
  /** Seconds since the epoch, as Electron reports them. */
  validStart: number;
  validExpiry: number;
  /** `sha256/<base64>` as Electron reports it. */
  fingerprint: string;
}

export interface Trust {
  workspace: string;
  hostPort: string;
  fingerprint: string;
  issuer: string;
  subject: string;
  validExpiry: number;
  trustedAtMs: number;
}

export class CertTrustStore {
  private trusts: Trust[] = [];
  private readonly file: string | null;
  constructor(file: string | null) {
    this.file = file;
    if (file) {
      try {
        const j = JSON.parse(readFileSync(file, "utf8")) as { trusts?: Trust[] };
        this.trusts = Array.isArray(j.trusts) ? j.trusts.filter((t) => typeof t?.hostPort === "string" && typeof t?.fingerprint === "string") : [];
      } catch {
        // none yet
      }
    }
  }

  private save(): void {
    if (!this.file) return;
    try {
      mkdirSync(dirname(this.file), { recursive: true });
      writeFileSync(this.file, JSON.stringify({ trusts: this.trusts }, null, 2));
    } catch {
      // the trust holds for this run
    }
  }

  list(workspace?: string): Trust[] {
    return this.trusts.filter((t) => workspace === undefined || t.workspace === workspace).map((t) => ({ ...t }));
  }

  has(workspace: string, hostPort: string, fingerprint: string): boolean {
    return this.trusts.some((t) => t.workspace === workspace && t.hostPort === hostPort && t.fingerprint === fingerprint);
  }

  add(workspace: string, c: CertInfo): Trust {
    this.trusts = this.trusts.filter((t) => !(t.workspace === workspace && t.hostPort === c.hostPort));
    const t: Trust = { workspace, hostPort: c.hostPort, fingerprint: c.fingerprint, issuer: c.issuer, subject: c.subject, validExpiry: c.validExpiry, trustedAtMs: Date.now() };
    this.trusts.push(t);
    this.save();
    return t;
  }

  /** Forget the trusts of a workspace (or all). Returns how many. */
  clear(workspace?: string): number {
    const before = this.trusts.length;
    this.trusts = this.trusts.filter((t) => workspace !== undefined && t.workspace !== workspace);
    this.save();
    return before - this.trusts.length;
  }
}

export type CertDecision = "trust" | "reject" | "timeout";

/** A request held for a decision: it settles once - trusted, rejected, or rejected by the clock. */
export class CertHold {
  readonly info: CertInfo;
  /** When the hold began (ms epoch). */
  readonly atMs = Date.now();
  readonly settled: Promise<CertDecision>;
  private resolve!: (d: CertDecision) => void;
  private done = false;
  private timer: ReturnType<typeof setTimeout>;
  constructor(info: CertInfo, ms: number = CERT_HOLD_MS) {
    this.info = info;
    this.settled = new Promise<CertDecision>((r) => (this.resolve = r));
    this.timer = setTimeout(() => this.decide("timeout"), ms);
  }

  get isSettled(): boolean {
    return this.done;
  }

  decide(d: CertDecision): boolean {
    if (this.done) return false;
    this.done = true;
    clearTimeout(this.timer);
    this.resolve(d);
    return true;
  }
}
