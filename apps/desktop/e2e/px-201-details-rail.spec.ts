/**
 * QUAL-PX-201 / PX-E2E-201 (DR-PX-2026-11-11-021 MWS-09): the details rail on the real Electron build, a real Core and a
 * scripted model. One task changes a file, starts a background terminal, delegates a child task and then asks for a
 * protected effect; it has checkpoints and a budget. Every value the rail draws names the preload read it comes from, and
 * the spec finds that read, with the task's or the session's id, in a log of the bridge kept in the main process (a tap on
 * the handlers, added by the test; the product is not changed), and compares what is on the screen with what the Core
 * answered. Stop on the terminal kills it and Stop on the child asks and cancels it; Review opens the changes; a restore
 * from a checkpoint previews and asks as it does from a turn; after a reload the phase and the agent counts equal the Core's;
 * at a task area of 800 px the rail is absent and More holds its facts; at the centre's minimum width nothing covers the
 * composer; the details popover is gone. With MODBIT_SHOTS_DIR set, the same run writes the evidence screenshots.
 */
import { expect, test, type ElectronApplication, type Page } from "@playwright/test";
import { mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { Gate } from "./support/stream-model.ts";
import { ECHO_LOOP, nodeTerminal } from "./support/apps-harness.ts";
import { routedModel, sessionOf, sha256File, type ModelPart } from "./support/conversation-harness.ts";
import { agentRow, openTask, openTaskMore, runTask, setDensity } from "./support/new-task.ts";
import { accessible, box, closeApp, git, launch, liftMinimumSize, makeRepo, setContentSize } from "./support/ui-harness.ts";

const SHOTS = process.env.MODBIT_SHOTS_DIR ?? "";
const GOAL = "Tidy the notes and delegate module a";
const CHILD_GOAL = "create src/a/a.txt containing alpha";
const URL1 = "https://example.invalid/one.git";

async function setTheme(page: Page, theme: "light" | "dark" | "high-contrast"): Promise<void> {
  await page.evaluate((t) => document.documentElement.setAttribute("data-theme", t), theme);
  await page.evaluate(() => new Promise<void>((done) => requestAnimationFrame(() => requestAnimationFrame(() => done()))));
}

/** An evidence screenshot of the window (only when asked for): the running app, never a fixture. */
async function shots(page: Page, name: string, both = false): Promise<void> {
  if (!SHOTS) return;
  mkdirSync(SHOTS, { recursive: true });
  for (const density of both ? (["comfortable", "compact"] as const) : (["comfortable"] as const)) {
    if (both) await setDensity(page, density);
    for (const theme of ["light", "dark"] as const) {
      await setTheme(page, theme);
      await page.mouse.move(520, 300);
      // A theme change fades its colours: the picture is taken once they have arrived.
      await page.waitForTimeout(400);
      await page.screenshot({ path: join(SHOTS, `${name}-${theme}-${density}.png`) });
    }
  }
  if (both) await setDensity(page, "comfortable");
  await setTheme(page, "light");
}

// ------------------------------------------------------------------ the bridge's log

/** The preload method each value names, and the channel main answers it on. */
const CHANNEL: Record<string, string> = {
  agentHeaders: "agents:headers",
  changesState: "changes:state",
  runMode: "runmode:get",
  pendingApprovals: "conversation:approvals",
  contextAccounting: "accounting:get",
  customizeHooks: "customize:hooks",
  taskEconomics: "task:economics",
  checkpoints: "checkpoints:list",
  transcript: "conversation:transcript",
  sessionSnapshot: "session:snapshot",
  terminalList: "terminal:list",
};

interface BridgeCall {
  channel: string;
  args: unknown[];
  result: unknown;
  atMs: number;
}

/**
 * Taps every handler the main process answers the preload on, from the test: each call is kept with its arguments and what
 * main answered (which is what the Core answered, converted). The renderer cannot reach this log and the product has no such tap.
 */
async function tapBridge(app: ElectronApplication): Promise<void> {
  await app.evaluate(({ ipcMain, BrowserWindow }, channels) => {
    const g = globalThis as unknown as { __bridgeLog?: unknown[] };
    if (g.__bridgeLog) return;
    const log: unknown[] = (g.__bridgeLog = []);
    const handlers = (ipcMain as unknown as { _invokeHandlers: Map<string, (...a: unknown[]) => unknown> })._invokeHandlers;
    // The events main pushes to the window after the snapshot: what the task's card (its state, its children) is folded from.
    for (const w of BrowserWindow.getAllWindows()) {
      const send = w.webContents.send.bind(w.webContents);
      w.webContents.send = (channel: string, ...args: unknown[]) => {
        const e = args[0] as { taskId?: string | null; aggregateType?: string } | undefined;
        if (channel === "core:event" && e?.taskId && e.aggregateType !== "assistant_stream") log.push({ channel, args: [e.taskId], result: null, atMs: Date.now() });
        send(channel, ...args);
      };
    }
    for (const channel of channels) {
      const fn = handlers.get(channel);
      if (!fn) throw new Error(`no handler for ${channel}`);
      handlers.set(channel, async (event: unknown, ...args: unknown[]) => {
        const result = await fn(event, ...args);
        log.push({ channel, args, result, atMs: Date.now() });
        if (log.length > 4000) log.splice(0, log.length - 4000);
        return result;
      });
    }
  }, Object.values(CHANNEL));
}

/** The latest answer on a channel whose arguments name `id` (a task or a session); null when the bridge never answered one. */
async function lastCall<T>(app: ElectronApplication, channel: string, id: string): Promise<T | null> {
  return app.evaluate(
    (_e, [ch, needle]) => {
      const log = ((globalThis as unknown as { __bridgeLog?: BridgeCall[] }).__bridgeLog ?? []) as BridgeCall[];
      for (let i = log.length - 1; i >= 0; i--) if (log[i]!.channel === ch && JSON.stringify(log[i]!.args).includes(needle!)) return { found: log[i]!.result as never };
      return null;
    },
    [channel, id],
  ).then((r) => (r ? ((r as { found: T }).found ?? ({} as T)) : null));
}

/** The reads that name the task; the others name the session, and the terminal list names neither (it is the Core's whole list). */
const BY_TASK = new Set(["changesState", "runMode", "contextAccounting", "taskEconomics", "checkpoints", "transcript"]);

/** Whether the bridge answered `method` for this task or session. The card's own facts come from the session's snapshot and the task's events after it. */
async function answered(app: ElectronApplication, method: string, taskId: string, sessionId: string): Promise<boolean> {
  const channel = CHANNEL[method];
  if (!channel) return false;
  if (method === "sessionSnapshot") return (await lastCall(app, channel, sessionId)) !== null || (await lastCall(app, "core:event", taskId)) !== null;
  return (await lastCall(app, channel, BY_TASK.has(method) ? taskId : method === "terminalList" ? "" : sessionId)) !== null;
}

/** The words the rail writes that are not values: headings, labels and controls. Everything else it shows must be a value with a read behind it. */
const STATIC = new Set(["Details", "Environment", "Approvals", "Context", "Tool servers", "Changes", "Evidence", "Background tasks", "Cost", "change", "manage", "restore", "Restore", "Review", "Verification", "show", "hide", "Stop it", "Keep", "State", "Phase", "Agents", "Updates recorded", "Attachments", "Risk", "Security", "Next", "Run mode:", "Cost cap:", "Time cap:", "branch not read yet"]);

/**
 * What in the rail's body has no Core read behind it (empty when everything has): a value that names no read, a value naming a
 * read the bridge never answered for this task or session, and any text that is neither a value nor one of the rail's own words.
 */
async function unreadValues(app: ElectronApplication, page: Page, taskId: string, sessionId: string): Promise<string[]> {
  const found = await page.getByTestId("rail-body").evaluate((el) => {
    const values = [...el.querySelectorAll<HTMLElement>("[data-rail-value]")].map((v) => ({ text: (v.textContent ?? "").trim(), src: v.getAttribute("data-src") ?? "" }));
    const loose: string[] = [];
    const walk = document.createTreeWalker(el, NodeFilter.SHOW_TEXT);
    for (let n = walk.nextNode(); n; n = walk.nextNode()) {
      const text = (n.textContent ?? "").trim();
      if (text && !n.parentElement?.closest("[data-rail-value]")) loose.push(text);
    }
    return { values, loose };
  });
  const bad: string[] = [];
  for (const text of found.loose) if (!STATIC.has(text)) bad.push(`"${text}" is on the rail and is not a value`);
  for (const v of found.values) {
    if (!v.src) bad.push(`"${v.text}" names no read`);
    else for (const method of v.src.split(" ")) if (!(await answered(app, method, taskId, sessionId))) bad.push(`"${v.text}" names ${method}, which the bridge never answered for this task`);
  }
  return bad;
}

interface Header {
  taskId: string;
  workspaceRoot: string;
  checkoutRoot?: string;
  executionLocation: string;
  filesChanged: number;
  linesAdded: number;
  linesRemoved: number;
  taskState: string;
  title: string;
}
interface Economics {
  verified: boolean;
  checksPassed: number;
  checksFailed: number;
  model: string;
  modelCalls: number;
  costUsd: number;
  pricingKnown: boolean;
}
interface Accounting {
  available: boolean;
  usedBp: number;
  windowSource: string;
  windowTokens: string;
  budgets: { maxCostMinor: string; spentMinor: string } | null;
}
interface Checkpoints {
  checkpoints: { turnId: string; status: string }[];
}

const MACHINE = process.platform === "darwin" ? "This Mac" : process.platform === "win32" ? "This PC" : "This computer";
const first = (page: Page, id: string) => page.getByTestId("rail-body").getByTestId(id).first();

/**
 * Where the rail differs from what the bridge last answered (empty when it does not): each section's value is computed here from
 * the logged answer, not read back from the page. Polled, because the rail and the log are written by the same answers a moment apart.
 */
async function railDiffersFromCore(app: ElectronApplication, page: Page, taskId: string, sessionId: string): Promise<string[]> {
  const bad: string[] = [];
  const same = async (what: string, id: string, want: string) => {
    const got = ((await first(page, id).textContent()) ?? "").replace(/\s+/g, " ").trim();
    if (got !== want) bad.push(`${what}: the rail says "${got}", the Core "${want}"`);
  };
  const header = (await lastCall<{ headers: Header[] }>(app, CHANNEL.agentHeaders!, sessionId))?.headers.find((h) => h.taskId === taskId);
  if (!header) return ["no header for the task in the bridge's log"];
  // Environment.
  await same("location", "conv-location", header.executionLocation === "local" ? MACHINE : header.executionLocation === "cloud" ? "Isolated cloud" : header.executionLocation);
  if ((await first(page, "conv-location").getAttribute("data-location")) !== header.executionLocation) bad.push("location: not the header's executionLocation");
  if ((await first(page, "rail-folder").getAttribute("title")) !== (header.checkoutRoot || header.workspaceRoot)) bad.push("folder: not the header's root");
  const changes = await lastCall<{ branch: string }>(app, CHANNEL.changesState!, taskId);
  if (!changes) bad.push("branch: the change set was never read");
  else await same("branch", "top-bar-branch", changes.branch);
  const mode = await lastCall<{ mode: string; rulesInForce: number }>(app, CHANNEL.runMode!, taskId);
  if (!mode) bad.push("run mode: never read");
  else {
    if ((await first(page, "rail-runmode").getAttribute("data-mode")) !== mode.mode) bad.push(`run mode: the rail is not on ${mode.mode}`);
    await same("saved rules", "rail-rules-count", `${mode.rulesInForce} saved ${mode.rulesInForce === 1 ? "rule" : "rules"}`);
  }
  // Approvals.
  const waiting = ((await lastCall<{ taskId: string }[]>(app, CHANNEL.pendingApprovals!, sessionId)) ?? []).filter((a) => a.taskId === taskId).length;
  await same("approvals", "rail-approvals-waiting", waiting > 0 ? `${waiting} waiting` : "None waiting");
  // Context.
  const accounting = await lastCall<Accounting>(app, CHANNEL.contextAccounting!, taskId);
  if (!accounting) bad.push("context: never read");
  else if (accounting.available && accounting.windowSource !== "UNKNOWN" && accounting.windowTokens !== "0") await same("context", "rail-context-use", `${Math.min(100, Math.round(accounting.usedBp / 100))}%`);
  // Changes.
  await same("changes", "rail-changes-size", header.filesChanged > 0 ? `+${header.linesAdded} −${header.linesRemoved} · ${header.filesChanged} ${header.filesChanged === 1 ? "file" : "files"}` : "No changes yet");
  // Evidence.
  const economics = await lastCall<Economics>(app, CHANNEL.taskEconomics!, taskId);
  if (!economics) bad.push("economics: never read");
  else {
    const total = economics.checksPassed + economics.checksFailed;
    const counts = total === 0 ? (economics.verified ? "verified" : "not run yet") : economics.verified ? `verified, ${economics.checksPassed} of ${total} passed` : `${economics.checksPassed} of ${total} passed`;
    await same("verification", "rail-verification", counts);
    // Cost.
    await same("spend", "rail-spend", economics.pricingKnown ? `$${economics.costUsd.toFixed(economics.costUsd >= 1 ? 2 : 4)}` : "not priced");
    await same("model calls", "rail-model-calls", `${economics.modelCalls} model ${economics.modelCalls === 1 ? "call" : "calls"}`);
    if (economics.model) await same("model", "rail-model", economics.model);
  }
  const list = await lastCall<Checkpoints>(app, CHANNEL.checkpoints!, taskId);
  const turns = new Set((list?.checkpoints ?? []).filter((c) => c.turnId && (c.status === "CURRENT" || c.status === "SUPERSEDED")).map((c) => c.turnId)).size;
  if (turns > 0) await same("checkpoints", "rail-checkpoints-count", `${turns} ${turns === 1 ? "checkpoint" : "checkpoints"}`);
  else if ((await first(page, "rail-checkpoints-count").count()) !== 0) bad.push("checkpoints: the rail counts some and the Core lists none");
  if (accounting?.budgets && accounting.budgets.maxCostMinor !== "0") await same("budget", "rail-budget-cost", `${Number(accounting.budgets.spentMinor).toLocaleString("en-US")} of ${Number(accounting.budgets.maxCostMinor).toLocaleString("en-US")} minor units`);
  return bad;
}

const railMatchesCore = (app: ElectronApplication, page: Page, taskId: string, sessionId: string, message: string) => expect.poll(() => railDiffersFromCore(app, page, taskId, sessionId), { timeout: 30_000, message }).toEqual([]);

/** The task's phase and agent counts as the Core folds them from its log, asked through the bridge. */
const coreFacts = (page: Page, taskId: string) => page.evaluate((t) => window.modbit.transcript(t, { limit: 1 }).then((p) => (p.facts ? { phase: p.facts.phase, agents: `${p.facts.agents.running + p.facts.agents.background}/${p.facts.agents.total}`, attachments: String(p.facts.attachments) } : null)), taskId);
const railFacts = async (page: Page) => ({ phase: await page.getByTestId("task-details").getAttribute("data-phase"), agents: await page.getByTestId("task-agents").textContent(), attachments: await page.getByTestId("task-attachments").textContent() });

test("PX-201: the details rail shows the Core's facts for the open task, each from a read the bridge answered; Stop stops a terminal and a child task; Review and restore open what asks; a reload keeps the Core's phase; at 800 the rail is absent and More holds its facts; the popover is gone", async () => {
  test.setTimeout(480_000);
  const repo = mkdtempSync(join(tmpdir(), "modbit-px201-repo-"));
  mkdirSync(join(repo, "src", "a"), { recursive: true });
  writeFileSync(join(repo, "src", "a", ".keep"), "");
  makeRepo(repo);
  const hold = new Gate();
  const childHold = new Gate();
  const parent: ModelPart[][] = [
    ["I will tidy the notes, keep a watcher running and hand module a to a child.", { call: { name: "plan.update", args: { outcome: "the notes are tidy and module a exists", expected_files: ["notes.txt"], steps: [{ id: "a", title: "module a" }], protected_effects: ["git.merge", "shell.exec"] } } }],
    [{ call: { name: "fs.read", args: { path: "notes.txt" } } }],
    [{ call: { name: "change.apply", args: { path: "notes.txt", op: "replace", content: "line 1\nline 2 tidied\nline 3\nline 4\n" } } }],
    [{ call: nodeTerminal(ECHO_LOOP) }],
    [{ call: { name: "agent.spawn", args: { idempotency_key: "child-a", objective: CHILD_GOAL, write_scope: ["src/a/"], work_node: "a", max_turns: 6, mode: "BACKGROUND" } } }],
    ["Adding the remote.", { call: { name: "shell.exec", args: { argv: ["git", "remote", "add", "origin1", URL1] } } }],
    ["That is all for now."],
  ];
  const child: ModelPart[][] = [[{ call: { name: "plan.update", args: { outcome: "a.txt exists", expected_files: ["src/a/a.txt"] } } }], [{ call: { name: "change.apply", args: { path: "src/a/a.txt", op: "replace", content: "alpha\n" } } }], [{ call: { name: "task.complete", args: { summary: "created src/a/a.txt", self_review: { findings: [] } } } }]];
  const model = await routedModel(async (goal, results) => {
    if (goal === CHILD_GOAL) {
      if (results === 1) await childHold.opened;
      return child[results] ?? ["Nothing further."];
    }
    if (results === 5) await hold.opened;
    return parent[results] ?? ["Nothing further."];
  });
  const { app, page } = await launch(mkdtempSync(join(tmpdir(), "modbit-e2e-px201-")), { env: { MODBIT_OPENAI_BASE_URL: model.url, MODBIT_CAPACITY: "model=4,provider=8" } });
  try {
    await tapBridge(app);
    await setContentSize(app, page, 1440, 900);
    await setTheme(page, "light");
    const taskId = await runTask(page, GOAL, repo);
    const sessionId = await sessionOf(page);
    await expect(page.getByTestId("conversation")).toHaveAttribute("data-loaded", "true");
    // A budget, set as a person would from the context tray's form: through the bridge, by the Core.
    await page.evaluate(([s, t]) => window.modbit.setTaskBudgets(s!, t!, { maxCostMinor: "1000", maxWallMs: "3600000" }), [sessionId, taskId]);

    // ---- The model is held: the task runs with a child task, a background terminal, checkpoints and a budget.
    const rail = page.getByTestId("details-rail");
    await expect(rail).toBeVisible();
    await expect(rail.getByTestId("child-card")).toHaveCount(1, { timeout: 90_000 });
    await expect(rail.getByTestId("rail-terminal")).toHaveCount(1, { timeout: 30_000 });
    await expect(rail.getByTestId("rail-budget-cost")).toBeVisible({ timeout: 30_000 });

    // Section 5.1: 320 wide beside the conversation, inside the task area; the toggle in the title bar says it is shown.
    const railBox = await box(page, "details-rail");
    const centre = await box(page, "region-conversation");
    const conversation = await box(page, "conversation");
    expect(railBox.width).toBe(320);
    expect(await page.getByTestId("shell").getAttribute("data-details-px")).toBe("320");
    expect(Math.abs(railBox.x + railBox.width - (centre.x + centre.width)), "the rail ends where the task area ends").toBeLessThanOrEqual(1);
    expect(Math.abs(conversation.x + conversation.width - railBox.x), "the conversation ends where the rail starts").toBeLessThanOrEqual(1);
    await expect(page.getByTestId("details-toggle")).toHaveAttribute("aria-pressed", "true");
    const toggles = await page.getByTestId("top-bar").locator("button[aria-pressed]").evaluateAll((els) => els.map((e) => e.getAttribute("data-testid")));
    expect(toggles[0], "Details is the first toggle").toBe("details-toggle");
    // The popover is gone, and what it and the title bar and the composer's foot held is in the rail.
    await expect(page.locator('[role="dialog"][data-testid="task-details"]')).toHaveCount(0);
    for (const id of ["top-bar-state", "top-bar-branch", "conv-location"]) {
      await expect(rail.getByTestId(id), `${id} is in the rail`).toHaveCount(1);
      await expect(page.getByTestId("top-bar").getByTestId(id), `${id} is not in the title bar`).toHaveCount(0);
      await expect(page.getByTestId("composer-foot").getByTestId(id), `${id} is not under the composer`).toHaveCount(0);
    }
    await expect(rail.locator("section")).toHaveCount(7);
    expect(await rail.locator("section").evaluateAll((els) => els.map((e) => e.getAttribute("aria-label")))).toEqual(["Environment", "Approvals", "Context", "Changes", "Evidence", "Background tasks", "Cost"]);

    // ---- Every value has a read behind it, and says what the read answered.
    await expect.poll(() => unreadValues(app, page, taskId, sessionId), { timeout: 30_000, message: "every value of the rail names a read the bridge answered" }).toEqual([]);
    await railMatchesCore(app, page, taskId, sessionId, "the running task");
    await expect(rail.getByTestId("top-bar-state")).toHaveText("Running");
    await expect(rail.getByTestId("conv-location")).toHaveText(MACHINE);
    await expect(rail.getByTestId("top-bar-branch")).toHaveText(git(repo, "rev-parse", "--abbrev-ref", "HEAD").trim());
    await expect(rail.getByTestId("rail-changes-size")).toHaveText("+2 −1 · 1 file");
    await expect(rail.getByTestId("rail-budget-cost")).toContainText("of 1,000 minor units");
    expect(Number(await rail.getByTestId("rail-checkpoints").getAttribute("data-count")), "at least two checkpoints").toBeGreaterThanOrEqual(2);
    await expect(rail.getByTestId("child-goal")).toHaveText(CHILD_GOAL);
    const childId = (await rail.getByTestId("child-card").getAttribute("data-task-id"))!;
    const coreChild = async () => (await page.evaluate((s) => window.modbit.agentHeaders(s, true), sessionId)).headers.find((h) => h.taskId === childId);
    expect((await coreChild())?.subagent, "the child is the Core's subagent of this session").toBe(true);
    await expect.poll(async () => (await coreChild())?.taskState).toBe(await rail.getByTestId("child-state").textContent());
    const terminals = await page.evaluate((t) => window.modbit.terminalList(t).then((l) => l.terminals), taskId);
    expect(terminals).toHaveLength(1);
    await expect(rail.getByTestId("rail-terminal-title")).toHaveText(terminals[0]!.title);
    // Tool servers: the Core lists none for this session (no extension declares one), so the section is left out.
    expect((await page.evaluate((s) => window.modbit.customizeHooks(s), sessionId)).extensions.flatMap((e) => e.servers)).toEqual([]);
    await expect(rail.getByTestId("rail-tool-servers")).toHaveCount(0);

    // The retained mutations: a value with no read, a value naming a read the bridge never answered, words that are no value, and a made-up number that names a real read.
    const inject = (html: string) => page.evaluate((h) => {
      const host = document.createElement("div");
      host.id = "px201-mutation";
      host.innerHTML = h;
      const body = document.querySelector('[data-testid="rail-body"]')!;
      body.insertBefore(host, body.firstChild);
    }, html);
    const eject = () => page.evaluate(() => document.getElementById("px201-mutation")?.remove());
    await inject('<span data-rail-value="">12 widgets</span>');
    expect(await unreadValues(app, page, taskId, sessionId)).toEqual(['"12 widgets" names no read']);
    await eject();
    await inject('<span data-rail-value="" data-src="reviewBundle">+20 −1</span>');
    expect(await unreadValues(app, page, taskId, sessionId)).toEqual(['"+20 −1" names reviewBundle, which the bridge never answered for this task']);
    await eject();
    await inject("<span>3 connected</span>");
    expect(await unreadValues(app, page, taskId, sessionId)).toEqual(['"3 connected" is on the rail and is not a value']);
    await eject();
    await inject('<span data-testid="rail-spend" data-rail-value="" data-src="taskEconomics">$9.99</span>');
    expect(await unreadValues(app, page, taskId, sessionId), "it names a real read").toEqual([]);
    expect((await railDiffersFromCore(app, page, taskId, sessionId)).filter((d) => d.startsWith("spend:")), "and is not what the read answered").toHaveLength(1);
    await eject();
    await inject('<span data-testid="conv-location" data-location="local" data-rail-value="" data-src="agentHeaders">Isolated cloud</span>');
    expect((await railDiffersFromCore(app, page, taskId, sessionId)).filter((d) => d.startsWith("location:")), "a hard-coded location is not the header's").toHaveLength(1);
    await eject();
    await railMatchesCore(app, page, taskId, sessionId, "clean again");

    // The rail in the three themes, and the evidence pictures.
    for (const theme of ["light", "dark", "high-contrast"] as const) {
      await setTheme(page, theme);
      await accessible(page, `the details rail in ${theme}`, '[data-testid="details-rail"]');
    }
    await setTheme(page, "light");
    await shots(page, "running-child-terminal-checkpoints-budget", true);

    // ---- Each control opens what exists today.
    await rail.getByTestId("rail-runmode").click();
    await expect(page.getByTestId("runmode-dialog")).toBeVisible();
    await page.getByTestId("runmode-close").click();
    await expect(page.getByTestId("runmode-dialog")).toHaveCount(0);
    await rail.getByTestId("rail-rules").click();
    await expect(page.getByTestId("runmode-dialog")).toBeVisible();
    await page.getByTestId("runmode-close").click();
    await expect(page.getByTestId("runmode-dialog")).toHaveCount(0);
    // Context: the same tray the ring opens, and both say it is open.
    await expect(async () => {
      if ((await rail.getByTestId("rail-context-toggle").getAttribute("aria-expanded")) !== "true") await rail.getByTestId("rail-context-toggle").click();
      await expect(page.getByTestId("context-tray")).toBeVisible({ timeout: 2_000 });
    }).toPass({ timeout: 20_000 });
    await expect(page.getByTestId("context-ring")).toHaveAttribute("aria-expanded", "true");
    await expect(async () => {
      if ((await rail.getByTestId("rail-context-toggle").getAttribute("aria-expanded")) === "true") await rail.getByTestId("rail-context-toggle").click();
      await expect(page.getByTestId("context-tray")).toHaveCount(0, { timeout: 2_000 });
    }).toPass({ timeout: 20_000 });

    // ---- Stop on the background terminal: it asks once more, then the Core kills it.
    const terminalId = terminals[0]!.terminalId;
    const terminalState = () => page.evaluate((t) => window.modbit.terminalList(t).then((l) => l.terminals.map((x) => [x.terminalId, x.state.toLowerCase()])), taskId);
    expect(await terminalState()).toEqual([[terminalId, "running"]]);
    await rail.getByTestId("rail-terminal-stop").click();
    await expect(rail.getByTestId("rail-terminal-stop-confirm")).toBeFocused();
    expect(await terminalState(), "the first click stops nothing").toEqual([[terminalId, "running"]]);
    await rail.getByTestId("rail-terminal-stop-cancel").click();
    await expect(rail.getByTestId("rail-terminal-stop")).toBeVisible();
    expect(await terminalState(), "Keep keeps it").toEqual([[terminalId, "running"]]);
    await rail.getByTestId("rail-terminal-stop").click();
    await rail.getByTestId("rail-terminal-stop-confirm").click();
    await expect.poll(async () => (await terminalState())[0]?.[1], { timeout: 30_000, message: "the Core's terminal is no longer running" }).not.toBe("running");
    await expect(rail.getByTestId("rail-terminal")).toHaveCount(0, { timeout: 15_000 });
    await expect(rail.getByTestId("rail-finished")).toHaveAttribute("data-count", "1");

    // ---- Stop on the child task: the same confirmation Cancel task asks; Escape steps back, Enter cancels.
    await rail.getByTestId("child-stop").click();
    await expect(page.getByTestId("confirm")).toHaveAttribute("data-confirm-kind", "cancel");
    await expect(page.getByTestId("confirm")).toContainText(CHILD_GOAL);
    await page.keyboard.press("Escape");
    await expect(page.getByTestId("confirm")).toHaveCount(0);
    expect((await coreChild())?.taskState, "Escape cancelled nothing").not.toBe("Cancelled");
    await rail.getByTestId("child-stop").click();
    await expect(page.getByTestId("confirm")).toHaveAttribute("data-confirm-kind", "cancel");
    await page.keyboard.press("Enter");
    await expect.poll(async () => (await coreChild())?.taskState, { timeout: 30_000 }).toBe("Cancelled");
    await expect(rail.getByTestId("child-stop")).toHaveCount(0, { timeout: 15_000 });
    await expect(rail.getByTestId("rail-finished")).toHaveAttribute("data-count", "2");
    await rail.getByTestId("rail-finished").click();
    await expect(rail.getByTestId("rail-finished-list").getByTestId("child-state")).toHaveText("Cancelled");
    await expect(rail.getByTestId("rail-terminal-ended")).toHaveCount(1);
    await expect(page.getByTestId("conversation"), "the parent's conversation is still the one open").toHaveAttribute("data-task-id", taskId);
    await expect.poll(() => unreadValues(app, page, taskId, sessionId), { timeout: 30_000 }).toEqual([]);
    await rail.getByTestId("rail-finished").click();

    // ---- The task asks for a protected effect: Approvals says one waits, of which kind, from the Core's list.
    hold.open();
    await expect(page.getByTestId("approval-card")).toBeVisible({ timeout: 90_000 });
    await expect(rail.getByTestId("rail-approvals-waiting")).toHaveText("1 waiting", { timeout: 30_000 });
    await expect(rail.getByTestId("rail-approvals-waiting-row")).toContainText("protected write");
    await expect(rail.getByTestId("top-bar-state")).toHaveText("Waiting");
    await railMatchesCore(app, page, taskId, sessionId, "the task waiting on approval");
    await expect.poll(() => unreadValues(app, page, taskId, sessionId), { timeout: 30_000 }).toEqual([]);
    await accessible(page, "the rail beside a docked approval", '[data-testid="shell"]');
    await shots(page, "waiting-on-approval", true);

    // ---- Restore from a checkpoint: the rail lists the Core's turn boundaries and opens the dialog that previews and asks. Cancel changes nothing.
    const before = sha256File(join(repo, "notes.txt"));
    await rail.getByTestId("rail-checkpoints").click();
    const points = rail.getByTestId("rail-checkpoint");
    expect(await points.count()).toBe(Number(await rail.getByTestId("rail-checkpoints").getAttribute("data-count")));
    const listed = await page.evaluate((t) => window.modbit.checkpoints(t).then((l) => l.checkpoints.map((c) => c.checkpointId)), taskId);
    for (const id of await points.evaluateAll((els) => els.map((e) => e.getAttribute("data-checkpoint-id")))) expect(listed, "each is a checkpoint the Core lists").toContain(id);
    const turnOne = rail.locator('[data-testid="rail-checkpoint"][data-turn-ordinal="1"]');
    await turnOne.getByTestId("rail-restore").click();
    await expect(page.getByTestId("restore-dialog")).toBeVisible();
    await expect(page.getByTestId("restore-intro")).toContainText("end of turn 1");
    await expect(page.getByTestId("restore-summary")).toBeVisible({ timeout: 30_000 });
    expect(await page.getByTestId("restore-file").evaluateAll((els) => els.map((e) => e.getAttribute("data-path"))), "the Core's preview names the file the task changed").toContain("notes.txt");
    await page.getByTestId("restore-cancel").click();
    await expect(page.getByTestId("restore-dialog")).toHaveCount(0);
    expect(sha256File(join(repo, "notes.txt")), "previewing and cancelling changed nothing").toBe(before);
    await rail.getByTestId("rail-checkpoints").click();
    await expect(rail.getByTestId("rail-checkpoint-list")).toHaveCount(0);

    // ---- Details in the rail's head: closed, the facts are clipped and in the accessibility tree; open, they are the Core's.
    const details = page.getByTestId("task-details");
    await expect(details).toHaveAttribute("data-open", "false");
    const clipped = (await details.boundingBox())!;
    expect(clipped.width <= 1 && clipped.height <= 1, "closed, the facts take no room").toBe(true);
    const tree = await rail.ariaSnapshot();
    for (const word of ["State", "Phase", "Agents", "Updates recorded", "Attachments"]) expect(tree, `${word} is in the accessibility tree while Details is closed`).toContain(word);
    await rail.getByTestId("rail-details-toggle").click();
    await expect(details).toHaveAttribute("data-open", "true");
    await expect(rail.getByTestId("rail-details-toggle")).toHaveAttribute("aria-expanded", "true");
    await expect(details.getByTestId("task-state")).toHaveText("Waiting");
    await expect(details.getByTestId("task-economics")).toBeVisible();
    await expect.poll(() => railFacts(page), { timeout: 30_000, message: "the phase and the agent counts are the Core's" }).toEqual(await coreFacts(page, taskId));
    await accessible(page, "the rail with Details open", '[data-testid="details-rail"]');
    await shots(page, "details-open");

    // ---- A reload: the phase and the agent counts equal the Core's, not a window's defaults.
    const live = await railFacts(page);
    expect(live.phase, "the Core's phase is not the default").not.toBe("drafting");
    expect(live.agents, "the task has agents").not.toBe("0/0");
    await page.reload();
    await openTask(page, taskId);
    await expect(page.getByTestId("details-rail")).toBeVisible();
    await expect.poll(() => railFacts(page), { timeout: 30_000, message: "after a reload the phase and agent counts equal the Core's" }).toEqual(await coreFacts(page, taskId));
    expect(await railFacts(page)).toEqual(live);
    // The retained mutation: a card that kept the window's defaults (what a reload gave before PX-196) is not the Core's.
    expect({ phase: "drafting", agents: "0/0", attachments: "0" }, "a default phase after a reload would fail").not.toEqual(await coreFacts(page, taskId));
    await expect(page.getByTestId("rail-approvals-waiting")).toHaveText("1 waiting", { timeout: 30_000 });
    await expect(page.getByTestId("rail-finished")).toHaveAttribute("data-count", "2");
    expect(await lastCall(app, CHANNEL.sessionSnapshot!, sessionId), "the reload read the session's snapshot").not.toBeNull();
    await expect.poll(() => unreadValues(app, page, taskId, sessionId), { timeout: 30_000 }).toEqual([]);
    await railMatchesCore(app, page, taskId, sessionId, "after the reload");

    // ---- Review opens the changes.
    await page.getByTestId("rail-review").click();
    await expect(page.getByTestId("review")).toBeVisible({ timeout: 30_000 });
    await expect(page.getByTestId("details-rail"), "the rail is the conversation's, not the review's").toHaveCount(0);
    await page.getByTestId("review-close").click();
    await expect(page.getByTestId("review")).toHaveCount(0);
    await expect(page.getByTestId("details-rail")).toBeVisible();

    // ---- More, with the rail shown, holds what the popover held and the rail does not: density, undo all, browser, cancel.
    const more = await openTaskMore(page);
    await expect(more).toHaveAttribute("data-holds-details", "false");
    await expect(more.getByTestId("rail-body")).toHaveCount(0);
    for (const id of ["conv-density", "undo-all", "task-browser", "task-cancel"]) await expect(more.getByTestId(id)).toBeVisible();
    await accessible(page, "More beside the rail", '[data-testid="shell"]');
    await page.keyboard.press("Escape");
    await expect(page.getByTestId("task-more")).toHaveCount(0);
    await expect(page.getByTestId("approval-card"), "Escape closed More and decided nothing").toBeVisible();

    // ---- The Details toggle hides the rail, the choice is stored with the layout preferences and survives a reload; its facts are then under More.
    await page.getByTestId("details-toggle").click();
    await expect(page.getByTestId("details-rail")).toHaveCount(0);
    await expect(page.getByTestId("details-toggle")).toHaveAttribute("aria-pressed", "false");
    expect(await page.evaluate(() => JSON.parse(localStorage.getItem("modbit.ui.v1") ?? "{}").detailsOpen)).toBe(false);
    await page.reload();
    await openTask(page, taskId);
    await expect(page.getByTestId("conversation")).toHaveAttribute("data-loaded", "true");
    await expect(page.getByTestId("details-toggle")).toHaveAttribute("aria-pressed", "false");
    await expect(page.getByTestId("details-rail")).toHaveCount(0);
    await expect((await openTaskMore(page)).getByTestId("rail-body")).toHaveCount(1);
    await page.keyboard.press("Escape");
    await page.getByTestId("details-toggle").click();
    await expect(page.getByTestId("details-rail")).toBeVisible();
    expect(await page.evaluate(() => JSON.parse(localStorage.getItem("modbit.ui.v1") ?? "{}").detailsOpen)).toBe(true);
    // It gives way while the apps panel is open, and asking for it back closes the panel.
    await page.getByTestId("pane-toggle-changes").click();
    await expect(page.getByTestId("region-apps")).toBeVisible();
    await expect(page.getByTestId("details-rail")).toHaveCount(0);
    await expect(page.getByTestId("details-toggle")).toHaveAttribute("aria-pressed", "false");
    await page.getByTestId("details-toggle").click();
    await expect(page.getByTestId("region-apps")).toHaveCount(0);
    await expect(page.getByTestId("details-rail")).toBeVisible();

    // ---- At a task area of 800 the rail is absent and More holds its facts; one pixel more and it is back.
    const list = Number(await page.getByTestId("shell").getAttribute("data-list-px"));
    await setContentSize(app, page, list + 801, 900);
    await expect(page.getByTestId("details-rail")).toHaveCount(1);
    await setContentSize(app, page, list + 800, 900);
    await expect(page.getByTestId("shell")).toHaveAttribute("data-center-px", "800");
    await expect(page.getByTestId("details-rail")).toHaveCount(0);
    await expect(page.getByTestId("details-toggle"), "no toggle for a rail that cannot be shown").toHaveCount(0);
    await setContentSize(app, page, 1000, 900);
    const under = await openTaskMore(page);
    await expect(under).toHaveAttribute("data-holds-details", "true");
    for (const id of ["top-bar-state", "conv-location", "rail-folder", "top-bar-branch", "rail-runmode", "rail-approvals-waiting", "rail-rules", "rail-context-toggle", "rail-changes-size", "rail-review", "rail-verification", "rail-checkpoints", "rail-finished", "rail-spend", "rail-model-calls", "rail-budget-cost", "rail-details-toggle", "task-phase", "task-agents", "conv-density", "undo-all", "task-browser", "task-cancel"]) await expect(under.getByTestId(id), `${id} is under More`).toHaveCount(1);
    await expect.poll(() => unreadValues(app, page, taskId, sessionId), { timeout: 30_000 }).toEqual([]);
    await railMatchesCore(app, page, taskId, sessionId, "under More");
    const moreBox = (await under.boundingBox())!;
    expect(moreBox.x >= 0 && moreBox.x + moreBox.width <= 1000 && moreBox.y + moreBox.height <= 900, "More is inside the window").toBe(true);
    const controls = (await under.getByTestId("task-cancel").boundingBox())!;
    expect(controls.y, "the controls stand first, above the details").toBeLessThan((await under.getByTestId("rail-body").boundingBox())!.y);
    // More never covers a docked decision: the approval card's answers stay on top of it and can be clicked.
    for (const id of ["approval-run", "approval-skip"]) expect(await page.getByTestId(id).evaluate((el) => { const r = el.getBoundingClientRect(); const at = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2); return at !== null && el.contains(at); }), `${id} is not under More`).toBe(true);
    for (const theme of ["light", "dark", "high-contrast"] as const) {
      await setTheme(page, theme);
      await accessible(page, `More holding the details in ${theme}`, '[data-testid="shell"]');
    }
    await setTheme(page, "light");
    await shots(page, "window-1000-rail-gone-more-open");
    await page.keyboard.press("Escape");
    await expect(page.getByTestId("task-more")).toHaveCount(0);

    // ---- At the centre's minimum width nothing covers the composer.
    await liftMinimumSize(app);
    await setContentSize(app, page, list + 448, 900);
    await expect(page.getByTestId("shell")).toHaveAttribute("data-rail", "false");
    await expect(page.getByTestId("details-rail")).toHaveCount(0);
    const narrow = await box(page, "region-conversation");
    const composer = await box(page, "composer");
    expect(composer.x >= narrow.x && composer.x + composer.width <= narrow.x + narrow.width + 0.5, "the composer is inside the task area").toBe(true);
    expect(await page.getByTestId("composer").evaluate((el) => {
      const r = el.getBoundingClientRect();
      return [0.1, 0.5, 0.9].every((f) => { const at = document.elementFromPoint(r.left + r.width * f, r.top + r.height / 2); return at !== null && el.contains(at); });
    }), "nothing is over the composer").toBe(true);
    await setContentSize(app, page, 1440, 900);
    await expect(page.getByTestId("details-rail")).toBeVisible();

    // ---- The approval is skipped: the count goes back to none, and the command never ran.
    await page.getByTestId("approval-skip").click();
    await expect(page.getByTestId("approval-card")).toHaveCount(0);
    await expect(page.getByTestId("rail-approvals-waiting")).toHaveText("None waiting", { timeout: 30_000 });
    expect(git(repo, "remote").trim(), "the skipped command did not run").toBe("");
    await expect(agentRow(page, taskId)).toHaveAttribute("data-state", /Waiting|Completed|ReadyForReview/, { timeout: 60_000 });
    await railMatchesCore(app, page, taskId, sessionId, "at the end");
  } finally {
    hold.open();
    childHold.open();
    await closeApp(app);
    model.server.closeAllConnections();
    model.server.close();
  }
});
