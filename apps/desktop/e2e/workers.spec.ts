/**
 * The worker fabric of the desktop (REQ-EV-0075 / 0076 / 0081; docs/32
 * "Security settings", docs/22 "Local browser"): the privileged effects —
 * the Core socket and its boot secret, the shell, the filesystem, credential
 * values, input into the browser views — live in Electron main and the
 * Core, behind typed requests the app's own top frame alone may make
 * (QUAL-EV-0075); the renderer is a view of the Core's session, not its
 * owner — killed and brought back, the session's truth is unchanged
 * (QUAL-EV-0076); a browser view whose process dies is restarted and
 * attached again to the same session while the Core and the task are
 * untouched (QUAL-EV-0081). Every test drives the real app against the
 * real Core.
 */
import { _electron as electron, expect, test, type ElectronApplication, type Page } from "@playwright/test";
import { execFileSync } from "node:child_process";
import { mkdtempSync, writeFileSync } from "node:fs";
import { createServer, type Server } from "node:http";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const appDir = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const coreBin = process.env.MODBIT_CORE_BIN ?? resolve(appDir, "..", "..", "target", "debug", process.platform === "win32" ? "modbit-core.exe" : "modbit-core");

type Calls = { name: string; args: unknown }[];
type Snapshot = { sessionId: string; generation: number; lastOffset: string; tasks: { taskId: string; state: string }[] };
/** A step is fixed, or computed (and possibly awaited) when the model is asked. */
type Script = ({ calls?: Calls } | ((toolTexts: string[]) => { calls?: Calls } | Promise<{ calls?: Calls }>))[];

function scriptedModel(script: Script): Promise<{ server: Server; url: string; toolTexts: string[]; requests: number[] }> {
  const toolTexts: string[] = [];
  const requests: number[] = [];
  return new Promise((resolveServer) => {
    const server = createServer((req, res) => {
      let body = "";
      req.on("data", (c) => (body += c));
      req.on("end", async () => {
        const parsed = JSON.parse(body || "{}") as { messages?: { role: string; content?: string }[] };
        const tools = (parsed.messages ?? []).filter((m) => m.role === "tool");
        const results = tools.length;
        requests.push(results);
        const last = tools.at(-1)?.content;
        if (typeof last === "string" && toolTexts.length < results) toolTexts.push(last);
        const step = script[results] ?? {};
        const reply = typeof step === "function" ? await step(toolTexts) : step;
        res.writeHead(200, { "content-type": "text/event-stream" });
        const send = (o: unknown) => res.write(`data: ${JSON.stringify(o)}\n\n`);
        (reply.calls ?? []).forEach((c, i) => send({ id: "c", model: "scripted", choices: [{ index: 0, delta: { tool_calls: [{ index: i, id: `call_${results}_${i}`, type: "function", function: { name: c.name, arguments: JSON.stringify(c.args) } }] }, finish_reason: null }] }));
        if (!reply.calls?.length) send({ id: "c", model: "scripted", choices: [{ index: 0, delta: { content: "Nothing further." }, finish_reason: null }] });
        send({ id: "c", model: "scripted", choices: [{ index: 0, delta: {}, finish_reason: reply.calls?.length ? "tool_calls" : "stop" }], usage: { prompt_tokens: 10, completion_tokens: 5 } });
        res.write("data: [DONE]\n\n");
        res.end();
      });
    });
    server.listen(0, "127.0.0.1", () => resolveServer({ server, url: `http://127.0.0.1:${(server.address() as { port: number }).port}`, toolTexts, requests }));
  });
}

/** A real page served over HTTP, with a heading the snapshot names. */
function fixtureSite(): Promise<{ server: Server; url: string; hits: string[] }> {
  const hits: string[] = [];
  return new Promise((resolveServer) => {
    const server = createServer((req, res) => {
      hits.push(req.url ?? "");
      res.writeHead(200, { "content-type": "text/html; charset=utf-8" });
      res.end(`<!doctype html><html><head><title>Greeter</title></head><body><main><h1>Greeter</h1><p>Hello from the fixture.</p><button type="button">Say hello</button></main></body></html>`);
    });
    server.listen(0, "127.0.0.1", () => resolveServer({ server, url: `http://127.0.0.1:${(server.address() as { port: number }).port}`, hits }));
  });
}

function git(cwd: string, ...args: string[]): string {
  return execFileSync("git", ["-C", cwd, ...args], { encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] }).trim();
}

function smallRepo(): string {
  const repo = mkdtempSync(join(tmpdir(), "modbit-workers-repo-"));
  writeFileSync(join(repo, "notes.txt"), "line 1\n");
  git(repo, "init", "-q", "-b", "main");
  git(repo, "add", "-A");
  git(repo, "-c", "user.name=t", "-c", "user.email=t@e", "commit", "-q", "-m", "base");
  return repo;
}

/** The task reaches review — or the failure names what the card and the model saw. */
async function readyForReview(page: Page, model: { toolTexts: string[] }): Promise<void> {
  const ready = page.getByTestId("column-readyForReview").getByTestId("task-card");
  const deadline = Date.now() + 90_000;
  while ((await ready.count()) === 0) {
    if (Date.now() > deadline) {
      const card = page.getByTestId("task-card").first();
      const state = await card.getAttribute("data-state").catch(() => null);
      const line = await card.getByTestId("task-screen-state").textContent().catch(() => null);
      const tail = model.toolTexts.map((t, i) => `[${i}] ${t.slice(0, 300).replace(/\n/g, " | ")}`).join("\n");
      throw new Error(`the task never reached review: state=${state} line=${line}\ntool texts:\n${tail}`);
    }
    await new Promise((r) => setTimeout(r, 500));
  }
}

async function closeApp(app: ElectronApplication): Promise<void> {
  const proc = app.process();
  await Promise.race([app.close(), new Promise<void>((r) => setTimeout(r, 15_000))]);
  if (proc.exitCode === null) proc.kill("SIGKILL");
}

async function launch(dataDir: string, extraEnv: Record<string, string>): Promise<{ app: ElectronApplication; page: Page }> {
  const app = await electron.launch({ args: [join(appDir, "dist", "main", "main.cjs")], env: { ...process.env, MODBIT_DATA_DIR: dataDir, MODBIT_CORE_BIN: coreBin, OPENAI_API_KEY: "", ANTHROPIC_API_KEY: "", MODBIT_SUPPRESS_OS_NOTIFICATIONS: "1", ...extraEnv } });
  const page = await app.firstWindow();
  app.process().stderr?.on("data", (d: Buffer) => process.stderr.write(`[electron] ${d}`));
  await expect(page.getByTestId("core-status")).toContainText("Core connected", { timeout: 60_000 });
  return { app, page };
}

/** The bridge as the renderer sees it: typed requests by name, nothing generic. */
const BRIDGE_ALLOW_LIST = [
  "addCredential", "applyUserPatch", "attachFile", "attention", "browserLog", "browserSession", "cancelTask", "closeBrowser", "codeView", "contextInspector", "coreStatus", "createSession", "createTask", "dashboard", "debugCoreInfo", "debugIpcRefusals", "debugRendererLog", "decideReview", "deliverNotification", "describeBrowser", "emergencyStop", "hideBrowser", "languages", "listCredentials", "localState", "notificationLog", "onBrowserState", "onCoreStatus", "onEvent", "onRecovery", "openBrowser", "openPullRequest", "probeBrowser", "providerStatus", "removeCredential", "resolveApproval", "respondToQuestion", "reviewBundle", "sessionSnapshot", "setBrowserControl", "setTaskSelection", "setupProvider", "showBrowser", "starterTasks", "startTask", "steerTask", "subscribe", "taskEconomics", "taskStatus", "trustRepository", "typeAsPerson",
];

test("renderer compromise (REQ-EV-0075): the app's renderer holds no secret, no Node, no socket; the bridge is typed requests only; the Core refuses what the renderer cannot vouch for; a foreign window with the same bridge is refused by main and audited", async () => {
  test.setTimeout(120_000);
  const model = await scriptedModel([]);
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-workers-"));
  try {
    const { app, page } = await launch(dataDir, { MODBIT_OPENAI_BASE_URL: model.url });
    // 1. Arbitrary code in the renderer (a compromise) finds no Node, no
    //    Electron internals, no raw IPC: the bridge alone, and it names
    //    typed requests — nothing that takes a channel, a command or a path
    //    to run.
    const surface = await page.evaluate(() => ({
      require: typeof (window as unknown as { require?: unknown }).require,
      process: typeof (window as unknown as { process?: unknown }).process,
      electron: typeof (window as unknown as { electron?: unknown }).electron,
      ipcRenderer: typeof (window as unknown as { ipcRenderer?: unknown }).ipcRenderer,
      bridge: Object.keys(window.modbit).sort(),
      bridgeIsFunctions: Object.values(window.modbit).every((v) => typeof v === "function"),
    }));
    expect(surface).toMatchObject({ require: "undefined", process: "undefined", electron: "undefined", ipcRenderer: "undefined", bridgeIsFunctions: true });
    expect(surface.bridge).toEqual([...BRIDGE_ALLOW_LIST].sort());
    // 2. The Core's endpoint is known to main only as an address; the boot
    //    secret never crosses the bridge, and the renderer cannot reach the
    //    socket at all (the page's CSP allows no connection).
    const info = await page.evaluate(() => window.modbit.debugCoreInfo());
    expect(info).not.toBeNull();
    expect(Object.keys(info!).sort()).toEqual(["endpoint", "pid"]);
    expect(JSON.stringify(info)).not.toMatch(/secret/i);
    const reach = await page.evaluate(async (endpoint) => {
      const out: Record<string, string> = {};
      const target = /^\d+\.\d+\.\d+\.\d+:\d+$/.test(endpoint) ? endpoint : "127.0.0.1:1";
      try {
        await fetch(`http://${target}/`);
        out.fetch = "reached";
      } catch (e) {
        out.fetch = `refused: ${(e as Error).name}`;
      }
      try {
        const ws = new WebSocket(`ws://${target}/`);
        out.websocket = await new Promise<string>((r) => {
          ws.onerror = () => r("refused: error");
          ws.onopen = () => r("reached");
        });
      } catch (e) {
        out.websocket = `refused: ${(e as Error).name}`;
      }
      return out;
    }, info!.endpoint);
    expect(reach.fetch).toMatch(/^refused/);
    expect(reach.websocket).toMatch(/^refused/);
    // 3. What the renderer may ask for is the person's own decisions, and
    //    the Core vouches for their objects: an approval the Core never
    //    issued cannot be resolved, a browser session it never opened
    //    cannot be reached, a credential's value is never listed.
    const sessionId = await page.evaluate(() => window.modbit.createSession());
    const forged = "0123456789abcdef0123456789abcdef";
    const approval = await page.evaluate(([sid, aid]) => window.modbit.resolveApproval(sid!, aid!, true, "compromised", "sha256:0").then(() => "resolved", (e: Error) => e.message), [sessionId, forged]);
    expect(approval).toMatch(/UNKNOWN_APPROVAL|NO_SUCH_APPROVAL|NOT_FOUND|APPROVAL/);
    expect(approval).not.toBe("resolved");
    const session = await page.evaluate(([bsid, tid]) => window.modbit.browserSession(bsid!, tid!).then(() => "reached", (e: Error) => e.message), [forged, forged]);
    expect(session).toMatch(/NO_SUCH_SESSION|UNKNOWN|NOT_FOUND/);
    await page.evaluate(() => window.modbit.addCredential("fixture", "http://127.0.0.1:1", "person", "hunter2-never-listed"));
    const listed = await page.evaluate(() => window.modbit.listCredentials());
    expect(listed).toHaveLength(1);
    expect(JSON.stringify(listed)).not.toContain("hunter2");
    expect(Object.keys(listed[0]!).sort()).toEqual(["handle", "label", "origin", "persisted", "username"]);
    // 4. A frame inside the app's page has no bridge (the preload runs in
    //    the top frame only).
    const inFrame = await page.evaluate(async () => {
      const f = document.createElement("iframe");
      f.srcdoc = "<p>frame</p>";
      document.body.appendChild(f);
      await new Promise((r) => (f.onload = r));
      const w = f.contentWindow as unknown as { modbit?: unknown; require?: unknown } | null;
      const out = { modbit: typeof w?.modbit, require: typeof w?.require };
      f.remove();
      return out;
    });
    expect(inFrame).toEqual({ modbit: "undefined", require: "undefined" });
    // 5. Another window in this app with the very same preload — what an
    //    attacker who could open a window would hold — is refused by main
    //    before its request is read: only the app's own top frame may ask.
    const preload = join(appDir, "dist", "preload", "preload.cjs");
    const foreign = await app.evaluate(async ({ BrowserWindow }, preloadPath) => {
      const w = new BrowserWindow({ show: false, webPreferences: { preload: preloadPath, contextIsolation: true, sandbox: true, nodeIntegration: false } });
      await w.loadURL("about:blank");
      const asked = (await w.webContents.executeJavaScript("Promise.all([window.modbit.createSession(), window.modbit.debugCoreInfo(), window.modbit.listCredentials()]).then(() => 'answered', (e) => String(e && e.message || e))")) as string;
      const hasBridge = (await w.webContents.executeJavaScript("typeof window.modbit")) as string;
      w.destroy();
      return { asked, hasBridge };
    }, preload);
    expect(foreign.hasBridge).toBe("object");
    expect(foreign.asked).toContain("SENDER_REFUSED");
    const refusals = await page.evaluate(() => window.modbit.debugIpcRefusals());
    expect(refusals.map((r) => r.channel)).toContain("session:create");
    expect(refusals.every((r) => r.reason === "not the app window")).toBe(true);
    // The app's own renderer, meanwhile, is answered as before.
    const snap = (await page.evaluate((sid) => window.modbit.sessionSnapshot(sid), sessionId)) as Snapshot;
    expect(snap.sessionId).toBe(sessionId);
    await closeApp(app);
  } finally {
    model.server.close();
  }
});

test("renderer killed (REQ-EV-0076): the renderer's process dies mid-session; main keeps the Core, the lease and the stream; the renderer comes back from the Core's snapshot and cursor with the session's truth unchanged and its typed requests answered", async () => {
  test.setTimeout(180_000);
  const repo = smallRepo();
  const model = await scriptedModel([
    { calls: [{ name: "plan.update", args: { outcome: "notes are read", expected_files: [] } }] },
    { calls: [{ name: "fs.read", args: { path: "notes.txt" } }] },
    { calls: [{ name: "task.complete", args: { summary: "read the notes", self_review: { findings: [] } } }] },
  ]);
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-workers-"));
  try {
    const { app, page: page1 } = await launch(dataDir, { MODBIT_OPENAI_BASE_URL: model.url });
    await page1.getByTestId("workspace").fill(repo);
    await page1.getByTestId("goal").fill("read the notes");
    await page1.getByTestId("run").click();
    const card1 = page1.getByTestId("task-card").first();
    await expect(card1).toContainText("read the notes", { timeout: 30_000 });
    await card1.getByTestId("task-start").click();
    await readyForReview(page1, model);
    const taskId = (await card1.getAttribute("data-task-id"))!;
    const sessionId = (await page1.evaluate(() => window.modbit.localState())).sessionId!;
    const before = (await page1.evaluate((sid) => window.modbit.sessionSnapshot(sid), sessionId)) as Snapshot;
    const coreBefore = await page1.evaluate(() => window.modbit.debugCoreInfo());
    expect(before.tasks.map((t) => [t.taskId, t.state])).toEqual([[taskId, "ReadyForReview"]]);
    // The renderer's process is killed under it (what a crash or the OS
    // does). Main notes it and opens a fresh window in the old one's place
    // (the crashed page is finished for the harness too).
    const crashed = page1.waitForEvent("crash", { timeout: 30_000 });
    const replaced = app.waitForEvent("window", { timeout: 60_000 });
    await app.evaluate(({ BrowserWindow }) => BrowserWindow.getAllWindows()[0]!.webContents.forcefullyCrashRenderer());
    await crashed;
    const page2 = await replaced;
    await expect(page2.getByTestId("core-status")).toContainText("Core connected", { timeout: 60_000 });
    expect(await app.evaluate(({ BrowserWindow }) => BrowserWindow.getAllWindows().length)).toBe(1);
    const page = page2;
    const rendererLog = await page.evaluate(() => window.modbit.debugRendererLog());
    expect(rendererLog).toHaveLength(1);
    expect(rendererLog[0]!.reason).toMatch(/crashed|killed|abnormal-exit/);
    expect(rendererLog[0]!.reloaded).toBe(true);
    // The same Core process, the same session, the same task in the same
    // state at the same offset: nothing of the session lived in the renderer.
    const coreAfter = await page.evaluate(() => window.modbit.debugCoreInfo());
    expect(coreAfter?.pid).toBe(coreBefore?.pid);
    const after = (await page.evaluate((sid) => window.modbit.sessionSnapshot(sid), sessionId)) as Snapshot;
    expect(after.lastOffset).toBe(before.lastOffset);
    expect(after.generation).toBe(before.generation);
    expect(after.tasks.map((t) => [t.taskId, t.state])).toEqual(before.tasks.map((t) => [t.taskId, t.state]));
    // The new renderer rebuilt its view from that snapshot: the card is
    // where it was, and its typed requests are answered by the same Core.
    const restored = page.getByTestId("column-readyForReview").getByTestId("task-card");
    await expect(restored).toHaveCount(1, { timeout: 30_000 });
    await expect(restored).toHaveAttribute("data-task-id", taskId);
    const status = await page.evaluate((tid) => window.modbit.taskStatus(tid), taskId);
    expect(status.state).toBe("ReadyForReview");
    expect(status.lastOffset).toBe(before.lastOffset);
    const bundle = await page.evaluate((tid) => window.modbit.reviewBundle(tid), taskId);
    expect(bundle.taskId).toBe(taskId);
    // No model request was made by the restart: the renderer drives no run.
    expect(model.requests).toEqual([0, 1, 2]);
    await closeApp(app);
  } finally {
    model.server.close();
  }
});

test("browser view killed (REQ-EV-0081): the view's process dies under the agent's session; the host restarts the view at its page and attaches it again to the same session; the Core, its log and the task are untouched, and the agent's next observation reads the restarted page", async () => {
  test.setTimeout(180_000);
  const repo = smallRepo();
  const site = await fixtureSite();
  let releaseSecondLook: () => void = () => {};
  const secondLook = new Promise<void>((r) => (releaseSecondLook = r));
  const model = await scriptedModel([
    { calls: [{ name: "plan.update", args: { outcome: "the greeter page is read twice", expected_files: [] } }] },
    { calls: [{ name: "browser.navigate", args: { url: `${site.url}/greet` } }] },
    { calls: [{ name: "browser.snapshot", args: { max_nodes: 50 } }] },
    // The model's fourth request is held until the test has killed the view.
    async () => {
      await secondLook;
      return { calls: [{ name: "browser.snapshot", args: { max_nodes: 50 } }] };
    },
    { calls: [{ name: "task.complete", args: { summary: "read the greeter twice", self_review: { findings: [] } } }] },
  ]);
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-workers-"));
  try {
    const { app, page } = await launch(dataDir, { MODBIT_OPENAI_BASE_URL: model.url });
    await page.getByTestId("workspace").fill(repo);
    await page.getByTestId("goal").fill("read the greeter twice");
    await page.getByTestId("run").click();
    const card = page.getByTestId("task-card").first();
    await expect(card).toContainText("read the greeter twice", { timeout: 30_000 });
    const taskId = (await card.getAttribute("data-task-id"))!;
    await card.getByTestId("task-browser").click();
    const panel = page.getByTestId("browser");
    await expect(panel).toHaveAttribute("data-browser-session-id", /^[0-9a-f]{32}$/, { timeout: 30_000 });
    const bsid = (await panel.getAttribute("data-browser-session-id"))!;
    await page.getByTestId("browser-close").click();
    await card.getByTestId("task-start").click();
    // The first look at the page has been taken.
    await expect.poll(() => model.toolTexts.length, { timeout: 60_000 }).toBeGreaterThanOrEqual(3);
    expect(model.toolTexts[2]).toContain('"name":"Greeter"');
    const identity = (await page.evaluate((id) => window.modbit.describeBrowser(id), bsid)) as unknown as { webContentsId: number; osProcessId: number; leaseGeneration: number; attached: boolean } | null;
    expect(identity?.attached).toBe(true);
    const coreBefore = await page.evaluate(() => window.modbit.debugCoreInfo());
    const sessionId = (await page.evaluate(() => window.modbit.localState())).sessionId!;
    // The view's process is killed under the session.
    await app.evaluate(({ webContents }, id) => webContents.fromId(id)!.forcefullyCrashRenderer(), identity!.webContentsId);
    // The host noted the loss and restarted the view — the same web
    // contents, a new OS process, the same session attached again.
    await expect.poll(async () => (await page.evaluate(() => window.modbit.browserLog())).filter((l) => l.browserSessionId === bsid && (l.kind === "view-gone" || l.kind === "view-restarted")).map((l) => `${l.kind}:${l.ok}`), { timeout: 30_000 }).toEqual(["view-gone:false", "view-restarted:true"]);
    const restarted = (await page.evaluate((id) => window.modbit.describeBrowser(id), bsid)) as unknown as { webContentsId: number; osProcessId: number; leaseGeneration: number; attached: boolean; url: string } | null;
    expect(restarted?.attached).toBe(true);
    expect(restarted?.webContentsId).toBe(identity?.webContentsId);
    expect(restarted?.osProcessId).toBeGreaterThan(0);
    expect(restarted?.osProcessId).not.toBe(identity?.osProcessId);
    expect(restarted?.url).toBe(`${site.url}/greet`);
    const core = await page.evaluate(([id, tid]) => window.modbit.browserSession(id!, tid!), [bsid, taskId]);
    expect(core.closed).toBe(false);
    expect(core.hostAttached).toBe(true);
    expect(core.hostKind).toBe("electron-main");
    // The agent's next observation reads the restarted page; the task
    // completes on the same Core process, in the same session.
    releaseSecondLook();
    await readyForReview(page, model);
    // The second look is a delta against the first (M7.2): the restarted
    // page is the same page — its entities unchanged, its state version
    // moved on by the restart's loads.
    const second = model.toolTexts[3]!;
    expect(second).toContain("status: SUCCESS");
    expect(second).toContain('"title":"Greeter"');
    expect(second).toContain('"mode":"delta"');
    expect(second).toContain('"unchanged":true');
    expect(second).toContain('"entity_count":3');
    const versions = /"from_version":(\d+).*"state_version":(\d+)/.exec(second)!;
    expect(Number(versions[2])).toBeGreaterThan(Number(versions[1]));
    const coreAfter = await page.evaluate(() => window.modbit.debugCoreInfo());
    expect(coreAfter?.pid).toBe(coreBefore?.pid);
    const snap = (await page.evaluate((sid) => window.modbit.sessionSnapshot(sid), sessionId)) as Snapshot;
    expect(snap.tasks.map((t: { taskId: string; state: string }) => [t.taskId, t.state])).toEqual([[taskId, "ReadyForReview"]]);
    // The page was fetched twice: the agent's navigation, and the restart at the same page.
    expect(site.hits).toEqual(["/greet", "/greet"]);
    await closeApp(app);
  } finally {
    model.server.close();
    site.server.close();
  }
});
