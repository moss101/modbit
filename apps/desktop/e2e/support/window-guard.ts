/**
 * The hardening guard for a window of the running app (REQ-PX-203, QUAL-PX-203; the static half is
 * `src/main/hardening-guard.test.ts`). What a window is, read from the main process and from the page itself: its web
 * preferences, its session, the Content-Security-Policy the page is under (read from the browser's own report of a blocked
 * inline script, so it is the policy in force, not one written down), whether the page can reach Node, whether a navigation
 * and a new window are refused, and which bridge it was given. `guardDrift` compares a window with the app's own window and
 * with the fixed rules; it is empty when the window is as hardened as the app's.
 */
import type { ElectronApplication, Page } from "@playwright/test";

export interface WindowFacts {
  /** The web preferences Electron applied, the security-relevant ones (the preload is proven by the bridge it installs). */
  prefs: Record<string, unknown>;
  defaultSession: boolean;
  /** The policy the page is under, from the violation report of an inline script; null where the script ran or nothing was reported. */
  csp: string | null;
  inlineScriptRan: boolean;
  nodeInPage: string[];
  /** The bridge's method names. */
  bridge: string[];
  navigated: boolean;
  openedWindow: boolean;
}

const PREF_KEYS = ["contextIsolation", "nodeIntegration", "nodeIntegrationInSubFrames", "nodeIntegrationInWorker", "sandbox", "webSecurity", "webviewTag", "allowRunningInsecureContent", "experimentalFeatures", "enableBlinkFeatures", "javascript", "partition", "additionalArguments", "safeDialogs", "disableHtmlFullscreenWindowResize"];

/** Ties the page to its window in the main process by a mark only the page carries. */
async function markOf(page: Page): Promise<string> {
  const mark = `g${Date.now().toString(36)}${Math.random().toString(36).slice(2)}`;
  await page.evaluate((m) => (document.documentElement.dataset.guardMark = m), mark);
  return mark;
}

async function mainFacts(app: ElectronApplication, mark: string): Promise<{ id: number; prefs: Record<string, unknown>; defaultSession: boolean; url: string } | null> {
  return app.evaluate(async ({ BrowserWindow, session }, [m, keys]) => {
    for (const w of BrowserWindow.getAllWindows()) {
      if (w.isDestroyed()) continue;
      const got = await w.webContents.executeJavaScript("document.documentElement.dataset.guardMark || ''").catch(() => "");
      if (got !== m) continue;
      // Electron keeps what it applied; the preload is not among it, and the bridge the page was given stands for it below.
      const all = (w.webContents as unknown as { getLastWebPreferences: () => Record<string, unknown> }).getLastWebPreferences();
      const prefs = Object.fromEntries((keys as string[]).map((k) => [k, all?.[k] ?? null]));
      return { id: w.webContents.id, prefs, defaultSession: w.webContents.session === session.defaultSession, url: w.webContents.getURL() };
    }
    return null;
  }, [mark, PREF_KEYS] as const);
}

/** Reads the facts of the window that shows `page`. It tries a navigation and a new window, both of which a hardened window refuses. */
export async function windowFacts(app: ElectronApplication, page: Page): Promise<WindowFacts> {
  const mark = await markOf(page);
  const main = await mainFacts(app, mark);
  if (!main) throw new Error("no window in the main process shows this page");
  const inPage = await page.evaluate(
    () =>
      new Promise<{ csp: string | null; ran: boolean; node: string[]; bridge: string[] }>((done) => {
        const w = window as unknown as Record<string, unknown>;
        const node = ["require", "process", "module", "Buffer", "global", "__dirname"].filter((k) => typeof w[k] !== "undefined");
        const bridge = Object.keys((w.modbit as object | undefined) ?? {}).sort();
        let finished = false;
        const finish = (csp: string | null) => {
          if (finished) return;
          finished = true;
          document.removeEventListener("securitypolicyviolation", onViolation);
          script.remove();
          done({ csp, ran: w.__guardInlineRan === true, node, bridge });
        };
        const onViolation = (e: SecurityPolicyViolationEvent) => {
          if (e.violatedDirective.startsWith("script-src")) finish(e.originalPolicy);
        };
        document.addEventListener("securitypolicyviolation", onViolation);
        const script = document.createElement("script");
        script.textContent = "window.__guardInlineRan = true;";
        document.head.appendChild(script);
        setTimeout(() => finish(null), 1500);
      }),
  );
  // A navigation: the window's own `will-navigate` handlers are given a navigation to another address, as Electron gives them one
  // the page starts, and must refuse it. (A navigation the page really starts and the window refuses leaves Playwright waiting
  // for that navigation forever, so it is not started from the page; a window that refuses nothing is shown below to fail.)
  const navigated = await app.evaluate(({ webContents }, id) => {
    const wc = webContents.fromId(id);
    if (!wc) return true;
    let prevented = false;
    const event = { preventDefault: () => (prevented = true), url: "https://example.invalid/guard-navigation", isMainFrame: true, isSameDocument: false };
    wc.emit("will-navigate", event, event.url, false, true);
    return !prevented;
  }, main.id);
  // A new window, as the page asks for one.
  const before = await app.evaluate(({ BrowserWindow }) => BrowserWindow.getAllWindows().length);
  const opened = await page.evaluate(() => window.open(location.href.replace(/#.*$/, "") + "?guard-window=1", "_blank") !== null).catch(() => false);
  await page.waitForTimeout(500);
  const after = await app.evaluate(({ BrowserWindow }) => BrowserWindow.getAllWindows().length);
  if (after > before) {
    // A window that should not exist is closed again, so one failing check does not leave the app in another state.
    await app.evaluate(({ BrowserWindow }, n) => BrowserWindow.getAllWindows().slice(0, Math.max(0, BrowserWindow.getAllWindows().length - n)).forEach((w) => w.destroy()), before);
  }
  return { prefs: main.prefs, defaultSession: main.defaultSession, csp: inPage.csp, inlineScriptRan: inPage.ran, nodeInPage: inPage.node, bridge: inPage.bridge, navigated, openedWindow: opened || after > before };
}

/** Why `w` is not as hardened as the app's window `ref` (empty when it is). */
export function guardDrift(ref: WindowFacts, w: WindowFacts): string[] {
  const bad: string[] = [];
  for (const [k, v] of Object.entries(ref.prefs)) if (JSON.stringify(w.prefs[k]) !== JSON.stringify(v)) bad.push(`webPreferences.${k} is ${JSON.stringify(w.prefs[k])}, the app's window has ${JSON.stringify(v)}`);
  for (const [k, want] of [["contextIsolation", true], ["nodeIntegration", false], ["sandbox", true], ["webSecurity", true]] as const) if (w.prefs[k] !== want) bad.push(`webPreferences.${k} is not ${want}`);
  if (w.prefs.nodeIntegrationInSubFrames === true || w.prefs.nodeIntegrationInWorker === true || w.prefs.webviewTag === true) bad.push("Node or webviews reach some frame");
  if (!w.defaultSession) bad.push("the window is not in the default session, where the policy header is set");
  if (w.inlineScriptRan) bad.push("an inline script ran: no Content-Security-Policy stops it");
  if (w.csp === null) bad.push("no Content-Security-Policy reported a blocked inline script");
  else if (w.csp !== ref.csp) bad.push(`the Content-Security-Policy is "${w.csp}", the app's window has "${ref.csp}"`);
  if (w.nodeInPage.length > 0) bad.push(`the page reaches Node: ${w.nodeInPage.join(", ")}`);
  if (JSON.stringify(w.bridge) !== JSON.stringify(ref.bridge)) bad.push("the page was given another bridge than the app's window");
  if (w.navigated) bad.push("a navigation was not refused");
  if (w.openedWindow) bad.push("a new window was not refused");
  return bad;
}

/** The rules alone, for the app's own window (the reference must hold them too). */
export function referenceDrift(ref: WindowFacts): string[] {
  return guardDrift(ref, ref).concat(ref.csp && /default-src 'self'/.test(ref.csp) && /connect-src 'none'/.test(ref.csp) ? [] : [`the app's own policy is not the pinned one: ${ref.csp}`]);
}
