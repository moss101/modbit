/**
 * The browser host (M7.1, docs/22 "Local browser"): Electron main holds one
 * sandboxed `WebContentsView` per browser session — its own persisted
 * partition, Node disabled, strict context isolation, the renderer sandbox —
 * attaches it to the Core's session over the authenticated socket, and
 * answers the Core's typed requests through the Chrome DevTools Protocol
 * on that same `webContents`. The renderer only choreographs where the view
 * sits; the page never gets a privileged API, never opens windows, never
 * downloads, never gets a permission, and is never navigated anywhere but
 * http(s). The person sees exactly the session the agent drives.
 *
 * Hardening (PX-073, PX-120) lives here, inside the host, so neither the
 * Core nor the model can go around it: every DevTools-protocol command goes
 * through one funnel that refuses the deny list; every request the page
 * makes goes through a per-session proxy that checks the address it
 * connects to; every tool call looks at the page it would act on; a
 * certificate error waits for the person; a permission is denied and named;
 * a session's view is owned by its task and reclaimed when it is the least
 * recently used of the hidden.
 */
import { BrowserWindow, session as electronSession, WebContentsView, type WebContents } from "electron";
import type { CoreClient } from "@modbit/ide-adapter-core";
import { indexDom, mergeAx, type AxNode, type DomIndex, type DomNode, type RawNode } from "./browser-ax.ts";
import { CertHold, CertTrustStore, type CertInfo, type Trust } from "./browser-cert.ts";
import { PageFeedback } from "./browser-feedback.ts";
import { NoticeBatcher, OBSERVER_BINDING, OBSERVER_SOURCE, OBSERVER_WORLD, type Notice } from "./browser-observer.ts";
import { CdpDenied, PolicyStore, checkOrigin, checkUrlLiteral, describeClass, guardCdp, type AddressClass, type BrowserPolicy } from "./browser-policy.ts";
import { admitsAgentInput, isHumanInput } from "./lease.ts";
import { StopRegistry } from "./stop.ts";
import { TargetProxy, resolverWithOverrides, type Denial, type Resolver } from "./target-proxy.ts";

/** A frame the last snapshot covered, as an action needs to find it again. */
interface FrameRec {
  key: string;
  frameId: string;
  /** The DevTools session of an out-of-process frame; undefined = the page's own session. */
  sessionId?: string;
  parentKey?: string;
  ownerBackendId?: number;
  origin: string;
}

interface ChildTarget {
  targetId: string;
  type: string;
  url: string;
  parentFrameId?: string;
}

export interface HostedSession {
  browserSessionId: string;
  taskId: string;
  sessionId: string;
  partition: string;
  view: WebContentsView;
  /** Bumped on every navigation and load the host observes. */
  stateVersion: number;
  attached: boolean;
  shown: boolean;
  /** The control lease generation this host last learned from the Core
   *  (M7.6): an agent input stamped with an older one is fenced here. */
  leaseGeneration: number;
  /** Who holds control, as last learned. */
  controller: "AGENT" | "USER";
  /** The bounds the view was last shown at (a hidden view keeps its layout). */
  lastBounds: { x: number; y: number; width: number; height: number };
  /** IMP-EV-0083: the exact web contents attached — verified before every request. */
  webContentsId: number;
  /** IMP-EV-0089: a JavaScript dialog the page opened is up (`MODAL_BLOCKING`). */
  dialogOpen: boolean;
  /** IMP-EV-0089: the dialog the page opened during the current action (type and message), if any. */
  dialogSeen: { type: string; message: string } | null;
  /** IMP-EV-0081: the view's process died and the host is bringing it back. */
  restarting?: boolean;
  /** IMP-EV-0089: permissions the page asked for during the current action (all denied). */
  permissionsAsked: string[];
  /** IMP-EV-0087: when the person last acted in the view (ms epoch), 0 = never. */
  humanInputAt: number;
  /** The workspace the session's task works in (what a certificate trust is remembered for). */
  workspace: string;
  /** The proxy every request of the page goes through (PX-120). */
  proxy: TargetProxy;
  feedback: PageFeedback;
  batcher: NoticeBatcher;
  /** The DevTools bridge is attached and its domains enabled. */
  cdpReady: boolean;
  /** Out-of-process frames attached to the page's DevTools session. */
  children: Map<string, ChildTarget>;
  /** The frames the last snapshot covered. */
  frames: Map<string, FrameRec>;
  /** Times the view's process died: an action that spans one has an unknown outcome. */
  goneCount: number;
  /** When the session last served a request or was shown (the LRU order). */
  lastUsedAt: number;
  /** The view was reclaimed to free memory; it is brought back on the next use. */
  reclaimed: { url: string; atMs: number } | null;
  /** The next request answers `VIEW_RESET` once, saying what was lost. */
  pendingReset: string | null;
  /** A certificate error waiting for the person. */
  certHold: CertHold | null;
  certCallbacks: ((trusted: boolean) => void)[];
  /** Called when a certificate hold appears (a navigation waiting on it gives up waiting). */
  onCertHold: (() => void) | null;
  /** Refusals of the target policy since the session began. */
  refusals: { atMs: number; host: string; address: string; class: AddressClass; reason: string; document: boolean; main: boolean; url: string }[];
  /** Document requests (main frame, sub-frame) seen lately, to tell a refused document from a refused script. */
  docRequests: { atMs: number; host: string; port: number; url: string; type: string }[];
}

interface PageState {
  url: string;
  title: string;
  ready: boolean;
  state_version: number;
}

type Clip = { x: number; y: number; width: number; height: number };

type HostRequest =
  | { kind: "navigate"; url: string }
  | { kind: "state" }
  | { kind: "snapshot"; max_nodes: number; observer?: boolean }
  | { kind: "capture"; clip: Clip | null; fit?: [number, number] }
  | { kind: "act"; backend_dom_node_id: number; action: "click" | "fill" | "select" | "check" | "uncheck" | "press" | "fill_credential"; value?: string; key?: string; at?: [number, number] | null; credential_handle?: string; frame?: string }
  | { kind: "scroll"; backend_dom_node_id?: number; frame?: string; mode: string; dx?: number; dy?: number }
  | { kind: "console"; since?: number; limit: number }
  | { kind: "network"; since?: number; limit: number; failed_only?: boolean }
  | { kind: "activity" }
  | { kind: "isolation" }
  | { kind: "close" };

const HOST_KIND = "electron-main";

/** The requests that look at or act on the page: each is gated on the page's origin. */
const PAGE_GATED = new Set(["snapshot", "capture", "act", "scroll", "console", "network", "activity"]);

/** The permissions a page may use (everything else is denied and named). */
const PERMISSIONS_ALLOWED = new Set(["clipboard-sanitized-write"]);

/** Hidden views kept before the least recently used is reclaimed. */
function hiddenCap(): number {
  const n = Number(process.env.MODBIT_BROWSER_HIDDEN_VIEW_CAP);
  return Number.isInteger(n) && n >= 0 ? n : 6;
}

/** IMP-EV-0089: what takes text — a text-like input, a textarea or an editable element, enabled and writable. */
const EDITABLE_CHECK = "function() { const t = this.tagName; const notText = ['button','submit','checkbox','radio','file','image','reset','range','color']; const ok = (t === 'INPUT' && !notText.includes((this.type || 'text').toLowerCase())) || t === 'TEXTAREA' || this.isContentEditable === true; if (!ok) return 'NOT_A_TEXT_FIELD:' + t + (this.type ? '/' + this.type : ''); if (this.disabled) return 'DISABLED'; if (this.readOnly) return 'READ_ONLY'; return 'ok'; }";

/** The nearest scrollable container of an element, or the page: scrolled as asked. */
const SCROLL_FN = `function(mode, dx, dy) {
  const doc = this.ownerDocument || document;
  const root = doc.scrollingElement || doc.documentElement;
  const scrollable = (el) => { if (!el || el === doc.body || el === doc.documentElement) return false; const s = getComputedStyle(el); const oy = /(auto|scroll|overlay)/.test(s.overflowY) && el.scrollHeight > el.clientHeight; const ox = /(auto|scroll|overlay)/.test(s.overflowX) && el.scrollWidth > el.clientWidth; return oy || ox; };
  let box = null;
  if (this.nodeType === 1 && this !== doc.documentElement && this !== doc.body) { for (let el = this; el; el = el.parentElement) { if (el !== this && scrollable(el)) { box = el; break; } if (el === this && scrollable(el)) { box = el; break; } } }
  const target = box || root;
  const read = () => [Math.round(target.scrollLeft), Math.round(target.scrollTop)];
  const before = read();
  if (mode === 'into_view') { this.scrollIntoView({ block: 'center', inline: 'nearest', behavior: 'instant' }); }
  else if (mode === 'to_top') { target.scrollTo({ top: 0, left: target.scrollLeft, behavior: 'instant' }); }
  else if (mode === 'to_bottom') { target.scrollTo({ top: target.scrollHeight, left: target.scrollLeft, behavior: 'instant' }); }
  else { target.scrollBy({ left: dx, top: dy, behavior: 'instant' }); }
  const after = read();
  const maxX = Math.max(0, Math.round(target.scrollWidth - target.clientWidth));
  const maxY = Math.max(0, Math.round(target.scrollHeight - target.clientHeight));
  const tag = target === root ? 'page' : (target.tagName.toLowerCase() + (target.id ? '#' + target.id : ''));
  const rect = mode === 'into_view' ? this.getBoundingClientRect() : null;
  return { x: after[0], y: after[1], maxX, maxY, moved: before[0] !== after[0] || before[1] !== after[1] || (rect ? rect.top >= 0 && rect.bottom <= innerHeight : false), container: tag };
}`;

function sleep(ms: number): Promise<void> {
  return new Promise<void>((r) => setTimeout(r, ms));
}

export interface HostOptions {
  /** The profile directory (policy and certificate trusts live under it). */
  dataDir: string;
  env: NodeJS.ProcessEnv;
}

export class BrowserHost {
  private readonly sessions = new Map<string, HostedSession>();
  /** Delivery log of every request answered, for the renderer's Browser panel and the E2E. */
  readonly log: { browserSessionId: string; kind: string; ok: boolean; code: string; generation: number; atMs: number; detail?: string }[] = [];
  /** The person's policy: which local destinations and which origins the sessions may use (PX-073, PX-120). */
  readonly policy: PolicyStore;
  /** Certificates the person trusted, per workspace (PX-073). */
  readonly trusts: CertTrustStore;
  private readonly window: () => BrowserWindow | null;
  private readonly client: () => CoreClient | null;
  private readonly notify: (channel: string, payload: unknown) => void;
  /** M7.8: the broker's secret for a handle, only for a page at the bound origin; null otherwise. */
  private readonly secretFor: (handle: string, pageOrigin: string) => string | null;
  /** Every secret value in the broker's custody (redacted from what the page logs). */
  private readonly secretValues: () => string[];
  /** How the proxies resolve names (the system's, with the operator's static answers). */
  private readonly resolver: Resolver;

  constructor(
    window: () => BrowserWindow | null,
    client: () => CoreClient | null,
    notify: (channel: string, payload: unknown) => void,
    secretFor: (handle: string, pageOrigin: string) => string | null = () => null,
    options: HostOptions = { dataDir: "", env: process.env },
    secretValues: () => string[] = () => [],
  ) {
    this.window = window;
    this.client = client;
    this.notify = notify;
    this.secretFor = secretFor;
    this.secretValues = secretValues;
    this.resolver = resolverWithOverrides(options.env.MODBIT_BROWSER_HOSTS);
    const dir = options.dataDir;
    this.policy = new PolicyStore(dir ? join(dir, "browser-policy.json") : null, { targets: options.env.MODBIT_BROWSER_ALLOW_TARGETS, origins: options.env.MODBIT_BROWSER_ALLOW_ORIGINS });
    this.trusts = new CertTrustStore(dir ? join(dir, "browser-trusted-certs.json") : null);
  }

  /** Open (or reuse) the task's session on the Core and attach a fresh view to it. */
  async open(sessionId: string, taskId: string): Promise<{ browserSessionId: string; partition: string; leaseGeneration: string }> {
    const c = this.client();
    if (!c) throw new Error("CORE_UNAVAILABLE: the local Core is restarting");
    const opened = await c.openBrowserSession(sessionId, taskId);
    const existing = this.sessions.get(opened.browserSessionId);
    if (existing && !this.gone(existing)) {
      if (!existing.attached) await this.attach(existing);
      return { browserSessionId: opened.browserSessionId, partition: opened.partition, leaseGeneration: opened.leaseGeneration.toString() };
    }
    if (existing?.reclaimed) {
      await this.revive(existing);
      if (!existing.attached) await this.attach(existing);
      return { browserSessionId: opened.browserSessionId, partition: opened.partition, leaseGeneration: opened.leaseGeneration.toString() };
    }
    const view = this.makeView(opened.partition);
    const workspace = await this.workspaceOf(c, sessionId, taskId);
    const feedback = new PageFeedback();
    feedback.secrets = () => this.secretValues();
    const hosted: HostedSession = {
      browserSessionId: opened.browserSessionId,
      taskId,
      sessionId,
      partition: opened.partition,
      view,
      stateVersion: 0,
      attached: false,
      shown: false,
      leaseGeneration: Number(opened.leaseGeneration),
      controller: opened.controller === "USER" ? "USER" : "AGENT",
      lastBounds: { x: 0, y: 0, width: 1024, height: 768 },
      webContentsId: view.webContents.id,
      dialogOpen: false,
      dialogSeen: null,
      permissionsAsked: [],
      humanInputAt: 0,
      workspace,
      proxy: undefined as unknown as TargetProxy,
      feedback,
      batcher: undefined as unknown as NoticeBatcher,
      cdpReady: false,
      children: new Map(),
      frames: new Map(),
      goneCount: 0,
      lastUsedAt: Date.now(),
      reclaimed: null,
      pendingReset: null,
      certHold: null,
      certCallbacks: [],
      onCertHold: null,
      refusals: [],
      docRequests: [],
    };
    hosted.proxy = new TargetProxy({ rules: () => this.policy.targetRules(), resolve: this.resolver, onDenied: (d) => this.onDenied(hosted, d) });
    feedback.blocked = (host) => {
      const d = hosted.proxy.denials.find((x) => `${x.host}:${x.port}` === host || x.host === host.split(":")[0]);
      return d ? "TARGET_NOT_ALLOWED" : null;
    };
    hosted.batcher = new NoticeBatcher((n) => this.sendNotice(hosted, n));
    await hosted.proxy.listen();
    // Every request the page makes goes through the proxy - loopback and link-local included.
    await electronSession.fromPartition(opened.partition).setProxy({ proxyRules: `http=127.0.0.1:${hosted.proxy.listening};https=127.0.0.1:${hosted.proxy.listening}`, proxyBypassRules: "<-loopback>" });
    this.harden(hosted);
    this.sessions.set(hosted.browserSessionId, hosted);
    // A document exists from the start (the CDP session needs a target);
    // nothing of the network is touched until the agent navigates.
    await view.webContents.loadURL("about:blank").catch(() => {});
    hosted.stateVersion = 0;
    await this.attach(hosted);
    await this.ensureCdp(hosted).catch(() => {});
    this.enforceHiddenCap();
    return { browserSessionId: opened.browserSessionId, partition: opened.partition, leaseGeneration: opened.leaseGeneration.toString() };
  }

  private makeView(partition: string): WebContentsView {
    return new WebContentsView({
      webPreferences: {
        partition,
        sandbox: true,
        contextIsolation: true,
        nodeIntegration: false,
        nodeIntegrationInWorker: false,
        nodeIntegrationInSubFrames: false,
        webSecurity: true,
        allowRunningInsecureContent: false,
        webviewTag: false,
        enableWebSQL: false,
      },
    });
  }

  /** The task's workspace root (what a certificate trust is remembered for); the session's id when none. */
  private async workspaceOf(c: CoreClient, sessionId: string, taskId: string): Promise<string> {
    try {
      const snap = await c.getSessionSnapshot(sessionId);
      const t = snap.tasks.find((x) => Buffer.from(x.taskId?.value ?? []).toString("hex") === taskId);
      if (t?.workspaceRoot) return t.workspaceRoot;
    } catch {
      // no snapshot: the session stands in
    }
    return `session:${sessionId}`;
  }

  /** A refusal of the target policy: recorded, and typed for whoever was waiting on the request. */
  private onDenied(h: HostedSession, d: Denial): void {
    const url = d.url || `${d.host}:${d.port}`;
    // Was it a document (a main frame or a sub-frame) or only a resource?
    // A document is told from a resource by the URL it asked for; a tunnel shows only host and port, so it must be a very recent document request to count.
    const doc = h.docRequests.find((r) => (d.url !== "" ? Date.now() - r.atMs < 60_000 && r.url === d.url : Date.now() - r.atMs < 3_000 && r.host === d.host && r.port === d.port));
    h.refusals.push({ atMs: d.atMs, host: d.host, address: d.address, class: d.class, reason: d.reason, document: doc !== undefined, main: doc?.type === "mainFrame", url });
    if (h.refusals.length > 100) h.refusals.splice(0, h.refusals.length - 100);
    this.log.push({ browserSessionId: h.browserSessionId, kind: "target-refused", ok: false, code: `TARGET_NOT_ALLOWED ${d.class} ${d.host}:${d.port}`, generation: h.leaseGeneration, atMs: d.atMs });
  }

  /** A refusal that was a main-frame document since `sinceMs`, if any. */
  private mainFrameRefusal(h: HostedSession, sinceMs: number) {
    return h.refusals.find((r) => r.atMs >= sinceMs && r.main);
  }

  /** The page never gains anything: no new windows, no downloads, no permissions, no non-http(s) navigation. */
  private harden(h: HostedSession): void {
    const wc = h.view.webContents;
    wc.setWindowOpenHandler(() => ({ action: "deny" }));
    // A destination the page asks for itself (a link, a script's location
    // change, a redirect): http(s) only, and a local or metadata address
    // only when the policy names it. The names that need resolving are
    // checked again by the proxy on the address connected to.
    const guardNavigation = (e: { preventDefault(): void }, url: string): void => {
      if (!/^https?:\/\//i.test(url)) {
        e.preventDefault();
        return;
      }
      const v = checkUrlLiteral(this.policy.targetRules(), url);
      if (!v.ok) {
        e.preventDefault();
        const u = new URL(url);
        const d: Denial = { host: u.hostname, port: Number(u.port) || (u.protocol === "https:" ? 443 : 80), url, class: v.class, address: v.address ?? "", reason: v.reason, atMs: Date.now() };
        h.docRequests.push({ atMs: d.atMs, host: d.host, port: d.port, url, type: "mainFrame" });
        h.proxy.denials.push(d);
        this.onDenied(h, d);
      }
    };
    wc.on("will-navigate", (e, url) => guardNavigation(e, url));
    wc.on("will-redirect", (e, url) => guardNavigation(e, url));
    wc.on("did-navigate", () => {
      this.bump(h);
      h.batcher.report({ kind: "navigation" });
    });
    wc.on("did-navigate-in-page", () => {
      this.bump(h);
      h.batcher.report({ kind: "navigation" });
    });
    wc.on("did-finish-load", () => {
      this.bump(h);
      h.batcher.report({ kind: "load" });
    });
    wc.on("render-process-gone", (_e, details) => {
      h.goneCount += 1;
      void this.restartView(h, details.reason);
    });
    // PX-073: a certificate error holds the request for the person.
    wc.on("certificate-error", (event, url, error, certificate, callback) => {
      event.preventDefault();
      let hostPort = url;
      try {
        const u = new URL(url);
        hostPort = `${u.hostname}:${u.port || (u.protocol === "https:" ? "443" : "80")}`;
      } catch {
        // an unparseable URL is held under its own text
      }
      if (this.trusts.has(h.workspace, hostPort, certificate.fingerprint)) {
        callback(true);
        return;
      }
      if (h.certHold && !h.certHold.isSettled && h.certHold.info.hostPort === hostPort && h.certHold.info.fingerprint === certificate.fingerprint) {
        h.certCallbacks.push(callback);
        return;
      }
      const info: CertInfo = { id: `${h.browserSessionId.slice(0, 8)}-${Date.now().toString(36)}`, hostPort, url, error, issuer: certificate.issuerName, subject: certificate.subjectName, validStart: certificate.validStart, validExpiry: certificate.validExpiry, fingerprint: certificate.fingerprint };
      const hold = new CertHold(info);
      h.certHold = hold;
      h.certCallbacks = [callback];
      this.log.push({ browserSessionId: h.browserSessionId, kind: "certificate-held", ok: false, code: `${error} ${hostPort}`, generation: h.leaseGeneration, atMs: Date.now() });
      this.notify("browser:state", { browserSessionId: h.browserSessionId, ...this.state(h), certPending: info });
      h.onCertHold?.();
      void hold.settled.then((d) => {
        const cbs = h.certCallbacks.splice(0);
        if (d === "trust") this.trusts.add(h.workspace, info);
        for (const cb of cbs) cb(d === "trust");
        if (h.certHold === hold) h.certHold = null;
        this.log.push({ browserSessionId: h.browserSessionId, kind: d === "trust" ? "certificate-trusted" : "certificate-rejected", ok: d === "trust", code: `${d} ${hostPort}`, generation: h.leaseGeneration, atMs: Date.now() });
        this.notify("browser:state", { browserSessionId: h.browserSessionId, ...this.state(h), certPending: null, certDecision: d });
      });
    });
    // IMP-EV-0087: the person's own input into the view (a real key, not
    // the agent's CDP input, which never reaches this hook while the host
    // dispatches it) takes control for the person at once; the agent's
    // next input is refused (`HUMAN_ACTIVE`) until control is returned.
    wc.on("before-input-event", (_e, input) => {
      if (this.agentInputInFlight > 0 || Date.now() < this.agentInputUntil || !isHumanInput(input.type)) return;
      void this.preempt(h);
    });
    // FIX-19: a mouse press or a scroll is the person acting as much as a
    // key is. Electron reports pointer input on `input-event` (the
    // `before-input-event` hook only carries the keyboard); the same guards
    // keep the host's own dispatched input out.
    wc.on("input-event", (_e: unknown, input: { type: string }) => {
      if (this.agentInputInFlight > 0 || Date.now() < this.agentInputUntil || !isHumanInput(input.type)) return;
      void this.preempt(h);
    });
    const s = electronSession.fromPartition(h.partition);
    if (this.hardenedSessions.has(s)) return;
    this.hardenedSessions.add(s);
    s.setPermissionRequestHandler((_wc, permission, cb) => {
      // IMP-EV-0089: a permission the page asks for is denied and named
      // on the action that provoked it (`PERMISSION_REQUIRED`), except the
      // minimal allow-list (PX-073).
      if (PERMISSIONS_ALLOWED.has(permission)) {
        cb(true);
        return;
      }
      if (!h.permissionsAsked.includes(permission)) h.permissionsAsked.push(permission);
      cb(false);
    });
    s.setPermissionCheckHandler((_wc, permission) => PERMISSIONS_ALLOWED.has(permission));
    s.on("will-download", (e) => e.preventDefault());
    s.setDevicePermissionHandler(() => false);
    // What kind of request each is, so a refused document is told from a refused script.
    s.webRequest.onBeforeRequest({ urls: ["http://*/*", "https://*/*"] }, (details, cb) => {
      if (details.resourceType === "mainFrame" || details.resourceType === "subFrame") {
        try {
          const u = new URL(details.url);
          h.docRequests.push({ atMs: Date.now(), host: u.hostname, port: Number(u.port) || (u.protocol === "https:" ? 443 : 80), url: details.url, type: details.resourceType });
          if (h.docRequests.length > 100) h.docRequests.splice(0, h.docRequests.length - 100);
        } catch {
          // not recorded
        }
      }
      cb({});
    });
  }

  /** Debuggers whose event handler is installed (a re-attach must not add a second). */
  private readonly boundDebuggers = new WeakSet<Electron.Debugger>();
  /** Sessions whose handlers are installed. */
  private readonly hardenedSessions = new WeakSet<Electron.Session>();
  /** Agent CDP input being dispatched right now (the person's hook ignores it). */
  private agentInputInFlight = 0;
  /** IMP-EV-0085: sessions under an emergency stop, enforced by the host itself, independent of the Core's loop. It holds until a new session lease is acquired (FIX-19: what the panel says); the Core's own stop is the Core's. */
  private readonly stops = new StopRegistry();
  /** Agent input dispatched by this host in the last moments (the person's hook ignores what arrives inside it). */
  private agentInputUntil = 0;

  /** IMP-EV-0087: the person acted in the view — control moves to them on the Core and here. */
  private async preempt(h: HostedSession): Promise<void> {
    h.humanInputAt = Date.now();
    if (h.controller === "USER") return;
    h.controller = "USER";
    const c = this.client();
    if (!c) return;
    try {
      const r = await c.setBrowserControl(h.sessionId, h.browserSessionId, h.taskId, "USER");
      h.leaseGeneration = Math.max(h.leaseGeneration, Number(r.leaseGeneration));
      this.notify("browser:state", { browserSessionId: h.browserSessionId, ...this.state(h), controller: "USER", leaseGeneration: h.leaseGeneration, preempted: true });
    } catch {
      // The Core will learn on the next hand-over; the host's own fence holds.
    }
  }

  /** IMP-EV-0085: halt every agent input this host would deliver for the session, at once, with the reason on record. */
  emergencyStop(sessionId: string, reason: string): void {
    this.stops.stop(sessionId, reason);
    this.log.push({ browserSessionId: sessionId, kind: "emergency-stop", ok: true, code: reason || "emergency stop", generation: 0, atMs: Date.now() });
    this.announceStop(sessionId);
  }

  /**
   * FIX-19: a session lease was acquired on the session (`SessionLeaseAcquired`
   * on the log). A new lease above the one the stop was raised under lifts the
   * host's fence, as the Browser panel tells the person it does; the views of
   * the session learn it at once. Whatever the Core still refuses stays refused.
   */
  sessionLeaseAcquired(sessionId: string, generation: number): void {
    if (!this.stops.leaseAcquired(sessionId, generation)) return;
    this.log.push({ browserSessionId: sessionId, kind: "emergency-stop-lifted", ok: true, code: `session lease ${generation}`, generation, atMs: Date.now() });
    this.announceStop(sessionId);
  }

  /** The views of `sessionId` learn that its stop was raised or lifted. */
  private announceStop(sessionId: string): void {
    for (const h of this.sessions.values()) if (h.sessionId === sessionId) this.notify("browser:state", { browserSessionId: h.browserSessionId, ...this.state(h) });
  }

  private bump(h: HostedSession): void {
    h.stateVersion += 1;
    this.notify("browser:state", { browserSessionId: h.browserSessionId, ...this.state(h) });
  }

  /** The observer's notice, folded and bounded, to the Core (observation only: nothing is acted on). */
  private sendNotice(h: HostedSession, n: Notice): void {
    const c = this.client();
    if (!c || !h.attached) return;
    void c.browserHostNotice(h.browserSessionId, n).catch(() => {});
    this.notify("browser:state", { browserSessionId: h.browserSessionId, ...this.state(h), observer: { changeSeq: n.change_seq, kind: n.kind } });
  }

  private async attach(h: HostedSession): Promise<void> {
    const c = this.client();
    if (!c) throw new Error("CORE_UNAVAILABLE: the local Core is restarting");
    c.onBrowserRequest = (r) => void this.handle(r);
    const a = await c.attachBrowserHost(h.browserSessionId, { hostKind: HOST_KIND, partition: h.partition, sandboxed: true, contextIsolated: true, nodeIntegration: false, taskId: h.taskId });
    h.leaseGeneration = Math.max(h.leaseGeneration, Number(a.leaseGeneration));
    h.attached = true;
  }

  /**
   * IMP-EV-0081: the view's process died — a crash, the OS. The session is
   * the Core's and this host's record of it stands (the same id, partition,
   * lease and controller); the view is loaded again at the page it was on,
   * the CDP bridge re-established, and the host attaches again to the same
   * session (the Core journals the attach). A request arriving meanwhile
   * answers `VIEW_RESTARTING`; the agent's next observation reads the
   * restarted page. The Core, its log and the task are untouched.
   */
  private async restartView(h: HostedSession, reason: string): Promise<void> {
    const wc = h.view.webContents;
    this.log.push({ browserSessionId: h.browserSessionId, kind: "view-gone", ok: false, code: reason, generation: h.leaseGeneration, atMs: Date.now() });
    this.notify("browser:state", { browserSessionId: h.browserSessionId, gone: reason });
    if (wc.isDestroyed() || reason === "clean-exit" || !this.sessions.has(h.browserSessionId)) return;
    h.restarting = true;
    h.attached = false;
    h.dialogOpen = false;
    h.cdpReady = false;
    h.children.clear();
    h.frames.clear();
    try {
      if (wc.debugger.isAttached()) wc.debugger.detach();
    } catch {
      // the session died with the process
    }
    const url = wc.getURL();
    try {
      await wc.loadURL(/^https?:\/\//i.test(url) ? url : "about:blank");
    } catch {
      // the page may be unreachable now; the view is up at whatever loaded
    }
    try {
      await this.attach(h);
      this.bump(h);
      this.log.push({ browserSessionId: h.browserSessionId, kind: "view-restarted", ok: true, code: reason, generation: h.leaseGeneration, atMs: Date.now() });
      this.notify("browser:state", { browserSessionId: h.browserSessionId, ...this.state(h), restarted: reason, controller: h.controller, leaseGeneration: h.leaseGeneration });
    } catch (e) {
      this.log.push({ browserSessionId: h.browserSessionId, kind: "view-restarted", ok: false, code: (e as Error).message.slice(0, 80), generation: h.leaseGeneration, atMs: Date.now() });
      this.notify("browser:state", { browserSessionId: h.browserSessionId, error: (e as Error).message });
    } finally {
      h.restarting = false;
    }
  }

  /** After a Core restart every live view attaches again to its session (the same partition). */
  async reattachAll(): Promise<void> {
    for (const h of this.sessions.values()) {
      if (this.gone(h)) continue;
      h.attached = false;
      try {
        await this.attach(h);
      } catch (e) {
        this.notify("browser:state", { browserSessionId: h.browserSessionId, error: (e as Error).message });
      }
    }
  }

  /** The view's web contents are gone: destroyed, or reclaimed and closed. */
  private gone(h: HostedSession): boolean {
    const wc = h.view.webContents as WebContents | undefined;
    return wc === undefined || wc.isDestroyed();
  }

  private state(h: HostedSession): PageState {
    if (this.gone(h)) return { url: h.reclaimed?.url ?? "", title: "", ready: false, state_version: h.stateVersion };
    const wc = h.view.webContents;
    if (wc.isDestroyed()) return { url: h.reclaimed?.url ?? "", title: "", ready: false, state_version: h.stateVersion };
    return { url: wc.getURL(), title: wc.getTitle(), ready: !wc.isLoading(), state_version: h.stateVersion };
  }

  /** Place the view over the renderer's placeholder (the renderer reports the rectangle). */
  show(browserSessionId: string, bounds: { x: number; y: number; width: number; height: number }): boolean {
    const h = this.sessions.get(browserSessionId);
    const win = this.window();
    if (!h || !win) return false;
    if (h.reclaimed) {
      // The person opened a view that was reclaimed: it comes back, reloaded, and the panel says so.
      void this.revive(h).then(() => {
        this.show(browserSessionId, bounds);
        this.notify("browser:state", { browserSessionId, ...this.state(h), reset: h.pendingReset });
      });
      return true;
    }
    if (this.gone(h)) return false;
    h.lastUsedAt = Date.now();
    if (!h.shown) {
      win.contentView.addChildView(h.view);
      h.shown = true;
    }
    h.lastBounds = { x: Math.round(bounds.x), y: Math.round(bounds.y), width: Math.max(0, Math.round(bounds.width)), height: Math.max(0, Math.round(bounds.height)) };
    h.view.setBounds(h.lastBounds);
    return true;
  }

  /** REQ-EV-0076: the app window is being replaced; the views it showed leave it (they are this process's) and are shown again on request. */
  windowReplaced(old: BrowserWindow): void {
    for (const h of this.sessions.values()) {
      if (!h.shown) continue;
      if (!old.isDestroyed()) old.contentView.removeChildView(h.view);
      h.shown = false;
    }
  }

  hide(browserSessionId: string): void {
    const h = this.sessions.get(browserSessionId);
    const win = this.window();
    if (!h || !win || !h.shown) return;
    win.contentView.removeChildView(h.view);
    h.shown = false;
    h.lastUsedAt = Date.now();
    this.enforceHiddenCap();
  }

  /**
   * PX-073: hidden views are capped; the least recently used that is not
   * held by the person, not loading and not waiting on a certificate is
   * reclaimed (its process freed), and the session says it was reset the
   * next time it is used.
   */
  enforceHiddenCap(): void {
    const cap = hiddenCap();
    const hidden = [...this.sessions.values()].filter((h) => !h.shown && !h.reclaimed && !this.gone(h));
    let over = hidden.length - cap;
    if (over <= 0) return;
    const candidates = hidden.filter((h) => h.controller !== "USER" && !h.view.webContents.isLoading() && !(h.certHold && !h.certHold.isSettled)).sort((a, b) => a.lastUsedAt - b.lastUsedAt);
    for (const h of candidates) {
      if (over <= 0) break;
      this.reclaim(h);
      over -= 1;
    }
  }

  private reclaim(h: HostedSession): void {
    const wc = h.view.webContents;
    const url = wc.getURL();
    h.reclaimed = { url: /^https?:\/\//i.test(url) ? url : "about:blank", atMs: Date.now() };
    h.cdpReady = false;
    h.children.clear();
    h.frames.clear();
    try {
      if (wc.debugger.isAttached()) wc.debugger.detach();
    } catch {
      // already gone
    }
    wc.close();
    this.log.push({ browserSessionId: h.browserSessionId, kind: "view-reclaimed", ok: true, code: "least recently used", generation: h.leaseGeneration, atMs: Date.now() });
    this.notify("browser:state", { browserSessionId: h.browserSessionId, ...this.state(h), reclaimed: true });
  }

  /** Bring a reclaimed view back: a new view in the same partition at the URL it had; the loss is stated on the next request. */
  private async revive(h: HostedSession): Promise<void> {
    const was = h.reclaimed;
    if (!was) return;
    const view = this.makeView(h.partition);
    h.view = view;
    h.webContentsId = view.webContents.id;
    h.shown = false;
    h.reclaimed = null;
    h.cdpReady = false;
    h.lastUsedAt = Date.now();
    this.harden(h);
    await view.webContents.loadURL("about:blank").catch(() => {});
    let loaded = "about:blank";
    if (/^https?:\/\//i.test(was.url) && checkUrlLiteral(this.policy.targetRules(), was.url).ok) {
      await view.webContents.loadURL(was.url).then(() => (loaded = was.url)).catch(() => {});
    }
    h.stateVersion += 1;
    h.pendingReset = `the view was reclaimed (least recently used of the hidden) and reloaded at ${loaded}; whatever was in the page's memory is gone`;
    this.log.push({ browserSessionId: h.browserSessionId, kind: "view-revived", ok: true, code: loaded, generation: h.leaseGeneration, atMs: Date.now() });
    await this.ensureCdp(h).catch(() => {});
  }

  /** What this host holds (for the renderer and the E2E). */
  describe(browserSessionId: string, taskId?: string): { browserSessionId: string; taskId: string; partition: string; attached: boolean; shown: boolean; url: string; title: string; stateVersion: number; leaseGeneration: number; controller: "AGENT" | "USER"; webContentsId: number; osProcessId: number; humanInputAt: number; stopped: string | null; reclaimed: boolean; certPending: CertInfo | null; changeSeq: number; refusals: number } | null {
    const h = this.sessions.get(browserSessionId);
    if (!h) return null;
    // A task sees only the views it owns (PX-073).
    if (taskId !== undefined && h.taskId !== taskId) return null;
    const destroyed = this.gone(h);
    if (destroyed && !h.reclaimed) return null;
    const s = this.state(h);
    return { browserSessionId: h.browserSessionId, taskId: h.taskId, partition: h.partition, attached: h.attached, shown: h.shown, url: s.url, title: s.title, stateVersion: h.stateVersion, leaseGeneration: h.leaseGeneration, controller: h.controller, webContentsId: h.webContentsId, osProcessId: destroyed ? 0 : h.view.webContents.getOSProcessId(), humanInputAt: h.humanInputAt, stopped: this.stops.reason(h.sessionId), reclaimed: h.reclaimed !== null, certPending: h.certHold && !h.certHold.isSettled ? h.certHold.info : null, changeSeq: h.batcher.changeSeq, refusals: h.refusals.length };
  }

  /** The views a task owns, and nobody else's (PX-073). */
  listViews(taskId: string): { browserSessionId: string; url: string; shown: boolean; reclaimed: boolean }[] {
    return [...this.sessions.values()]
      .filter((h) => h.taskId === taskId)
      .map((h) => ({ browserSessionId: h.browserSessionId, url: this.state(h).url, shown: h.shown, reclaimed: h.reclaimed !== null }));
  }

  /** Select a view for a task: only one it owns. */
  selectView(taskId: string, browserSessionId: string): ReturnType<BrowserHost["describe"]> {
    const h = this.sessions.get(browserSessionId);
    if (!h || h.taskId !== taskId) return null;
    h.lastUsedAt = Date.now();
    return this.describe(browserSessionId, taskId);
  }

  /** The refusals the target policy made in a session (for the panel and the E2E). */
  refusals(browserSessionId: string): HostedSession["refusals"] {
    return this.sessions.get(browserSessionId)?.refusals.slice() ?? [];
  }

  // ---- the person's controls over certificates, data and policy ----

  certDecide(browserSessionId: string, id: string, decision: "trust" | "reject"): boolean {
    const h = this.sessions.get(browserSessionId);
    if (!h?.certHold || h.certHold.info.id !== id) return false;
    return h.certHold.decide(decision);
  }

  certTrusts(): Trust[] {
    return this.trusts.list();
  }

  certClear(): number {
    const n = this.trusts.clear();
    // Connections made under a trust are closed: the next handshake is verified again, and the
    // error reaches the hold (a kept-alive connection would carry on without asking).
    for (const h of this.sessions.values()) void electronSession.fromPartition(h.partition).closeAllConnections();
    return n;
  }

  /**
   * PX-073: sign out of every site a session visited - cookies, storage,
   * service workers, caches, saved authentication and resolved hosts of the
   * partition are wiped, and the session's own console and network record
   * with them. Downloads are never kept (the host refuses every one).
   */
  async clearData(browserSessionId?: string): Promise<{ cleared: string[] }> {
    const targets = browserSessionId ? [this.sessions.get(browserSessionId)].filter((h): h is HostedSession => h !== undefined) : [...this.sessions.values()];
    const cleared: string[] = [];
    for (const h of targets) {
      const s = electronSession.fromPartition(h.partition);
      await s.clearStorageData();
      await s.clearCache();
      await s.clearAuthCache();
      await s.clearHostResolverCache();
      await s.cookies.flushStore();
      s.flushStorageData();
      h.feedback.console.clear();
      h.feedback.network.clear();
      cleared.push(h.browserSessionId);
      this.log.push({ browserSessionId: h.browserSessionId, kind: "data-cleared", ok: true, code: "partition wiped", generation: h.leaseGeneration, atMs: Date.now() });
    }
    return { cleared };
  }

  getPolicy(): BrowserPolicy {
    return this.policy.get();
  }

  setPolicy(next: { allowTargets?: string[]; allowOrigins?: string[] }): BrowserPolicy {
    return this.policy.set(next);
  }

  /**
   * M7.6: the person takes or returns control. The Core moves the lease
   * (a new generation), and this host learns it before answering — so an
   * agent input already on its way, stamped with the old generation, is
   * fenced the moment it arrives. The view keeps running: taking control
   * blocks the agent's input, never the person's or the observation.
   */
  async setControl(browserSessionId: string, controller: "AGENT" | "USER"): Promise<{ controller: string; leaseGeneration: number; changed: boolean }> {
    const h = this.sessions.get(browserSessionId);
    if (!h) throw new Error("NO_SUCH_SESSION: this host holds no view for the session");
    const c = this.client();
    if (!c) throw new Error("CORE_UNAVAILABLE: the local Core is restarting");
    const r = await c.setBrowserControl(h.sessionId, h.browserSessionId, h.taskId, controller);
    h.leaseGeneration = Number(r.leaseGeneration);
    h.controller = r.controller === "USER" ? "USER" : "AGENT";
    this.notify("browser:state", { browserSessionId, ...this.state(h), controller: h.controller, leaseGeneration: h.leaseGeneration });
    return { controller: r.controller, leaseGeneration: Number(r.leaseGeneration), changed: r.changed };
  }

  /** The person's own input into the view (the same session): what a takeover is for. Used by the E2E to type as the person. */
  typeAsPerson(browserSessionId: string, text: string): boolean {
    const h = this.sessions.get(browserSessionId);
    if (!h || this.gone(h)) return false;
    for (const ch of text) h.view.webContents.sendInputEvent({ type: "char", keyCode: ch });
    return true;
  }

  async close(browserSessionId: string): Promise<void> {
    const h = this.sessions.get(browserSessionId);
    if (!h) return;
    this.hide(browserSessionId);
    const c = this.client();
    if (c && h.attached) await c.closeBrowserSession(h.sessionId, h.browserSessionId).catch(() => {});
    this.release(h);
  }

  private release(h: HostedSession): void {
    this.hide(h.browserSessionId);
    h.batcher.stop();
    h.proxy.close();
    if (!this.gone(h)) {
      try {
        if (h.view.webContents.debugger.isAttached()) h.view.webContents.debugger.detach();
      } catch {
        // already gone
      }
      h.view.webContents.close();
    }
    this.sessions.delete(h.browserSessionId);
  }

  closeAll(): void {
    for (const h of [...this.sessions.values()]) this.release(h);
  }

  /** One request from the Core: answered on the same connection, always. */
  private async handle(r: { requestId: string; browserSessionId?: { value: Uint8Array } | undefined; leaseGeneration: bigint; requestJson: string }): Promise<void> {
    const c = this.client();
    if (!c) return;
    const bsid = Buffer.from(r.browserSessionId?.value ?? new Uint8Array()).toString("hex");
    const h = this.sessions.get(bsid);
    let req: HostRequest;
    try {
      req = JSON.parse(r.requestJson) as HostRequest;
    } catch {
      await c.respondBrowserHost(r.requestId, { kind: "error", code: "MALFORMED", message: "request_json" }).catch(() => {});
      return;
    }
    // The Core reading the page on its own account (after a change notice) is not a request the agent made.
    const background = req.kind === "snapshot" && req.observer === true;
    const answer = async (): Promise<unknown> => {
      if (!h || (this.gone(h) && !h.reclaimed)) return { kind: "error", code: "NO_SUCH_SESSION", message: `this host holds no view for ${bsid}` };
      if (h.reclaimed && req.kind !== "state" && req.kind !== "close") await this.revive(h);
      h.lastUsedAt = Date.now();
      // IMP-EV-0083: the view answering is exactly the one attached — the
      // same web contents, alive; a page's title or URL never stands in for it.
      if (this.gone(h) || h.view.webContents.id !== h.webContentsId) return { kind: "error", code: "WINDOW_UNVERIFIABLE", message: `the session's view is not the web contents attached (${h.webContentsId})` };
      // IMP-EV-0081: the view is coming back from a dead process; nothing is read or done until it is.
      if (h.restarting) return { kind: "error", code: "VIEW_RESTARTING", message: "the view's process died and is being restarted; retry the observation" };
      // PX-073: a view that was reclaimed says so once, instead of letting the agent act on a page that is not the one it left.
      if (h.pendingReset && !background && req.kind !== "state" && req.kind !== "close") {
        const why = h.pendingReset;
        h.pendingReset = null;
        return { kind: "error", code: "VIEW_RESET", message: why };
      }
      if (req.kind === "navigate" || req.kind === "act" || req.kind === "scroll") {
        // IMP-EV-0085: under an emergency stop no input runs, whatever the Core's loop does.
        const stop = this.stops.reason(h.sessionId);
        if (stop !== null) return { kind: "error", code: "EMERGENCY_STOPPED", message: stop };
        // Agent input under the control lease (M7.6): the generation the Core
        // stamped must be the one this host holds, and the agent must hold control.
        const verdict = admitsAgentInput(Number(r.leaseGeneration), h.leaseGeneration, h.controller);
        if (!verdict.ok) return { kind: "error", code: verdict.code, message: verdict.code === "HUMAN_ACTIVE" ? `the person holds control (lease generation ${h.leaseGeneration}); agent input is blocked, observation is not` : `input stamped with lease generation ${r.leaseGeneration} is fenced: the session is at ${h.leaseGeneration}` };
        // IMP-EV-0089: a modal dialog the page opened is the person's to answer.
        if (h.dialogOpen) return { kind: "error", code: "MODAL_BLOCKING", message: "the page has a dialog open (alert, confirm or prompt); it is the person's to answer" };
      }
      // PX-073: every call looks at the page it would act on, not only the navigation that brought it there.
      if (PAGE_GATED.has(req.kind) || req.kind === "navigate") {
        const v = checkOrigin(this.policy.originRules(), this.policy.get().allowOrigins, h.view.webContents.getURL());
        if (!v.ok && (req.kind !== "navigate" || v.code === "FILE_ORIGIN")) {
          return { kind: "error", code: v.code, message: `${v.reason}${v.allowed.length > 0 ? `; allowed: ${v.allowed.join(", ")}` : ""}; nothing was done` };
        }
      }
      switch (req.kind) {
        case "navigate":
          return this.navigate(h, req.url);
        case "state":
          return { kind: "state", state: this.state(h) };
        case "snapshot":
          // IMP-EV-0089: a JavaScript dialog holds the renderer; no tree can
          // be read until the person answers it.
          if (h.dialogOpen) return { kind: "error", code: "MODAL_BLOCKING", message: "the page has a dialog open (alert, confirm or prompt); it is the person's to answer" };
          return this.snapshot(h, req.max_nodes);
        case "capture":
          return this.capture(h, req.clip, req.fit ?? null);
        case "act":
          return this.act(h, req.backend_dom_node_id, req.action, req.value ?? "", req.key ?? "", req.at ?? null, req.credential_handle ?? null, req.frame ?? null);
        case "scroll":
          return this.scroll(h, req.backend_dom_node_id ?? null, req.frame ?? null, req.mode, req.dx ?? 0, req.dy ?? 0);
        case "console": {
          const read = h.feedback.console.read(req.since ?? 0, Math.min(Math.max(req.limit, 1), 200));
          return { kind: "console", entries: read.entries, next_seq: read.nextSeq, dropped: read.dropped };
        }
        case "network": {
          const read = h.feedback.network.read(req.since ?? 0, Math.min(Math.max(req.limit, 1), 300));
          const entries = req.failed_only ? read.entries.filter((e) => e.failed !== undefined || e.status >= 400) : read.entries;
          return { kind: "network", entries, next_seq: read.nextSeq, dropped: read.dropped };
        }
        case "activity":
          return { kind: "activity", inflight: h.feedback.inflight, quiet_ms: h.feedback.quietMs(), change_seq: h.batcher.changeSeq, loading: h.view.webContents.isLoading() };
        case "isolation":
          return this.isolation(h);
        case "close": {
          const state = this.state(h);
          this.release(h);
          return { kind: "state", state };
        }
        default:
          return { kind: "error", code: "UNSUPPORTED", message: (req as { kind: string }).kind };
      }
    };
    let response: unknown;
    try {
      response = await answer();
    } catch (e) {
      response = e instanceof CdpDenied ? { kind: "error", code: "CDP_DENIED", message: e.message } : { kind: "error", code: "CDP", message: (e as Error).message.slice(0, 500) };
    }
    const rr = response as { kind: string; code?: string; message?: string };
    if (!background) {
      this.log.push({ browserSessionId: bsid, kind: req.kind, ok: rr.kind !== "error", code: rr.code ?? "", generation: Number(r.leaseGeneration), atMs: Date.now(), ...(rr.kind === "error" && rr.message ? { detail: rr.message.slice(0, 200) } : {}) });
      if (this.log.length > 500) this.log.splice(0, this.log.length - 500);
      if (h && rr.kind !== "error") this.notify("browser:state", { browserSessionId: bsid, ...this.state(h), lastRequest: req.kind });
    }
    await c.respondBrowserHost(r.requestId, response).catch(() => {});
  }

  private async navigate(h: HostedSession, url: string): Promise<unknown> {
    if (!/^https?:\/\/\S+$/i.test(url)) return { kind: "error", code: "NAVIGATION_BLOCKED", message: `${url} is not an http(s) URL` };
    const rules = this.policy.targetRules();
    // PX-120: the destination, before anything is sent - what the URL says, then what its name resolves to.
    const literal = checkUrlLiteral(rules, url);
    const refuse = (reason: string, cls: string): unknown => ({ kind: "error", code: "TARGET_NOT_ALLOWED", message: `${reason} (${cls}); nothing was loaded` });
    if (!literal.ok) {
      this.onDenied(h, { host: new URL(url).hostname, port: Number(new URL(url).port) || 0, url, class: literal.class, address: literal.address ?? "", reason: literal.reason, atMs: Date.now() });
      return refuse(literal.reason, describeClass(literal.class));
    }
    const u = new URL(url);
    const pinned = await h.proxy.pin(u.hostname, Number(u.port) || (u.protocol === "https:" ? 443 : 80));
    if (!pinned.ok && pinned.denial) return refuse(pinned.denial.reason, describeClass(pinned.denial.class));
    const wc = h.view.webContents;
    const started = Date.now();
    h.docRequests.push({ atMs: started, host: u.hostname, port: Number(u.port) || (u.protocol === "https:" ? 443 : 80), url, type: "mainFrame" });
    // A certificate error holds the load for the person; the Core hears at once that it is waiting, not after its own deadline.
    const certWaiting = new Promise<"cert">((res) => {
      h.onCertHold = () => res("cert");
    });
    const loading = wc.loadURL(url).then(
      () => "loaded" as const,
      (e: Error) => e,
    );
    const first = await Promise.race([loading, certWaiting]);
    h.onCertHold = null;
    if (first === "cert") {
      void loading.catch(() => {});
      const info = h.certHold?.info;
      return { kind: "error", code: "CERTIFICATE_PENDING", message: `the certificate of ${info?.hostPort ?? u.host} is not trusted (${info?.error ?? "certificate error"}); issuer ${info?.issuer ?? "?"}, subject ${info?.subject ?? "?"}, fingerprint ${info?.fingerprint ?? "?"}: the person decides in the Browser panel` };
    }
    // A hop of the navigation the proxy or a redirect check refused.
    const refused = this.mainFrameRefusal(h, started);
    if (refused) return refuse(`${refused.reason}; the page now shows the policy's refusal`, describeClass(refused.class));
    // A load aborted by another navigation of the same page (a held load the person just let through
    // finishing as this one started) is not a failure of this one: what the page settles on is the answer.
    if (first !== "loaded" && /\(-3\)/.test(first.message)) {
      const until = Date.now() + 8_000;
      while (wc.isLoading() && Date.now() < until) await sleep(50);
      if (!wc.isLoading() && wc.getURL().startsWith(u.origin)) return { kind: "state", state: this.state(h) };
    }
    if (first !== "loaded") {
      const rejected = h.certHold && h.certHold.isSettled ? "CERTIFICATE_REJECTED" : null;
      if (rejected) return { kind: "error", code: "CERTIFICATE_REJECTED", message: `the person did not trust the certificate (${first.message.slice(0, 120)})` };
      // A load that failed still leaves the page where it is: report it, do not guess.
      return { kind: "error", code: "NAVIGATION_FAILED", message: first.message.slice(0, 300) };
    }
    return { kind: "state", state: this.state(h) };
  }

  /** The funnel every DevTools-protocol command passes through (PX-073): the deny list is applied before anything is sent. */
  private async send(h: HostedSession, method: string, params: Record<string, unknown> = {}, opts: { sessionId?: string; structural?: boolean } = {}): Promise<any> {
    try {
      guardCdp(method, opts.structural === true);
    } catch (e) {
      this.log.push({ browserSessionId: h.browserSessionId, kind: "cdp-denied", ok: false, code: method, generation: h.leaseGeneration, atMs: Date.now() });
      throw e;
    }
    const d = h.view.webContents.debugger;
    return opts.sessionId ? d.sendCommand(method, params, opts.sessionId) : d.sendCommand(method, params);
  }

  /** Attach the DevTools bridge (once) and enable what the host reads: events, the observer, out-of-process frames. */
  private async ensureCdp(h: HostedSession): Promise<void> {
    const wc = h.view.webContents;
    const d = wc.debugger;
    if (d.isAttached() && h.cdpReady) return;
    if (!d.isAttached()) {
      d.attach("1.3");
      if (!this.boundDebuggers.has(d)) {
        this.boundDebuggers.add(d);
        d.on("message", (_e, method, params, sessionId) => this.onCdpEvent(h, method, params as Record<string, unknown>, sessionId));
      }
    }
    h.cdpReady = true;
    const ok = (p: Promise<unknown>) => p.catch(() => {});
    // IMP-EV-0089: a JavaScript dialog is a modal the agent cannot act
    // around; the host records it open and closed.
    await ok(this.send(h, "Page.enable"));
    await this.enableObservation(h);
    await ok(this.send(h, "Target.setAutoAttach", { autoAttach: true, waitForDebuggerOnStart: false, flatten: true }, { structural: true }));
  }

  /** The domains the host reads, and the observer script, on one DevTools session (the page's or a frame's). */
  private async enableObservation(h: HostedSession, sessionId?: string): Promise<void> {
    const opts = sessionId ? { sessionId } : {};
    const ok = (p: Promise<unknown>) => p.catch(() => {});
    await ok(this.send(h, "DOM.enable", {}, opts));
    await ok(this.send(h, "Accessibility.enable", {}, opts));
    await ok(this.send(h, "Runtime.enable", {}, opts));
    await ok(this.send(h, "Log.enable", {}, opts));
    await ok(this.send(h, "Network.enable", {}, opts));
    await ok(this.send(h, "Runtime.addBinding", { name: OBSERVER_BINDING, executionContextName: OBSERVER_WORLD }, opts));
    await ok(this.send(h, "Page.addScriptToEvaluateOnNewDocument", { source: OBSERVER_SOURCE, worldName: OBSERVER_WORLD, runImmediately: true }, opts));
  }

  private onCdpEvent(h: HostedSession, method: string, params: Record<string, unknown>, sessionId?: string): void {
    if (method === "Page.javascriptDialogOpening" && !sessionId) {
      const p = params as { type?: unknown; message?: unknown } | undefined;
      h.dialogOpen = true;
      h.dialogSeen = { type: String(p?.type ?? "dialog"), message: String(p?.message ?? "").slice(0, 200) };
      this.log.push({ browserSessionId: h.browserSessionId, kind: "dialog", ok: true, code: String(p?.type ?? "open"), generation: h.leaseGeneration, atMs: Date.now() });
      return;
    }
    if (method === "Page.javascriptDialogClosed" && !sessionId) {
      h.dialogOpen = false;
      return;
    }
    if (method === "Runtime.bindingCalled") {
      const p = params as { name?: string; payload?: string };
      if (p.name !== OBSERVER_BINDING) return;
      try {
        const r = JSON.parse(String(p.payload)) as { kind?: string; added?: number; removed?: number; attributes?: number; text?: number; focus?: boolean; coalesced?: number };
        h.batcher.report({ ...r, frame: sessionId ? (h.children.get(sessionId)?.targetId ?? null) : null });
      } catch {
        // a report the host cannot read is no notice
      }
      return;
    }
    if (method === "Target.attachedToTarget") {
      const p = params as { sessionId?: string; targetInfo?: { targetId: string; type: string; url: string; parentFrameId?: string } };
      if (!p.sessionId || !p.targetInfo) return;
      const info = p.targetInfo;
      if (info.type === "iframe" || info.type === "page") {
        h.children.set(p.sessionId, { targetId: info.targetId, type: info.type, url: info.url, ...(info.parentFrameId ? { parentFrameId: info.parentFrameId } : {}) });
        void this.enableObservation(h, p.sessionId).then(() => this.send(h, "Target.setAutoAttach", { autoAttach: true, waitForDebuggerOnStart: false, flatten: true }, { sessionId: p.sessionId!, structural: true })).catch(() => {});
      }
      return;
    }
    if (method === "Target.detachedFromTarget") {
      const p = params as { sessionId?: string };
      if (p.sessionId) h.children.delete(p.sessionId);
      return;
    }
    if (method === "Target.targetInfoChanged") {
      const p = params as { targetInfo?: { targetId: string; url: string } };
      for (const c of h.children.values()) if (c.targetId === p.targetInfo?.targetId) c.url = p.targetInfo.url;
      return;
    }
    h.feedback.ingest(method, params, sessionId);
  }

  private async snapshot(h: HostedSession, maxNodes: number): Promise<unknown> {
    await this.ensureCdp(h);
    const limit = Math.max(1, Math.min(maxNodes || 400, 400));
    const changeSeq = h.batcher.changeSeq;
    // The frames: the page's own (same-process frames included) and the
    // out-of-process ones the DevTools session auto-attached.
    const tree = (await this.send(h, "Page.getFrameTree")) as { frameTree: FrameTree };
    const topFrame = tree.frameTree.frame;
    const sameProcess: { frame: FrameTree["frame"]; parentId?: string }[] = [];
    const walk = (t: FrameTree, parentId?: string) => {
      if (parentId !== undefined) sameProcess.push({ frame: t.frame, parentId });
      for (const c of t.childFrames ?? []) walk(c, t.frame.id);
    };
    walk(tree.frameTree);
    const doc = (await this.send(h, "DOM.getDocument", { depth: -1, pierce: true }).catch(() => null)) as { root: DomNode } | null;
    const topDom: DomIndex = doc ? indexDom(doc.root, topFrame.id) : { byBackend: new Map(), owners: new Map(), walked: 0 };
    const topAx = (await this.send(h, "Accessibility.getFullAXTree")) as { nodes: AxNode[] };
    const top = mergeAx(topAx.nodes, { prefix: "", limit, dom: topDom });
    const nodes: RawNode[] = [...top.nodes];
    const frames: { key: string; origin: string; url: string; name: string; parent: string | null; oopif: boolean }[] = [];
    const recs = new Map<string, FrameRec>();
    const axByBackend = (list: RawNode[]) => new Map(list.filter((n) => n.backend_dom_node_id !== null).map((n) => [n.backend_dom_node_id as number, n.id]));
    const topByBackend = axByBackend(top.nodes);
    const frameRoots = new Map<string, { rootId: string | null; byBackend: Map<number, string>; key: string | undefined; sessionId?: string }>();
    frameRoots.set(topFrame.id, { rootId: top.rootId, byBackend: topByBackend, key: undefined });
    let truncated = top.truncated;
    let n = 0;
    const frameLimit = Math.min(100, Math.max(20, Math.floor(limit / 2)));
    const originOf = (u: string): string => {
      try {
        const x = new URL(u);
        return x.protocol === "http:" || x.protocol === "https:" ? x.origin.toLowerCase() : "";
      } catch {
        return "";
      }
    };
    // Same-process child frames, in the order the page lists them.
    for (const { frame, parentId } of sameProcess) {
      if (n >= 8) break;
      n += 1;
      const key = `f${n}`;
      const parentRoot = frameRoots.get(parentId ?? topFrame.id);
      const owner = topDom.owners.get(frame.id);
      let ax: { nodes: AxNode[] };
      try {
        ax = (await this.send(h, "Accessibility.getFullAXTree", { frameId: frame.id })) as { nodes: AxNode[] };
      } catch {
        continue;
      }
      const rootParent = (owner && parentRoot?.byBackend.get(owner.ownerBackendId)) ?? parentRoot?.rootId ?? null;
      const origin = frame.securityOrigin && frame.securityOrigin !== "://" ? frame.securityOrigin.toLowerCase() : originOf(frame.url);
      const merged = mergeAx(ax.nodes, { prefix: `${key}:`, frame: key, limit: frameLimit, dom: topDom, rootParent, rootRole: "iframe", rootName: frame.name || origin || frame.url });
      nodes.push(...merged.nodes);
      truncated ||= merged.truncated;
      frameRoots.set(frame.id, { rootId: merged.rootId, byBackend: axByBackend(merged.nodes), key });
      frames.push({ key, origin, url: frame.url, name: frame.name ?? "", parent: parentRoot?.key ?? null, oopif: false });
      recs.set(key, { key, frameId: frame.id, ...(parentRoot?.key ? { parentKey: parentRoot.key } : {}), ...(owner ? { ownerBackendId: owner.ownerBackendId } : {}), origin });
    }
    // Out-of-process frames: each its own DevTools session.
    for (const [sid, child] of h.children) {
      if (n >= 8) break;
      if (child.type !== "iframe") continue;
      n += 1;
      const key = `f${n}`;
      try {
        const [axr, ft, cdoc] = await Promise.all([
          this.send(h, "Accessibility.getFullAXTree", {}, { sessionId: sid }) as Promise<{ nodes: AxNode[] }>,
          this.send(h, "Page.getFrameTree", {}, { sessionId: sid }) as Promise<{ frameTree: FrameTree }>,
          this.send(h, "DOM.getDocument", { depth: -1, pierce: true }, { sessionId: sid }).catch(() => null) as Promise<{ root: DomNode } | null>,
        ]);
        const frame = ft.frameTree.frame;
        const dom = cdoc ? indexDom(cdoc.root, frame.id) : { byBackend: new Map(), owners: new Map(), walked: 0 };
        const parentFrameId = child.parentFrameId ?? topFrame.id;
        const parentRoot = frameRoots.get(parentFrameId) ?? frameRoots.get(topFrame.id);
        const owner = topDom.owners.get(child.targetId) ?? topDom.owners.get(frame.id);
        const rootParent = (owner && parentRoot?.byBackend.get(owner.ownerBackendId)) ?? parentRoot?.rootId ?? null;
        const origin = originOf(frame.url) || originOf(child.url);
        const merged = mergeAx(axr.nodes, { prefix: `${key}:`, frame: key, limit: frameLimit, dom, rootParent, rootRole: "iframe", rootName: frame.name || origin || frame.url });
        nodes.push(...merged.nodes);
        truncated ||= merged.truncated;
        frameRoots.set(frame.id, { rootId: merged.rootId, byBackend: axByBackend(merged.nodes), key, sessionId: sid });
        frames.push({ key, origin, url: frame.url || child.url, name: frame.name ?? "", parent: parentRoot?.key ?? null, oopif: true });
        recs.set(key, { key, frameId: frame.id, sessionId: sid, ...(parentRoot?.key ? { parentKey: parentRoot.key } : {}), ...(owner ? { ownerBackendId: owner.ownerBackendId } : {}), origin });
      } catch {
        // a frame that went away while it was read is not on the page now
      }
    }
    h.frames = recs;
    // The layout box of what can be acted on (bounded: the first 80
    // actionable nodes of the page's own frame), in CSS pixels of the view.
    const actionable = new Set(["button", "link", "textbox", "searchbox", "combobox", "checkbox", "radio", "slider", "spinbutton", "listbox", "menuitem", "tab", "option", "switch", "canvas", "image", "img", "figure", "graphics-document"]);
    const targets = top.nodes.filter((x) => x.backend_dom_node_id !== null && actionable.has(x.role.toLowerCase())).slice(0, 80);
    for (let i = 0; i < targets.length; i += 20) {
      await Promise.all(
        targets.slice(i, i + 20).map(async (t) => {
          try {
            const bm = (await this.send(h, "DOM.getBoxModel", { backendNodeId: t.backend_dom_node_id })) as { model: { border: number[] } };
            const q = bm.model.border;
            const xs = [q[0]!, q[2]!, q[4]!, q[6]!];
            const ys = [q[1]!, q[3]!, q[5]!, q[7]!];
            const x = Math.min(...xs);
            const y = Math.min(...ys);
            t.bounds = { x: Math.max(0, Math.round(x)), y: Math.max(0, Math.round(y)), width: Math.max(0, Math.round(Math.max(...xs) - x)), height: Math.max(0, Math.round(Math.max(...ys) - y)) };
          } catch {
            // no box (detached or hidden): the entity stays, unlocated
          }
        }),
      );
    }
    return { kind: "snapshot", state: this.state(h), nodes, truncated, frames: frames.map((f) => ({ key: f.key, origin: f.origin, url: f.url, name: f.name, parent: f.parent, oopif: f.oopif })), change_seq: changeSeq };
  }

  private async capture(h: HostedSession, clip: Clip | null, fit: [number, number] | null): Promise<unknown> {
    // A view hidden with the panel is off the window, and off the window
    // Chromium composites no frame on Linux and Windows: neither CDP's
    // `Page.captureScreenshot` nor `capturePage` ever answers (hosted CI).
    // For the capture the view is parked back in the window — first beside
    // it (out of sight, same layout), and only if that yields no frame
    // either, in place for the instant the capture takes — then hidden
    // again. The image is normalized to CSS pixels (the clip's size, or
    // the view's) so a HiDPI display does not double it, then scaled down
    // to fit the budget; a capture that takes too long is an error the
    // Core hears, never a request left unanswered.
    const wc = h.view.webContents;
    const rect = clip ? { x: Math.round(clip.x), y: Math.round(clip.y), width: Math.max(1, Math.round(clip.width)), height: Math.max(1, Math.round(clip.height)) } : undefined;
    const attempt = async (): Promise<Electron.NativeImage> => {
      const timeout = new Promise<never>((_, reject) => setTimeout(() => reject(new Error("CAPTURE_TIMEOUT")), 7_000));
      return Promise.race([wc.capturePage(rect, { stayHidden: true, stayAwake: true }), timeout]);
    };
    const win = this.window();
    let image: Electron.NativeImage | null = null;
    let code = "CAPTURE_FAILED";
    let message = "";
    try {
      if (h.shown || !win) {
        image = await attempt();
      } else {
        const width = win.getContentSize()[0] ?? 1024;
        win.contentView.addChildView(h.view);
        try {
          h.view.setBounds({ ...h.lastBounds, x: width + 64 });
          try {
            image = await attempt();
          } catch (e) {
            if ((e as Error).message !== "CAPTURE_TIMEOUT") throw e;
            h.view.setBounds(h.lastBounds);
            image = await attempt();
          }
        } finally {
          win.contentView.removeChildView(h.view);
        }
      }
    } catch (e) {
      code = (e as Error).message === "CAPTURE_TIMEOUT" ? "CAPTURE_TIMEOUT" : "CAPTURE_FAILED";
      message = (e as Error).message.slice(0, 300);
    }
    if (!image) return { kind: "error", code, message };
    if (image.isEmpty()) return { kind: "error", code: "CAPTURE_EMPTY", message: "the page produced no frame" };
    // The size in CSS pixels: the clip's, or the view's own.
    const want = rect ? { width: rect.width, height: rect.height } : { width: Math.max(1, h.lastBounds.width), height: Math.max(1, h.lastBounds.height) };
    let size = image.getSize();
    if (size.width !== want.width || size.height !== want.height) image = image.resize({ width: want.width, height: want.height });
    size = image.getSize();
    if (fit && (size.width > fit[0] || size.height > fit[1])) {
      const k = Math.min(fit[0] / size.width, fit[1] / size.height);
      image = image.resize({ width: Math.max(1, Math.floor(size.width * k)), height: Math.max(1, Math.floor(size.height * k)) });
      size = image.getSize();
    }
    return { kind: "capture", state: this.state(h), png_base64: image.toPNG().toString("base64"), clip, width: size.width, height: size.height };
  }

  /** The DevTools session and frame record an action on `frameKey` goes through. */
  private frameCtx(h: HostedSession, frameKey: string | null): { sessionId?: string; rec?: FrameRec } {
    if (!frameKey) return {};
    const rec = h.frames.get(frameKey);
    if (!rec) return {};
    return { ...(rec.sessionId ? { sessionId: rec.sessionId } : {}), rec };
  }

  /** The offset of a frame's viewport in the page's viewport: the sum of its iframe elements' content boxes up the chain. */
  private async frameOffset(h: HostedSession, rec: FrameRec | undefined): Promise<{ x: number; y: number }> {
    let x = 0;
    let y = 0;
    let cur = rec;
    let guard = 0;
    while (cur && cur.ownerBackendId !== undefined && guard++ < 8) {
      const parent = cur.parentKey ? h.frames.get(cur.parentKey) : undefined;
      const sid = parent?.sessionId;
      const opts = sid ? { sessionId: sid } : {};
      const resolved = (await this.send(h, "DOM.resolveNode", { backendNodeId: cur.ownerBackendId }, opts)) as { object: { objectId: string } };
      const r = (await this.send(h, "Runtime.callFunctionOn", { objectId: resolved.object.objectId, functionDeclaration: "function() { const r = this.getBoundingClientRect(); return { x: r.left + this.clientLeft, y: r.top + this.clientTop }; }", returnByValue: true }, opts)) as { result: { value?: { x: number; y: number } } };
      x += r.result.value?.x ?? 0;
      y += r.result.value?.y ?? 0;
      cur = parent;
    }
    return { x, y };
  }

  /**
   * PX-121: scroll an element into view, the page, or the nearest scrollable
   * container of an element - by script in the element's own frame, never by
   * input the person's hook would mistake for theirs - and report where it
   * is now. Lazy content gets a short quiet period to load.
   */
  private async scroll(h: HostedSession, backendNodeId: number | null, frameKey: string | null, mode: string, dx: number, dy: number): Promise<unknown> {
    await this.ensureCdp(h);
    const { sessionId } = this.frameCtx(h, frameKey);
    const opts = sessionId ? { sessionId } : {};
    const goneBefore = h.goneCount;
    let objectId: string;
    try {
      if (backendNodeId !== null) {
        const r = (await this.send(h, "DOM.resolveNode", { backendNodeId }, opts)) as { object: { objectId: string } };
        objectId = r.object.objectId;
      } else {
        const r = (await this.send(h, "Runtime.evaluate", { expression: "document.documentElement" })) as { result: { objectId: string } };
        objectId = r.result.objectId;
      }
    } catch (e) {
      return { kind: "error", code: "TARGET_GONE", message: `the element is no longer in the document (${(e as Error).message.slice(0, 120)})` };
    }
    const clampedX = Math.max(-5000, Math.min(5000, Math.round(dx)));
    const clampedY = Math.max(-5000, Math.min(5000, Math.round(dy)));
    let value: { x: number; y: number; maxX: number; maxY: number; moved: boolean; container: string } | undefined;
    try {
      const r = (await this.send(h, "Runtime.callFunctionOn", { objectId, functionDeclaration: SCROLL_FN, arguments: [{ value: mode }, { value: clampedX }, { value: clampedY }], returnByValue: true }, opts)) as { result: { value?: typeof value }; exceptionDetails?: { text?: string } };
      if (r.exceptionDetails) return { kind: "error", code: "SCROLL_NOT_POSSIBLE", message: `the element cannot be scrolled (${r.exceptionDetails.text ?? "script error"})` };
      value = r.result.value;
    } catch (e) {
      if (h.goneCount !== goneBefore || h.view.webContents.isCrashed()) return { kind: "error", code: "OUTCOME_UNKNOWN", message: "the page's process failed during the scroll" };
      throw e;
    }
    if (!value) return { kind: "error", code: "SCROLL_NOT_POSSIBLE", message: "the scroll reported nothing" };
    // Lazy content: a short quiet period for what the scroll triggers.
    await sleep(250);
    if (h.goneCount !== goneBefore || h.view.webContents.isCrashed()) return { kind: "error", code: "OUTCOME_UNKNOWN", message: "the page's process failed during the scroll" };
    return { kind: "scrolled", state: this.state(h), x: value.x, y: value.y, max_x: value.maxX, max_y: value.maxY, moved: value.moved, container: value.container };
  }

  /**
   * M7.4: act on the DOM node the compiler resolved, as a person would — a
   * real click at the element's box, text insertion after a focus, real key
   * events — then wait for the page to settle (a navigation it started, or
   * a short quiet period) and report the state. The node id is the one the
   * Core resolved at the current version; a node that is gone is an error,
   * never a click elsewhere.
   */
  private async act(h: HostedSession, backendNodeId: number, action: string, value: string, key: string, at: [number, number] | null, credentialHandle: string | null, frameKey: string | null): Promise<unknown> {
    const wc = h.view.webContents;
    await this.ensureCdp(h);
    const { sessionId, rec } = this.frameCtx(h, frameKey);
    if (frameKey && !rec) return { kind: "error", code: "TARGET_STALE", message: `frame ${frameKey} is not part of the page last read` };
    const sopts = sessionId ? { sessionId } : {};
    await this.send(h, "DOM.enable", {}, sopts).catch(() => {});
    let objectId: string;
    try {
      const r = (await this.send(h, "DOM.resolveNode", { backendNodeId }, sopts)) as { object: { objectId: string } };
      objectId = r.object.objectId;
    } catch (e) {
      return { kind: "error", code: "TARGET_GONE", message: `the element is no longer in the document (${(e as Error).message.slice(0, 120)})` };
    }
    const versionBefore = h.stateVersion;
    const goneBefore = h.goneCount;
    const started = Date.now();
    h.permissionsAsked.length = 0;
    h.dialogSeen = null;
    const call = async (fn: string, args: unknown[] = []) => (await this.send(h, "Runtime.callFunctionOn", { objectId, functionDeclaration: fn, arguments: args.map((a) => ({ value: a })), returnByValue: true }, sopts)) as { result: { value?: unknown }; exceptionDetails?: { text?: string } };
    // A navigation the action starts is watched from before the action is
    // dispatched: a local page can start and finish its load between the
    // click and a listener attached afterwards, and a settle that then
    // waits for a load already done outlives the Core's deadline (hosted
    // Windows: the approved sign-in never answered, the outcome unknown).
    let navStarted = false;
    let navDone = false;
    let onNavDone: (() => void) | null = null;
    const onStart = () => {
      navStarted = true;
    };
    const onDone = () => {
      navDone = true;
      onNavDone?.();
    };
    wc.on("did-start-navigation", onStart);
    wc.on("did-finish-load", onDone);
    wc.on("did-fail-load", onDone);
    const settle = async (): Promise<boolean> => {
      // A navigation the action started settles at its load (bounded well
      // under the Core's deadline); otherwise the page gets a short quiet
      // period for its own updates.
      const t0 = Date.now();
      while (!navStarted && Date.now() - t0 < 400) await sleep(25);
      if (navStarted) {
        if (!navDone) await Promise.race([new Promise<void>((r) => (onNavDone = r)), sleep(8_000)]);
      } else {
        await sleep(250);
      }
      return navStarted || h.stateVersion !== versionBefore;
    };
    const st = { dispatched: false };
    try {
      const out = await this.perform(h, call, action, value, key, at, credentialHandle, backendNodeId, settle, sessionId, rec, st);
      // IMP-EV-0081 / PX-121: the page's process died while the input was in
      // flight - whatever came back, nobody knows what the input did.
      if (st.dispatched && (h.goneCount !== goneBefore || wc.isCrashed())) return { kind: "error", code: "OUTCOME_UNKNOWN", message: `the page's process failed during the ${action}; the input may have been delivered` };
      // PX-120: the action led the page to a destination the policy refuses.
      const refused = this.mainFrameRefusal(h, started);
      if (refused && (out as { kind?: string }).kind === "acted") return { kind: "error", code: "TARGET_NOT_ALLOWED", message: `${refused.reason} (${describeClass(refused.class)}); the action led the page to a destination the policy refuses` };
      return out;
    } catch (e) {
      if (e instanceof CdpDenied) throw e;
      if (st.dispatched) return { kind: "error", code: "OUTCOME_UNKNOWN", message: `the ${action} failed after the input was delivered (${(e as Error).message.slice(0, 160)}); its effect is unknown` };
      throw e;
    } finally {
      wc.removeListener("did-start-navigation", onStart);
      wc.removeListener("did-finish-load", onDone);
      wc.removeListener("did-fail-load", onDone);
    }
  }

  private async perform(h: HostedSession, call: (fn: string, args?: unknown[]) => Promise<{ result: { value?: unknown }; exceptionDetails?: { text?: string } }>, action: string, value: string, key: string, at: [number, number] | null, credentialHandle: string | null, backendNodeId: number, settle: () => Promise<boolean>, sessionId: string | undefined, rec: FrameRec | undefined, st: { dispatched: boolean }): Promise<unknown> {
    const wc = h.view.webContents;
    const sopts = sessionId ? { sessionId } : {};
    let detail = "";
    switch (action) {
      case "click":
      case "check":
      case "uncheck": {
        if (action !== "click") {
          const checked = await call("function() { return this.checked === true; }");
          if ((checked.result.value === true) === (action === "check")) {
            return { kind: "acted", state: this.state(h), navigated: false, detail: `already ${action}ed` };
          }
        }
        await this.send(h, "DOM.scrollIntoViewIfNeeded", { backendNodeId }, sopts).catch(() => {});
        // The element's box in viewport coordinates (what a click and a
        // hit test both use); the centre, or the point the model chose
        // inside it (a visual region, M7.5) — clamped to the box, never outside it.
        const box = await call("function() { const r = this.getBoundingClientRect(); return { left: r.left, top: r.top, right: r.right, bottom: r.bottom }; }");
        const b = box.result.value as { left: number; top: number; right: number; bottom: number } | undefined;
        if (!b || b.right - b.left <= 0 || b.bottom - b.top <= 0) return { kind: "error", code: "TARGET_OCCLUDED", message: "the element has no visible box (hidden or collapsed)" };
        const lx = at ? Math.min(b.right - 1, Math.max(b.left, b.left + at[0])) : (b.left + b.right) / 2;
        const ly = at ? Math.min(b.bottom - 1, Math.max(b.top, b.top + at[1])) : (b.top + b.bottom) / 2;
        // IMP-EV-0089: what is at the point must be the element or inside it —
        // an overlay, a menu or a dialog over it makes the click TARGET_OCCLUDED.
        const hit = await call("function(x, y) { const el = (this.ownerDocument || document).elementFromPoint(x, y); if (!el) return 'NONE'; return (el === this || this.contains(el) || el.contains(this)) ? 'ok' : (el.tagName + (el.id ? '#' + el.id : '')); }", [lx, ly]);
        if (hit.result.value !== "ok") return { kind: "error", code: "TARGET_OCCLUDED", message: `${hit.result.value === "NONE" ? "nothing" : String(hit.result.value)} is at the click point, over the element` };
        // A frame's element is in its frame's viewport: the input goes in the page's.
        const off = rec ? await this.frameOffset(h, rec) : { x: 0, y: 0 };
        const x = lx + off.x;
        const y = ly + off.y;
        this.agentInputInFlight += 1;
        try {
          // A click whose handler opens a dialog holds the renderer, and the
          // input's acknowledgement with it: the dispatch is not waited for
          // past a bound — the dialog is reported on the next request.
          const bounded = <T,>(p: Promise<T>) => Promise.race([p, new Promise<void>((r) => setTimeout(r, 1_500))]);
          st.dispatched = true;
          await bounded(this.send(h, "Input.dispatchMouseEvent", { type: "mouseMoved", x, y }, { structural: true }));
          await bounded(this.send(h, "Input.dispatchMouseEvent", { type: "mousePressed", x, y, button: "left", clickCount: 1 }, { structural: true }));
          await bounded(this.send(h, "Input.dispatchMouseEvent", { type: "mouseReleased", x, y, button: "left", clickCount: 1 }, { structural: true }));
        } finally {
          this.agentInputInFlight -= 1;
          this.agentInputUntil = Date.now() + 250;
        }
        detail = `${action} at ${Math.round(x)},${Math.round(y)}`;
        break;
      }
      case "fill": {
        // IMP-EV-0089 / IMP-EV-0090: the destination is verified as a text
        // entry before anything is replaced (never a button, a checkbox, a
        // read-only or disabled field); text goes in as typed input, never
        // through the clipboard.
        const editable = await call(EDITABLE_CHECK);
        if (editable.result.value !== "ok") return { kind: "error", code: "TARGET_NOT_EDITABLE", message: `the element does not take text (${String(editable.result.value)})` };
        this.agentInputInFlight += 1;
        try {
          await this.send(h, "DOM.focus", { backendNodeId }, sopts);
          st.dispatched = true;
          await call("function() { if ('value' in this) { this.value = ''; this.dispatchEvent(new Event('input', {bubbles:true})); } else if (this.isContentEditable) { this.textContent = ''; } }");
          if (value.length > 0) await this.send(h, "Input.insertText", { text: value }, { structural: true });
          await call("function() { this.dispatchEvent(new Event('change', {bubbles:true})); }");
        } finally {
          this.agentInputInFlight -= 1;
          this.agentInputUntil = Date.now() + 250;
        }
        detail = `inserted ${value.length} chars`;
        break;
      }
      case "fill_credential": {
        // M7.8 (docs/22 "Credentials"): the value comes from the broker's
        // custody for this page's origin only, goes into the field through
        // the same path a typed value takes, and is never returned or logged.
        const pageOrigin = (() => {
          try {
            const u = new URL(rec?.origin ? rec.origin : wc.getURL());
            return `${u.protocol}//${u.host}`.toLowerCase();
          } catch {
            return "";
          }
        })();
        const secret = credentialHandle ? this.secretFor(credentialHandle, pageOrigin) : null;
        if (secret === null) return { kind: "error", code: "CREDENTIAL_UNAVAILABLE", message: `no credential ${credentialHandle ?? ""} for ${pageOrigin || "this page"}` };
        const tag = await call(EDITABLE_CHECK);
        if (tag.result.value !== "ok") return { kind: "error", code: "TARGET_NOT_EDITABLE", message: `a credential is filled into a text field (${String(tag.result.value)})` };
        this.agentInputInFlight += 1;
        try {
          await this.send(h, "DOM.focus", { backendNodeId }, sopts);
          st.dispatched = true;
          await call("function() { this.value = ''; this.dispatchEvent(new Event('input', {bubbles:true})); }");
          await this.send(h, "Input.insertText", { text: secret }, { structural: true });
          await call("function() { this.dispatchEvent(new Event('change', {bubbles:true})); }");
        } finally {
          this.agentInputInFlight -= 1;
          this.agentInputUntil = Date.now() + 250;
        }
        detail = `filled credential ${credentialHandle}`;
        break;
      }
      case "select": {
        st.dispatched = true;
        const r = await call("function(v) { if (this.tagName !== 'SELECT') return 'NOT_SELECT'; const o = [...this.options].find(o => o.value === v || o.textContent.trim() === v); if (!o) return 'NO_OPTION'; this.value = o.value; this.dispatchEvent(new Event('input', {bubbles:true})); this.dispatchEvent(new Event('change', {bubbles:true})); return 'ok'; }", [value]);
        if (r.result.value !== "ok") return { kind: "error", code: String(r.result.value ?? "SELECT_FAILED"), message: `select ${JSON.stringify(value)}` };
        detail = `selected ${JSON.stringify(value)}`;
        break;
      }
      case "press": {
        await this.send(h, "DOM.focus", { backendNodeId }, sopts).catch(() => {});
        const codes: Record<string, number> = { Enter: 13, Tab: 9, Escape: 27, Backspace: 8, ArrowDown: 40, ArrowUp: 38, ArrowLeft: 37, ArrowRight: 39, Space: 32 };
        const code = codes[key];
        if (code === undefined) return { kind: "error", code: "UNSUPPORTED_KEY", message: key };
        const text = key === "Enter" ? "\r" : key === "Space" ? " " : undefined;
        this.agentInputInFlight += 1;
        try {
          st.dispatched = true;
          await this.send(h, "Input.dispatchKeyEvent", { type: text ? "keyDown" : "rawKeyDown", key, code: key, windowsVirtualKeyCode: code, nativeVirtualKeyCode: code, ...(text ? { text } : {}) }, { structural: true });
          await this.send(h, "Input.dispatchKeyEvent", { type: "keyUp", key, code: key, windowsVirtualKeyCode: code, nativeVirtualKeyCode: code }, { structural: true });
        } finally {
          this.agentInputInFlight -= 1;
          this.agentInputUntil = Date.now() + 250;
        }
        detail = `pressed ${key}`;
        break;
      }
      default:
        return { kind: "error", code: "UNSUPPORTED_ACTION", message: action };
    }
    const navigated = await settle();
    // IMP-EV-0089: a permission the action provoked was denied; the action
    // is reported as needing it, so the model does not chase a feature the
    // session never grants.
    if (h.permissionsAsked.length > 0) {
      const asked = h.permissionsAsked.splice(0);
      return { kind: "error", code: "PERMISSION_REQUIRED", message: `the page asked for ${asked.join(", ")} after the ${action}; the session grants no permission` };
    }
    // IMP-EV-0089: a dialog the action opened is the person's to answer —
    // it stays up in the view when the page has one; a view without a
    // window to show it in has it dismissed unanswered by Chromium (a
    // confirm answers "no", a prompt nothing). Either way the action is
    // reported as blocked by the modal, with what it asked.
    if (h.dialogSeen) {
      const d = h.dialogSeen;
      h.dialogSeen = null;
      return { kind: "error", code: "MODAL_BLOCKING", message: `the ${action} opened a ${d.type} dialog (${JSON.stringify(d.message)}); it is the person's to answer${h.dialogOpen ? " and is still open" : " and was dismissed unanswered"}` };
    }
    return { kind: "acted", state: this.state(h), navigated, detail };
  }

  /** The isolation report of a hosted view, for the renderer's Browser panel and the E2E. */
  async probe(browserSessionId: string): Promise<unknown> {
    const h = this.sessions.get(browserSessionId);
    if (!h || this.gone(h)) return null;
    return this.isolation(h);
  }

  /** Asked of the page itself: nothing privileged is reachable from the document. */
  private async isolation(h: HostedSession): Promise<unknown> {
    await this.ensureCdp(h);
    const r = (await this.send(h, "Runtime.evaluate", { expression: "typeof process !== 'undefined' || typeof require !== 'undefined' || typeof window.electron !== 'undefined' || typeof window.modbit !== 'undefined'", returnByValue: true })) as { result: { value?: unknown } };
    return { kind: "isolation", node_reachable: r.result.value === true, partition: h.partition, sandboxed: true, context_isolated: true };
  }
}

interface FrameTree {
  frame: { id: string; parentId?: string; url: string; name?: string; securityOrigin?: string };
  childFrames?: FrameTree[];
}

import { join } from "node:path";
