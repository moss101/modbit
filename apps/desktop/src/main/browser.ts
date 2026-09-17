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
import { admitsAgentInput } from "./lease.js";

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
  | { kind: "act"; backend_dom_node_id: number; action: "click" | "fill" | "select" | "check" | "uncheck" | "press" | "fill_credential"; value?: string; key?: string; at?: [number, number] | null; credential_handle?: string }
  | { kind: "isolation" }
  | { kind: "close" };

const HOST_KIND = "electron-main";

export class BrowserHost {
  private readonly sessions = new Map<string, HostedSession>();
  /** Delivery log of every request answered, for the renderer's Browser panel and the E2E. */
  readonly log: { browserSessionId: string; kind: string; ok: boolean; code: string; generation: number; atMs: number }[] = [];

  constructor(
    private readonly window: () => BrowserWindow | null,
    private readonly client: () => CoreClient | null,
    private readonly notify: (channel: string, payload: unknown) => void,
    /** M7.8: the broker's secret for a handle, only for a page at the bound origin; null otherwise. */
    private readonly secretFor: (handle: string, pageOrigin: string) => string | null = () => null,
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
    const hosted: HostedSession = { browserSessionId: opened.browserSessionId, taskId, sessionId, partition: opened.partition, view, stateVersion: 0, attached: false, shown: false, leaseGeneration: Number(opened.leaseGeneration), controller: opened.controller === "USER" ? "USER" : "AGENT", lastBounds: { x: 0, y: 0, width: 1024, height: 768 } };
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
    const a = await c.attachBrowserHost(h.browserSessionId, { hostKind: HOST_KIND, partition: h.partition, sandboxed: true, contextIsolated: true, nodeIntegration: false, taskId: h.taskId });
    h.leaseGeneration = Math.max(h.leaseGeneration, Number(a.leaseGeneration));
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
    h.lastBounds = { x: Math.round(bounds.x), y: Math.round(bounds.y), width: Math.max(0, Math.round(bounds.width)), height: Math.max(0, Math.round(bounds.height)) };
    h.view.setBounds(h.lastBounds);
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
  describe(browserSessionId: string): { browserSessionId: string; taskId: string; partition: string; attached: boolean; shown: boolean; url: string; title: string; stateVersion: number; leaseGeneration: number; controller: "AGENT" | "USER" } | null {
    const h = this.sessions.get(browserSessionId);
    if (!h || h.view.webContents.isDestroyed()) return null;
    const s = this.state(h);
    return { browserSessionId: h.browserSessionId, taskId: h.taskId, partition: h.partition, attached: h.attached, shown: h.shown, url: s.url, title: s.title, stateVersion: h.stateVersion, leaseGeneration: h.leaseGeneration, controller: h.controller };
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
    if (!h || h.view.webContents.isDestroyed()) return false;
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
      // Agent input under the control lease (M7.6): the generation the Core
      // stamped must be the one this host holds, and the agent must hold control.
      if (req.kind === "navigate" || req.kind === "act") {
        const verdict = admitsAgentInput(Number(r.leaseGeneration), h.leaseGeneration, h.controller);
        if (!verdict.ok) return { kind: "error", code: verdict.code, message: verdict.code === "USER_HAS_CONTROL" ? `the person holds control (lease generation ${h.leaseGeneration}); agent input is blocked, observation is not` : `input stamped with lease generation ${r.leaseGeneration} is fenced: the session is at ${h.leaseGeneration}` };
      }
      switch (req.kind) {
        case "navigate":
          return this.navigate(h, req.url);
        case "state":
          return { kind: "state", state: this.state(h) };
        case "snapshot":
          return this.snapshot(h, req.max_nodes);
        case "capture":
          return this.capture(h, req.clip);
        case "act":
          return this.act(h, req.backend_dom_node_id, req.action, req.value ?? "", req.key ?? "", req.at ?? null, req.credential_handle ?? null);
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
    this.log.push({ browserSessionId: bsid, kind: req.kind, ok: rr.kind !== "error", code: rr.code ?? "", generation: Number(r.leaseGeneration), atMs: Date.now() });
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
    const actionable = new Set(["button", "link", "textbox", "searchbox", "combobox", "checkbox", "radio", "slider", "spinbutton", "listbox", "menuitem", "tab", "option", "switch", "canvas", "image", "img", "figure", "graphics-document"]);
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
    // A view hidden with the panel is off the window, and off the window
    // Chromium composites no frame on Linux and Windows: neither CDP's
    // `Page.captureScreenshot` nor `capturePage` ever answers (hosted CI).
    // For the capture the view is parked back in the window — first beside
    // it (out of sight, same layout), and only if that yields no frame
    // either, in place for the instant the capture takes — then hidden
    // again. The image is normalized to CSS pixels (the clip's size) so a
    // HiDPI display does not double it; a capture that takes too long is an
    // error the Core hears, never a request left unanswered.
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
    if (rect) {
      const size = image.getSize();
      if (size.width !== rect.width || size.height !== rect.height) image = image.resize({ width: rect.width, height: rect.height });
    }
    return { kind: "capture", state: this.state(h), png_base64: image.toPNG().toString("base64"), clip };
  }

  /**
   * M7.4: act on the DOM node the compiler resolved, as a person would — a
   * real click at the element's box, text insertion after a focus, real key
   * events — then wait for the page to settle (a navigation it started, or
   * a short quiet period) and report the state. The node id is the one the
   * Core resolved at the current version; a node that is gone is an error,
   * never a click elsewhere.
   */
  private async act(h: HostedSession, backendNodeId: number, action: string, value: string, key: string, at: [number, number] | null, credentialHandle: string | null): Promise<unknown> {
    const wc = h.view.webContents;
    const d = await this.cdp(wc);
    await d.sendCommand("DOM.enable");
    let objectId: string;
    try {
      const r = (await d.sendCommand("DOM.resolveNode", { backendNodeId })) as { object: { objectId: string } };
      objectId = r.object.objectId;
    } catch (e) {
      return { kind: "error", code: "TARGET_GONE", message: `the element is no longer in the document (${(e as Error).message.slice(0, 120)})` };
    }
    const versionBefore = h.stateVersion;
    const call = async (fn: string, args: unknown[] = []) => (await d.sendCommand("Runtime.callFunctionOn", { objectId, functionDeclaration: fn, arguments: args.map((a) => ({ value: a })), returnByValue: true })) as { result: { value?: unknown }; exceptionDetails?: { text?: string } };
    // A navigation the action starts is watched from before the action is
    // dispatched: a local page can start and finish its load between the
    // click and a listener attached afterwards, and a settle that then
    // waits for a load already done outlives the Core's deadline (hosted
    // Windows: the approved sign-in never answered, the outcome unknown).
    const sleep = (ms: number) => new Promise<void>((r) => setTimeout(r, ms));
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
        await sleep(150);
      }
      return navStarted || h.stateVersion !== versionBefore;
    };
    try {
      return await this.perform(h, d, call, action, value, key, at, credentialHandle, backendNodeId, settle);
    } finally {
      wc.removeListener("did-start-navigation", onStart);
      wc.removeListener("did-finish-load", onDone);
      wc.removeListener("did-fail-load", onDone);
    }
  }

  private async perform(h: HostedSession, d: Electron.Debugger, call: (fn: string, args?: unknown[]) => Promise<{ result: { value?: unknown }; exceptionDetails?: { text?: string } }>, action: string, value: string, key: string, at: [number, number] | null, credentialHandle: string | null, backendNodeId: number, settle: () => Promise<boolean>): Promise<unknown> {
    const wc = h.view.webContents;
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
        await d.sendCommand("DOM.scrollIntoViewIfNeeded", { backendNodeId }).catch(() => {});
        const bm = (await d.sendCommand("DOM.getBoxModel", { backendNodeId })) as { model: { border: number[] } };
        const q = bm.model.border;
        // The centre of the box, or the point the model chose inside it
        // (a visual region, M7.5) — clamped to the box, never outside it.
        const left = Math.min(q[0]!, q[2]!, q[4]!, q[6]!);
        const top = Math.min(q[1]!, q[3]!, q[5]!, q[7]!);
        const right = Math.max(q[0]!, q[2]!, q[4]!, q[6]!);
        const bottom = Math.max(q[1]!, q[3]!, q[5]!, q[7]!);
        const x = at ? Math.min(right - 1, Math.max(left, left + at[0])) : (left + right) / 2;
        const y = at ? Math.min(bottom - 1, Math.max(top, top + at[1])) : (top + bottom) / 2;
        await d.sendCommand("Input.dispatchMouseEvent", { type: "mouseMoved", x, y });
        await d.sendCommand("Input.dispatchMouseEvent", { type: "mousePressed", x, y, button: "left", clickCount: 1 });
        await d.sendCommand("Input.dispatchMouseEvent", { type: "mouseReleased", x, y, button: "left", clickCount: 1 });
        detail = `${action} at ${Math.round(x)},${Math.round(y)}`;
        break;
      }
      case "fill": {
        await d.sendCommand("DOM.focus", { backendNodeId });
        await call("function() { if ('value' in this) { this.value = ''; this.dispatchEvent(new Event('input', {bubbles:true})); } }");
        if (value.length > 0) await d.sendCommand("Input.insertText", { text: value });
        await call("function() { this.dispatchEvent(new Event('change', {bubbles:true})); }");
        detail = `inserted ${value.length} chars`;
        break;
      }
      case "fill_credential": {
        // M7.8 (docs/22 "Credentials"): the value comes from the broker's
        // custody for this page's origin only, goes into the field through
        // the same path a typed value takes, and is never returned or logged.
        const pageOrigin = (() => {
          try {
            const u = new URL(wc.getURL());
            return `${u.protocol}//${u.host}`.toLowerCase();
          } catch {
            return "";
          }
        })();
        const secret = credentialHandle ? this.secretFor(credentialHandle, pageOrigin) : null;
        if (secret === null) return { kind: "error", code: "CREDENTIAL_UNAVAILABLE", message: `no credential ${credentialHandle ?? ""} for ${pageOrigin || "this page"}` };
        const tag = await call("function() { return (this.tagName === 'INPUT' || this.tagName === 'TEXTAREA') ? 'ok' : 'NOT_INPUT'; }");
        if (tag.result.value !== "ok") return { kind: "error", code: "CREDENTIAL_TARGET_NOT_FIELD", message: "a credential is filled into an input" };
        await d.sendCommand("DOM.focus", { backendNodeId });
        await call("function() { this.value = ''; this.dispatchEvent(new Event('input', {bubbles:true})); }");
        await d.sendCommand("Input.insertText", { text: secret });
        await call("function() { this.dispatchEvent(new Event('change', {bubbles:true})); }");
        detail = `filled credential ${credentialHandle}`;
        break;
      }
      case "select": {
        const r = await call("function(v) { if (this.tagName !== 'SELECT') return 'NOT_SELECT'; const o = [...this.options].find(o => o.value === v || o.textContent.trim() === v); if (!o) return 'NO_OPTION'; this.value = o.value; this.dispatchEvent(new Event('input', {bubbles:true})); this.dispatchEvent(new Event('change', {bubbles:true})); return 'ok'; }", [value]);
        if (r.result.value !== "ok") return { kind: "error", code: String(r.result.value ?? "SELECT_FAILED"), message: `select ${JSON.stringify(value)}` };
        detail = `selected ${JSON.stringify(value)}`;
        break;
      }
      case "press": {
        await d.sendCommand("DOM.focus", { backendNodeId }).catch(() => {});
        const codes: Record<string, number> = { Enter: 13, Tab: 9, Escape: 27, Backspace: 8, ArrowDown: 40, ArrowUp: 38, ArrowLeft: 37, ArrowRight: 39, Space: 32 };
        const code = codes[key];
        if (code === undefined) return { kind: "error", code: "UNSUPPORTED_KEY", message: key };
        const text = key === "Enter" ? "\r" : key === "Space" ? " " : undefined;
        await d.sendCommand("Input.dispatchKeyEvent", { type: text ? "keyDown" : "rawKeyDown", key, code: key, windowsVirtualKeyCode: code, nativeVirtualKeyCode: code, ...(text ? { text } : {}) });
        await d.sendCommand("Input.dispatchKeyEvent", { type: "keyUp", key, code: key, windowsVirtualKeyCode: code, nativeVirtualKeyCode: code });
        detail = `pressed ${key}`;
        break;
      }
      default:
        return { kind: "error", code: "UNSUPPORTED_ACTION", message: action };
    }
    const navigated = await settle();
    return { kind: "acted", state: this.state(h), navigated, detail };
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
