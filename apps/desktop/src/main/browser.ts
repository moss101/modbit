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
 */
import { BrowserWindow, session as electronSession, WebContentsView, type WebContents } from "electron";
import type { CoreClient } from "@modbit/ide-adapter-core";

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
}

interface PageState {
  url: string;
  title: string;
  ready: boolean;
  state_version: number;
}

type HostRequest =
  | { kind: "navigate"; url: string }
  | { kind: "state" }
  | { kind: "snapshot"; max_nodes: number }
  | { kind: "capture"; clip: { x: number; y: number; width: number; height: number } | null }
  | { kind: "isolation" }
  | { kind: "close" };

const HOST_KIND = "electron-main";

export class BrowserHost {
  private readonly sessions = new Map<string, HostedSession>();
  /** Delivery log of every request answered, for the renderer's Browser panel and the E2E. */
  readonly log: { browserSessionId: string; kind: string; ok: boolean; code: string; atMs: number }[] = [];

  constructor(
    private readonly window: () => BrowserWindow | null,
    private readonly client: () => CoreClient | null,
    private readonly notify: (channel: string, payload: unknown) => void,
  ) {}

  /** Open (or reuse) the task's session on the Core and attach a fresh view to it. */
  async open(sessionId: string, taskId: string): Promise<{ browserSessionId: string; partition: string; leaseGeneration: string }> {
    const c = this.client();
    if (!c) throw new Error("CORE_UNAVAILABLE: the local Core is restarting");
    const opened = await c.openBrowserSession(sessionId, taskId);
    const existing = this.sessions.get(opened.browserSessionId);
    if (existing && !existing.view.webContents.isDestroyed()) {
      if (!existing.attached) await this.attach(existing);
      return { browserSessionId: opened.browserSessionId, partition: opened.partition, leaseGeneration: opened.leaseGeneration.toString() };
    }
    const view = new WebContentsView({
      webPreferences: {
        partition: opened.partition,
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
    const hosted: HostedSession = { browserSessionId: opened.browserSessionId, taskId, sessionId, partition: opened.partition, view, stateVersion: 0, attached: false, shown: false };
    this.harden(hosted);
    this.sessions.set(hosted.browserSessionId, hosted);
    // A document exists from the start (the CDP session needs a target);
    // nothing of the network is touched until the agent navigates.
    await view.webContents.loadURL("about:blank").catch(() => {});
    hosted.stateVersion = 0;
    await this.attach(hosted);
    return { browserSessionId: opened.browserSessionId, partition: opened.partition, leaseGeneration: opened.leaseGeneration.toString() };
  }

  /** The page never gains anything: no new windows, no downloads, no permissions, no non-http(s) navigation. */
  private harden(h: HostedSession): void {
    const wc = h.view.webContents;
    wc.setWindowOpenHandler(() => ({ action: "deny" }));
    wc.on("will-navigate", (e, url) => {
      if (!/^https?:\/\//i.test(url)) e.preventDefault();
    });
    wc.on("will-redirect", (e, url) => {
      if (!/^https?:\/\//i.test(url)) e.preventDefault();
    });
    wc.on("did-navigate", () => this.bump(h));
    wc.on("did-navigate-in-page", () => this.bump(h));
    wc.on("did-finish-load", () => this.bump(h));
    wc.on("render-process-gone", (_e, details) => this.notify("browser:state", { browserSessionId: h.browserSessionId, gone: details.reason }));
    const s = electronSession.fromPartition(h.partition);
    s.setPermissionRequestHandler((_wc, _permission, cb) => cb(false));
    s.setPermissionCheckHandler(() => false);
    s.on("will-download", (e) => e.preventDefault());
    s.setDevicePermissionHandler(() => false);
  }

  private bump(h: HostedSession): void {
    h.stateVersion += 1;
    this.notify("browser:state", { browserSessionId: h.browserSessionId, ...this.state(h) });
  }

  private async attach(h: HostedSession): Promise<void> {
    const c = this.client();
    if (!c) throw new Error("CORE_UNAVAILABLE: the local Core is restarting");
    c.onBrowserRequest = (r) => void this.handle(r);
    await c.attachBrowserHost(h.browserSessionId, { hostKind: HOST_KIND, partition: h.partition, sandboxed: true, contextIsolated: true, nodeIntegration: false, taskId: h.taskId });
    h.attached = true;
  }

  /** After a Core restart every live view attaches again to its session (the same partition). */
  async reattachAll(): Promise<void> {
    for (const h of this.sessions.values()) {
      if (h.view.webContents.isDestroyed()) continue;
      h.attached = false;
      try {
        await this.attach(h);
      } catch (e) {
        this.notify("browser:state", { browserSessionId: h.browserSessionId, error: (e as Error).message });
      }
    }
  }

  private state(h: HostedSession): PageState {
    const wc = h.view.webContents;
    return { url: wc.getURL(), title: wc.getTitle(), ready: !wc.isLoading(), state_version: h.stateVersion };
  }

  /** Place the view over the renderer's placeholder (the renderer reports the rectangle). */
  show(browserSessionId: string, bounds: { x: number; y: number; width: number; height: number }): boolean {
    const h = this.sessions.get(browserSessionId);
    const win = this.window();
    if (!h || !win || h.view.webContents.isDestroyed()) return false;
    if (!h.shown) {
      win.contentView.addChildView(h.view);
      h.shown = true;
    }
    h.view.setBounds({ x: Math.round(bounds.x), y: Math.round(bounds.y), width: Math.max(0, Math.round(bounds.width)), height: Math.max(0, Math.round(bounds.height)) });
    return true;
  }

  hide(browserSessionId: string): void {
    const h = this.sessions.get(browserSessionId);
    const win = this.window();
    if (!h || !win || !h.shown) return;
    win.contentView.removeChildView(h.view);
    h.shown = false;
  }

  /** What this host holds (for the renderer and the E2E). */
  describe(browserSessionId: string): { browserSessionId: string; taskId: string; partition: string; attached: boolean; shown: boolean; url: string; title: string; stateVersion: number } | null {
    const h = this.sessions.get(browserSessionId);
    if (!h || h.view.webContents.isDestroyed()) return null;
    const s = this.state(h);
    return { browserSessionId: h.browserSessionId, taskId: h.taskId, partition: h.partition, attached: h.attached, shown: h.shown, url: s.url, title: s.title, stateVersion: h.stateVersion };
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
    if (!h.view.webContents.isDestroyed()) {
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
    const answer = async (): Promise<unknown> => {
      if (!h || h.view.webContents.isDestroyed()) return { kind: "error", code: "NO_SUCH_SESSION", message: `this host holds no view for ${bsid}` };
      switch (req.kind) {
        case "navigate":
          return this.navigate(h, req.url);
        case "state":
          return { kind: "state", state: this.state(h) };
        case "snapshot":
          return this.snapshot(h, req.max_nodes);
        case "capture":
          return this.capture(h, req.clip);
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
      response = { kind: "error", code: "CDP", message: (e as Error).message.slice(0, 500) };
    }
    const rr = response as { kind: string; code?: string };
    this.log.push({ browserSessionId: bsid, kind: req.kind, ok: rr.kind !== "error", code: rr.code ?? "", atMs: Date.now() });
    if (this.log.length > 500) this.log.splice(0, this.log.length - 500);
    if (h && rr.kind !== "error") this.notify("browser:state", { browserSessionId: bsid, ...this.state(h), lastRequest: req.kind });
    await c.respondBrowserHost(r.requestId, response).catch(() => {});
  }

  private async navigate(h: HostedSession, url: string): Promise<unknown> {
    if (!/^https?:\/\/\S+$/i.test(url)) return { kind: "error", code: "NAVIGATION_BLOCKED", message: `${url} is not an http(s) URL` };
    const wc = h.view.webContents;
    try {
      await wc.loadURL(url);
    } catch (e) {
      // A load that failed still leaves the page where it is: report it, do not guess.
      return { kind: "error", code: "NAVIGATION_FAILED", message: (e as Error).message.slice(0, 300) };
    }
    return { kind: "state", state: this.state(h) };
  }

  private async cdp(wc: WebContents): Promise<Electron.Debugger> {
    const d = wc.debugger;
    if (!d.isAttached()) d.attach("1.3");
    return d;
  }

  private async snapshot(h: HostedSession, maxNodes: number): Promise<unknown> {
    const wc = h.view.webContents;
    const d = await this.cdp(wc);
    await d.sendCommand("Accessibility.enable");
    await d.sendCommand("DOM.enable");
    type Ax = { nodeId: string; ignored?: boolean; role?: { value?: unknown }; name?: { value?: unknown }; value?: { value?: unknown }; childIds?: string[]; parentId?: string; backendDOMNodeId?: number; properties?: { name: string; value?: { value?: unknown } }[] };
    const tree = (await d.sendCommand("Accessibility.getFullAXTree")) as { nodes: Ax[] };
    const byId = new Map(tree.nodes.map((n) => [n.nodeId, n]));
    const depth = new Map<string, number>();
    for (const n of tree.nodes) {
      let dpt = 0;
      let p = n.parentId;
      while (p) {
        dpt += 1;
        p = byId.get(p)?.parentId;
        if (dpt > 64) break;
      }
      depth.set(n.nodeId, dpt);
    }
    const limit = Math.max(1, Math.min(maxNodes || 400, 400));
    const live = tree.nodes.filter((n) => !n.ignored);
    const nodes = live.slice(0, limit).map((n) => ({
      id: n.nodeId,
      parent: n.parentId ?? null,
      role: String(n.role?.value ?? ""),
      name: String(n.name?.value ?? "").slice(0, 200),
      value: String(n.value?.value ?? "").slice(0, 200),
      depth: depth.get(n.nodeId) ?? 0,
      ignored: false,
      backend_dom_node_id: typeof n.backendDOMNodeId === "number" ? n.backendDOMNodeId : null,
      bounds: null as null | { x: number; y: number; width: number; height: number },
      disabled: (n.properties ?? []).some((p) => (p.name === "disabled" || p.name === "readonly") && p.value?.value === true),
    }));
    // The layout box of what can be acted on (bounded: the first 80
    // actionable nodes), in CSS pixels of the view.
    const actionable = new Set(["button", "link", "textbox", "searchbox", "combobox", "checkbox", "radio", "slider", "spinbutton", "listbox", "menuitem", "tab", "option", "switch"]);
    let boxes = 0;
    for (const n of nodes) {
      if (boxes >= 80 || n.backend_dom_node_id === null || !actionable.has(n.role.toLowerCase())) continue;
      boxes += 1;
      try {
        const bm = (await d.sendCommand("DOM.getBoxModel", { backendNodeId: n.backend_dom_node_id })) as { model: { border: number[] } };
        const q = bm.model.border;
        const xs = [q[0]!, q[2]!, q[4]!, q[6]!];
        const ys = [q[1]!, q[3]!, q[5]!, q[7]!];
        const x = Math.min(...xs);
        const y = Math.min(...ys);
        n.bounds = { x: Math.max(0, Math.round(x)), y: Math.max(0, Math.round(y)), width: Math.max(0, Math.round(Math.max(...xs) - x)), height: Math.max(0, Math.round(Math.max(...ys) - y)) };
      } catch {
        // no box (detached or hidden): the entity stays, unlocated
      }
    }
    return { kind: "snapshot", state: this.state(h), nodes, truncated: live.length > limit };
  }

  private async capture(h: HostedSession, clip: { x: number; y: number; width: number; height: number } | null): Promise<unknown> {
    const d = await this.cdp(h.view.webContents);
    const r = (await d.sendCommand("Page.captureScreenshot", { format: "png", ...(clip ? { clip: { ...clip, scale: 1 } } : {}) })) as { data: string };
    return { kind: "capture", state: this.state(h), png_base64: r.data, clip };
  }

  /** The isolation report of a hosted view, for the renderer's Browser panel and the E2E. */
  async probe(browserSessionId: string): Promise<unknown> {
    const h = this.sessions.get(browserSessionId);
    if (!h || h.view.webContents.isDestroyed()) return null;
    return this.isolation(h);
  }

  /** Asked of the page itself: nothing privileged is reachable from the document. */
  private async isolation(h: HostedSession): Promise<unknown> {
    const d = await this.cdp(h.view.webContents);
    const r = (await d.sendCommand("Runtime.evaluate", { expression: "typeof process !== 'undefined' || typeof require !== 'undefined' || typeof window.electron !== 'undefined' || typeof window.modbit !== 'undefined'", returnByValue: true })) as { result: { value?: unknown } };
    return { kind: "isolation", node_reachable: r.result.value === true, partition: h.partition, sandboxed: true, context_isolated: true };
  }
}
