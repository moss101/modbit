/**
 * QUAL-PX-206 / PX-E2E-206 (DR-PX-2026-11-11-021 MWS-13, MWS-14's page shape, section 9): Inbox on the real Electron build, a
 * real Core and a scripted model.
 *
 * Needs you: two approvals (a protected write allowed once and another skipped, each with its real effect or its absence), two
 * mode proposals (one switched, one kept, each read back from the Core), and a destructive effect that offers no decision in
 * Inbox and opens its task, where the card asks twice. Updates: a task that reaches review while Inbox is open is listed with its
 * time, unread; opening it marks it read through the Core and opens its task; the click handler of the notification the OS was
 * given for it opens its task; a second update stays unread. The record survives a kill of the real Core and a restart of the
 * app, with the same entries and read states; filled past its cap it keeps the cap; it is cleared by the person.
 *
 * Every count is checked against the Core's own reads; every control is accounted for; axe is clean. Retained mutations: a
 * destructive effect decided while Inbox showed it; an update record carrying transcript text; Inbox's count drawn from the
 * renderer's own list instead of the Core's attention count; a control nobody proves. Each fails its check.
 * With MODBIT_SHOTS_DIR set the same run writes the evidence screenshots.
 */
import { expect, test, type ElectronApplication, type Page } from "@playwright/test";
import { mkdirSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { createServer, type Server } from "node:http";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { agentRow, expectTaskState, gearMenu, setDensity, toNewTask } from "./support/new-task.ts";
import { accessible, closeApp, git, launch, makeRepo, setContentSize } from "./support/ui-harness.ts";

const SHOTS = process.env.MODBIT_SHOTS_DIR ?? "";
const tmp = (name: string) => mkdtempSync(join(tmpdir(), `modbit-e2e-px206-${name}-`));
const URL1 = "https://example.invalid/one.git";
const URL2 = "https://example.invalid/two.git";
/** What the assistant says in the task that reaches review: it must never reach the record. */
const SAID = "Reviewed-notes-assistant-words-4471";
const SUMMARY = "Summary-words-9932-of-the-review";

// ---------------------------------------------------------------- the scripted model

class Gate {
  private done!: () => void;
  readonly opened = new Promise<void>((d) => (this.done = d));
  open(): void {
    this.done();
  }
}
type Part = string | { call: { name: string; args: unknown } } | { wait: Gate };
interface Body {
  messages?: { role: string; content?: unknown }[];
}

/** An OpenAI-compatible scripted model answering each task from its own script (by a key its goal holds, the step by the tool results so far); a step may wait on a gate. */
function routedModel(routes: Record<string, Part[][]>): Promise<{ server: Server; url: string }> {
  return new Promise((done) => {
    const server = createServer((req, res) => {
      let raw = "";
      req.on("data", (c) => (raw += c));
      req.on("end", async () => {
        const body = JSON.parse(raw || "{}") as Body;
        const text = JSON.stringify(body.messages ?? []);
        const key = Object.keys(routes).find((k) => text.includes(k));
        const results = (body.messages ?? []).filter((m) => m.role === "tool").length;
        const parts = (key ? routes[key]![results] : undefined) ?? ["Nothing further."];
        if (process.env.PX206_DEBUG) process.stderr.write(`[model] ${key ?? "?"} step ${results} roles ${(body.messages ?? []).map((m) => m.role[0]).join("")} last ${JSON.stringify((body.messages ?? []).slice(-2)).slice(0, 600)}\n`);
        for (const p of parts) if (typeof p === "object" && "wait" in p) await p.wait.opened;
        res.writeHead(200, { "content-type": "text/event-stream" });
        const send = (o: unknown) => res.write(`data: ${JSON.stringify(o)}\n\n`);
        let calls = 0;
        for (const p of parts) {
          if (typeof p === "string") send({ id: "c", model: "scripted", choices: [{ index: 0, delta: { content: p }, finish_reason: null }] });
          else if ("call" in p) send({ id: "c", model: "scripted", choices: [{ index: 0, delta: { tool_calls: [{ index: calls, id: `call_${results}_${calls++}`, type: "function", function: { name: p.call.name, arguments: JSON.stringify(p.call.args) } }] }, finish_reason: null }] });
        }
        send({ id: "c", model: "scripted", choices: [{ index: 0, delta: {}, finish_reason: calls > 0 ? "tool_calls" : "stop" }], usage: { prompt_tokens: 10, completion_tokens: 5 } });
        res.write("data: [DONE]\n\n");
        res.end();
      });
    });
    server.listen(0, "127.0.0.1", () => done({ server, url: `http://127.0.0.1:${(server.address() as { port: number }).port}` }));
  });
}

// ---------------------------------------------------------------- screenshots

async function setTheme(page: Page, theme: "light" | "dark"): Promise<void> {
  await page.evaluate((t) => document.documentElement.setAttribute("data-theme", t), theme);
  await page.evaluate(() => new Promise<void>((d) => requestAnimationFrame(() => requestAnimationFrame(() => d()))));
}

/** The evidence screenshots of one state in light and dark and in both densities (only when asked for); `show` brings the state back after Settings changed the density. */
async function shots(page: Page, name: string, show: () => Promise<void>): Promise<void> {
  if (!SHOTS) return;
  mkdirSync(SHOTS, { recursive: true });
  for (const density of ["comfortable", "compact"] as const) {
    await setDensity(page, density);
    await show();
    for (const theme of ["light", "dark"] as const) {
      await setTheme(page, theme);
      await page.waitForTimeout(300);
      await page.screenshot({ path: join(SHOTS, `${name}-${density}-${theme}.png`) });
    }
  }
  await setDensity(page, "comfortable");
  await setTheme(page, "light");
  await show();
}

// ---------------------------------------------------------------- the checks

const sessionOf = (page: Page) => page.evaluate(() => window.modbit.localState().then((l) => l.sessionId ?? ""));
const conversationTask = async (page: Page) => {
  const conv = page.getByTestId("conversation");
  await expect(conv).toHaveAttribute("data-task-id", /.+/, { timeout: 30_000 });
  return (await conv.getAttribute("data-task-id"))!;
};
async function showInbox(page: Page): Promise<void> {
  if (await page.getByTestId("settings-nav").isVisible()) await page.getByTestId("nav-back").click();
  if (!(await page.getByTestId("inbox").isVisible())) await page.getByTestId("open-inbox").click();
  await expect(page.getByTestId("inbox")).toBeVisible();
}

/**
 * The counts are the Core's (empty when they hold): Inbox's count in the task list is the number of tasks the Core lists an
 * attention item for; Needs you's is every pending approval, every other item and every pending proposal the Core lists.
 */
async function countDrift(page: Page, sid: string): Promise<string[]> {
  const core = await page.evaluate(async (s) => {
    const attention = await window.modbit.attention(s);
    const approvals = await window.modbit.dockApprovals(s);
    const headers = await window.modbit.agentHeaders(s);
    const lists = await Promise.all(headers.headers.filter((h) => ["RUNNING", "WAITING", "NEEDS_ATTENTION"].includes(h.statusClass)).map((h) => window.modbit.modeProposals(h.taskId)));
    const pending = lists.flatMap((l) => l.proposals).filter((p) => p.status === "PENDING").length;
    return { tasks: new Set(attention.items.map((i) => i.taskId)).size, needs: approvals.length + attention.items.filter((i) => i.kind !== "APPROVAL").length + pending };
  }, sid);
  const bad: string[] = [];
  const shown = (await page.getByTestId("open-inbox-count").count()) === 1 ? Number(await page.getByTestId("open-inbox-count").textContent()) : 0;
  if (shown !== core.tasks) bad.push(`Inbox's count is ${shown}; the Core's attention count is ${core.tasks}`);
  const label = (await page.getByTestId("inbox-view-needs").textContent()) ?? "";
  if (label !== `Needs you · ${core.needs}`) bad.push(`Needs you says "${label}"; the Core lists ${core.needs}`);
  return bad;
}

/** Whether a destructive effect was decided while Inbox showed it: the Core must still list it, and Inbox must offer no decision for it. */
async function destructiveDecidedFromInbox(page: Page, sid: string, approvalId: string): Promise<string[]> {
  const bad: string[] = [];
  const pending = await page.evaluate((s) => window.modbit.dockApprovals(s).then((l) => l.map((a) => a.approvalId)), sid);
  if (!pending.includes(approvalId)) bad.push("the destructive effect is no longer pending: it was decided while Inbox showed it");
  const card = page.locator(`[data-testid="inbox-approval"][data-approval-id="${approvalId}"]`);
  if ((await card.count()) === 1) {
    for (const id of ["inbox-approval-allow", "inbox-approval-skip"]) if ((await card.getByTestId(id).count()) !== 0) bad.push(`Inbox offers ${id} for an effect that cannot be undone`);
    if ((await card.getByTestId("inbox-approval-open").count()) !== 1) bad.push("the destructive effect does not open its task");
  }
  return bad;
}

const ENTRY_KEYS = ["atMs", "id", "kind", "offset", "taskId", "title"];
/** The record's shape on disk (empty when it holds): each entry is exactly ids, kind, title, time and offset, and no transcript text is anywhere in it. */
function recordDrift(path: string, said: readonly string[]): string[] {
  const raw = readFileSync(path, "utf8");
  const bad: string[] = [];
  const j = JSON.parse(raw) as { entries?: Record<string, unknown>[] };
  for (const e of j.entries ?? []) {
    const keys = Object.keys(e).sort();
    if (keys.join(",") !== ENTRY_KEYS.join(",")) bad.push(`an update holds ${keys.filter((k) => !ENTRY_KEYS.includes(k)).join(", ") || "fewer fields"} beside its ids, title and time`);
    if (!/^[0-9a-f]{32}$/.test(String(e.taskId)) || !/^\d+$/.test(String(e.offset)) || typeof e.atMs !== "number") bad.push("an update's ids or time are not ids or a time");
  }
  for (const s of said) if (raw.includes(s)) bad.push(`the record holds transcript text: "${s}"`);
  return bad;
}

/** Each update's read state as drawn equals the Core's: the task's read offset has reached the update's offset. */
async function readDrift(page: Page, sid: string): Promise<string[]> {
  const { updates, headers } = await page.evaluate(async (s) => ({ updates: await window.modbit.inboxUpdates(), headers: await window.modbit.agentHeaders(s, true) }), sid);
  const readOf = new Map(headers.headers.map((h) => [h.taskId, BigInt(h.readOffset)] as const));
  const bad: string[] = [];
  for (const u of updates.updates) {
    const r = readOf.get(u.taskId);
    const want = r === undefined ? true : BigInt(u.offset) <= r;
    const row = page.locator(`[data-testid="inbox-update"][data-update-id="${u.id}"]`);
    if ((await row.count()) !== 1) {
      bad.push(`update ${u.id.slice(0, 8)} is not drawn`);
      continue;
    }
    const drawn = (await row.getAttribute("data-read")) === "true";
    if (drawn !== want) bad.push(`update ${u.id.slice(0, 8)} is drawn ${drawn ? "read" : "unread"}; the Core says ${want ? "read" : "unread"}`);
  }
  return bad;
}

/** "Nothing is drawn that does nothing" (AWS-07): every control Inbox draws is one this spec drives to its effect; a disabled one says why. */
const PROVEN = new Set(["inbox-view-needs", "inbox-view-updates", "inbox-view-all", "inbox-mark-all-read", "inbox-notification-settings", "attention-item", "inbox-approval-skip", "inbox-approval-allow", "inbox-approval-open", "inbox-proposal-keep", "inbox-proposal-switch", "inbox-update", "inbox-clear", "inbox-clear-cancel", "inbox-clear-confirm"]);
async function inertControls(page: Page): Promise<string[]> {
  const found = await page.getByTestId("inbox").evaluate((root) => {
    const out: { id: string; disabled: boolean; why: string }[] = [];
    for (const el of root.querySelectorAll<HTMLElement>("button, input, textarea, select, summary, a[href], [role='button'], [role='radio'], [tabindex]:not([tabindex='-1'])")) {
      const id = el.getAttribute("data-testid") ?? `${el.tagName.toLowerCase()}${el.className ? `.${String(el.className).split(" ")[0]}` : ""}`;
      const disabled = (el as HTMLButtonElement).disabled === true || el.getAttribute("aria-disabled") === "true";
      out.push({ id, disabled, why: el.getAttribute("title") ?? "" });
    }
    return out;
  });
  const bad: string[] = [];
  for (const c of found) {
    if (!PROVEN.has(c.id)) bad.push(`control ${c.id} is drawn and nothing proves what it does`);
    else if (c.disabled && !c.why.trim() && !["inbox-approval-skip", "inbox-approval-allow", "inbox-proposal-keep", "inbox-proposal-switch"].includes(c.id)) bad.push(`control ${c.id} is disabled and does not say why`);
  }
  return bad;
}

/** Notifications the OS is given are held, not shown, so the test can press them: the click handler is main's own. */
async function holdNotifications(app: ElectronApplication): Promise<void> {
  await app.evaluate(({ Notification }) => {
    process.env.MODBIT_SUPPRESS_OS_NOTIFICATIONS = "0";
    (Notification as unknown as { isSupported: () => boolean }).isSupported = () => true;
    const held: unknown[] = [];
    (globalThis as unknown as { __px206Held: unknown[] }).__px206Held = held;
    Notification.prototype.show = function (this: unknown) {
      held.push(this);
    };
  });
}
async function clickNotification(app: ElectronApplication, title: string): Promise<boolean> {
  return app.evaluate((_e, t) => {
    const held = (globalThis as unknown as { __px206Held?: { title: string; emit: (e: string) => void }[] }).__px206Held ?? [];
    const n = [...held].reverse().find((x) => x.title === t);
    if (!n) return false;
    n.emit("click");
    return true;
  }, title);
}

// ---------------------------------------------------------------- the qualification

test("PX-206 Inbox: Needs you decides approvals and proposals, a destructive effect opens its task; Updates are listed with their time and the Core's read state; the record survives a Core kill and a restart, keeps its cap and is cleared; counts are the Core's", async () => {
  test.setTimeout(600_000);
  const repo = makeRepo(tmp("repo"));
  // The destructive task works in a repository of its own: its untracked file would be a change no other task planned.
  const dirty = makeRepo(tmp("dirty"));
  writeFileSync(join(dirty, "scratch.tmp"), "untracked\n");
  const READY = "Review the notes file";
  const ADD = "Add the origin1 remote";
  const SKIP = "Add the origin2 remote";
  const CLEAN = "Clean the build output";
  const SWITCH = "Plan the parser change";
  const KEEP = "Plan the docs change";
  const SHOT = "Plan the screenshot change";
  const readyGate = new Gate();
  // Held to the end: the proposing tasks and the skipped one stay running, so no update arrives that the restart checks do not expect.
  const hold = new Gate();
  const plan = (outcome: string, files: string[] = []): Part => ({ call: { name: "plan.update", args: { outcome, expected_files: files } } });
  const propose = (reason: string): Part[][] => [["The plan is ready.", { call: { name: "task.propose_mode", args: { mode: "AGENT", reason } } }], [{ wait: hold }, "Waiting."]];
  const model = await routedModel({
    // A completion needs a plan and the repository's no-op check (makeRepo declares it); each task plans first.
    [READY]: [[plan("review the notes")], [{ wait: readyGate }, SAID, { call: { name: "fs.read", args: { path: "notes.txt" } } }], [{ call: { name: "task.complete", args: { summary: SUMMARY, self_review: { findings: [] } } } }]],
    [ADD]: [[plan("add the origin1 remote")], ["Adding the remote.", { call: { name: "shell.exec", args: { argv: ["git", "remote", "add", "origin1", URL1] } } }], [{ call: { name: "task.complete", args: { summary: "added", self_review: { findings: [] } } } }]],
    [SKIP]: [[plan("add the origin2 remote")], ["Adding the remote.", { call: { name: "shell.exec", args: { argv: ["git", "remote", "add", "origin2", URL2] } } }], [{ wait: hold }, "It was skipped."]],
    [CLEAN]: [[plan("clean the build output", ["scratch.tmp"])], ["Cleaning the build output.", { call: { name: "shell.exec", args: { argv: ["git", "clean", "-fd"] } } }], [{ call: { name: "task.complete", args: { summary: "cleaned", self_review: { findings: [] } } } }]],
    [SWITCH]: propose("the parser plan is ready to carry out"),
    [KEEP]: propose("the docs plan is ready to carry out"),
    [SHOT]: propose("the plan is ready"),
  });
  const dataDir = tmp("profile");
  // The proposing tasks hold their model call open after the proposal: the host gives enough model slots for all of them.
  const env = { MODBIT_OPENAI_BASE_URL: model.url, MODBIT_CAPACITY: "model=8,provider=8" };
  const record = join(dataDir, "inbox-updates.json");
  let { app, page } = await launch(dataDir, { env });
  const send = async (goal: string, plan = false, where = repo): Promise<string> => {
    await toNewTask(page);
    await page.getByTestId("goal").fill(goal);
    await page.getByTestId("workspace").fill(where);
    if (plan) {
      await page.getByTestId("home-mode-button").click();
      await page.getByTestId("menu-item-plan").click();
      await expect(page.getByTestId("home-mode")).toHaveAttribute("data-mode", "PLAN");
    }
    await page.getByTestId("run").click();
    return conversationTask(page);
  };
  try {
    await setContentSize(app, page, 1440, 900);
    await setTheme(page, "light");
    await holdNotifications(app);
    // The person opts in to completion notifications (off until then).
    await gearMenu(page, "settings.open");
    await page.getByTestId("notify-completion").check();
    await page.getByTestId("nav-back").click();

    // ---- Four tasks from home: one held before its first answer, three that come to wait on an approval.
    const ids: Record<string, string> = {};
    ids[READY] = await send(READY);
    for (const goal of [ADD, SKIP, CLEAN]) {
      ids[goal] = await send(goal, false, goal === CLEAN ? dirty : repo);
      await expect(page.locator(`[data-testid="approval-card"]`).first()).toBeVisible({ timeout: 60_000 });
    }
    const sid = await sessionOf(page);
    await expect.poll(() => page.evaluate((s) => window.modbit.dockApprovals(s).then((l) => l.length), sid), { timeout: 60_000 }).toBe(3);
    const listed = await page.evaluate((s) => window.modbit.dockApprovals(s), sid);
    const write = listed.find((a) => a.taskId === ids[ADD])!;
    const skip = listed.find((a) => a.taskId === ids[SKIP])!;
    const destructive = listed.find((a) => a.taskId === ids[CLEAN])!;
    expect(destructive.effectClass).toBe("Destructive");
    expect(write.effectClass).not.toMatch(/destructive|external/i);

    // ---- Inbox: Needs you lists the three approvals as cards, in words; the counts are the Core's.
    await showInbox(page);
    await expect(page.getByTestId("top-bar-title")).toHaveText("Inbox");
    const cards = page.getByTestId("inbox-approval");
    await expect(cards).toHaveCount(3, { timeout: 30_000 });
    await expect.poll(() => countDrift(page, sid), { timeout: 15_000 }).toEqual([]);
    const writeCard = page.locator(`[data-testid="inbox-approval"][data-approval-id="${write.approvalId}"]`);
    const skipCard = page.locator(`[data-testid="inbox-approval"][data-approval-id="${skip.approvalId}"]`);
    const destructiveCard = page.locator(`[data-testid="inbox-approval"][data-approval-id="${destructive.approvalId}"]`);
    await expect(writeCard).toHaveAttribute("data-intent-hash", write.intentHash);
    await expect(writeCard).toContainText("Run this command?");
    await expect(writeCard.getByTestId("attention-item")).toHaveText(ADD);
    await expect(writeCard.getByTestId("inbox-approval-command")).toHaveText(`git remote add origin1 ${URL1}`);
    await expect(destructiveCard.getByTestId("inbox-approval-command")).toHaveText("git clean -fd");
    await expect(destructiveCard.getByTestId("inbox-approval-tag")).toHaveText("Cannot be undone");
    await expect(destructiveCard.getByTestId("inbox-approval-why")).toContainText("It cannot be undone, so it is decided in its task");
    // Plain words: no tool name, effect class or Core command in Needs you.
    expect(await page.getByTestId("attention").innerText()).not.toMatch(/shell\.exec|Destructive|ProtectedWrite|EXTERNAL|ResolveApproval|StartTask|_/);
    expect(await destructiveDecidedFromInbox(page, sid, destructive.approvalId)).toEqual([]);

    // ---- The task held at its first answer reaches review while Inbox is open: an unread update with its time.
    readyGate.open();
    await expectTaskState(page, ids[READY]!, "ReadyForReview", 90_000);
    const readyRow = page.locator(`[data-testid="inbox-update"][data-task-id="${ids[READY]}"]`);
    await expect(readyRow).toHaveCount(1, { timeout: 30_000 });
    await expect(readyRow).toHaveAttribute("data-kind", "READY_FOR_REVIEW");
    await expect(readyRow).toHaveAttribute("data-read", "false");
    await expect(readyRow).toContainText("Ready to review");
    await expect(readyRow).toContainText(READY);
    await expect(readyRow.getByTestId("inbox-update-when")).toHaveText(/^(now|\d+m)$/);
    expect(await readyRow.getByTestId("inbox-update-when").getAttribute("title")).toMatch(/\d/);
    await expect(readyRow).toHaveAttribute("aria-label", /unread$/);
    expect(await readDrift(page, sid)).toEqual([]);
    expect(recordDrift(record, [SAID, SUMMARY])).toEqual([]);
    // The OS was given the completion notification (held here instead of shown).
    await expect.poll(() => page.evaluate(() => window.modbit.notificationLog().then((l) => l.map((n) => [n.id, n.shown]))), { timeout: 30_000 }).toContainEqual([`task:${ids[READY]}`, true]);

    // ---- Proposals: two Plan tasks suggest Agent. Inbox switches one and keeps the other, in place, within the Core's 15 s.
    if (SHOTS) {
      ids[SHOT] = await send(SHOT, true);
      await showInbox(page);
      await expect(page.locator(`[data-testid="inbox-proposal"][data-task-id="${ids[SHOT]}"]`)).toBeVisible({ timeout: 60_000 });
      await shots(page, "inbox-needs-you", async () => {
        await showInbox(page);
        await expect(cards).toHaveCount(3);
      });
    }
    ids[SWITCH] = await send(SWITCH, true);
    ids[KEEP] = await send(KEEP, true);
    await showInbox(page);
    const switchRow = page.locator(`[data-testid="inbox-proposal"][data-task-id="${ids[SWITCH]}"]`);
    const keepRow = page.locator(`[data-testid="inbox-proposal"][data-task-id="${ids[KEEP]}"]`);
    await expect(switchRow).toBeVisible({ timeout: 60_000 });
    await expect(keepRow).toBeVisible({ timeout: 60_000 });
    await expect(switchRow).toContainText("Switch to Agent mode?");
    await expect(switchRow.getByTestId("inbox-proposal-reason")).toHaveText("the parser plan is ready to carry out");
    await switchRow.getByTestId("inbox-proposal-switch").click();
    await expect(page.getByTestId("inbox-note")).toContainText(`Switched “${SWITCH}” to Agent.`);
    await keepRow.getByTestId("inbox-proposal-keep").click();
    await expect(page.getByTestId("inbox-note")).toContainText(`Kept “${KEEP}” in Plan.`);
    const outcome = async (t: string) => page.evaluate((x) => Promise.all([window.modbit.modeProposals(x), window.modbit.composerPosture(x)]).then(([l, p]) => [l.proposals.map((q) => q.outcomeReason).join(","), p.mode]), t);
    expect(await outcome(ids[SWITCH]!)).toEqual(["ACCEPTED_BY_USER", "AGENT"]);
    expect(await outcome(ids[KEEP]!)).toEqual(["DECLINED_BY_USER", "PLAN"]);
    await expect(switchRow).toHaveCount(0, { timeout: 15_000 });
    await expect(keepRow).toHaveCount(0, { timeout: 15_000 });

    // ---- Approvals: Skip and Allow once in place, bound to the hash shown; each effect is real (or really absent).
    await skipCard.getByTestId("inbox-approval-skip").click();
    await expect(page.getByTestId("inbox-note")).toContainText(`Skipped: “${SKIP}”`);
    await writeCard.getByTestId("inbox-approval-allow").click();
    await expect(page.getByTestId("inbox-note")).toContainText(`Allowed once: “${ADD}” runs it now.`);
    await expect.poll(() => page.evaluate((s) => window.modbit.dockApprovals(s).then((l) => l.map((a) => a.approvalId)), sid), { timeout: 30_000 }).toEqual([destructive.approvalId]);
    await expect.poll(() => git(repo, "remote").trim().split(/\s+/), { timeout: 60_000 }).toContain("origin1");
    expect(git(repo, "remote").trim().split(/\s+/)).not.toContain("origin2");
    await expect(cards).toHaveCount(1);
    await expect.poll(() => countDrift(page, sid), { timeout: 15_000 }).toEqual([]);

    // ---- The protected write's task reaches review while Inbox is open: a second unread update.
    await expectTaskState(page, ids[ADD]!, "ReadyForReview", 90_000);
    const addRow = page.locator(`[data-testid="inbox-update"][data-task-id="${ids[ADD]}"]`);
    await expect(addRow).toHaveAttribute("data-read", "false", { timeout: 30_000 });

    // ---- Opening an update marks it read through the Core and opens its task.
    const readyEntry = (await page.evaluate(() => window.modbit.inboxUpdates())).updates.find((u) => u.taskId === ids[READY])!;
    await readyRow.click();
    await expect(page.getByTestId("conversation")).toHaveAttribute("data-task-id", ids[READY]!);
    await expect.poll(() => page.evaluate(([s, t]) => window.modbit.agentHeaders(s!, true).then((h) => h.headers.find((x) => x.taskId === t)!.readOffset), [sid, ids[READY]]).then((r) => BigInt(r) >= BigInt(readyEntry.offset))).toBe(true);
    await showInbox(page);
    await expect(readyRow).toHaveAttribute("data-read", "true");
    await expect(addRow).toHaveAttribute("data-read", "false");
    expect(await readDrift(page, sid)).toEqual([]);
    // The notification's click handler (main's own) opens its task.
    expect(await clickNotification(app, READY)).toBe(true);
    await expect(page.getByTestId("conversation")).toHaveAttribute("data-task-id", ids[READY]!);
    await expect(page.getByTestId("inbox")).toHaveCount(0);
    await showInbox(page);
    await shots(page, "inbox-updates-read-unread", async () => {
      await showInbox(page);
      await expect(addRow).toHaveAttribute("data-read", "false");
    });

    // ---- The views and the page's actions.
    await page.getByTestId("inbox-view-needs").click();
    await expect(page.getByTestId("inbox-updates")).toHaveCount(0);
    await expect(destructiveCard).toBeVisible();
    await page.getByTestId("inbox-view-updates").click();
    await expect(page.getByTestId("attention")).toHaveCount(0);
    await expect(page.getByTestId("inbox-updates")).toBeVisible();
    await page.getByTestId("inbox-view-all").click();
    await expect(page.getByTestId("attention")).toBeVisible();
    await expect(page.getByTestId("inbox-view-all")).toHaveAttribute("aria-checked", "true");
    expect(await inertControls(page)).toEqual([]);
    await accessible(page, "Inbox with Needs you and Updates", '[data-testid="shell"]');
    await page.getByTestId("inbox-notification-settings").click();
    await expect(page.getByTestId("settings")).toBeVisible();
    await expect.poll(() => page.evaluate(() => document.activeElement?.closest('[data-testid="notification-preferences"]') !== null)).toBe(true);
    await showInbox(page);

    // ---- The destructive effect opens its task; there the card is the one that decides, and it asks twice.
    await shots(page, "inbox-destructive", async () => {
      await showInbox(page);
      await expect(destructiveCard).toBeVisible();
    });
    await destructiveCard.getByTestId("inbox-approval-open").click();
    await expect(page.getByTestId("conversation")).toHaveAttribute("data-task-id", ids[CLEAN]!);
    const taskCard = page.locator(`[data-testid="approval-card"][data-approval-id="${destructive.approvalId}"]`);
    await expect(taskCard).toBeVisible();
    await expect(taskCard).toHaveAttribute("data-effect-class", "Destructive");
    await taskCard.getByTestId("approval-run").click();
    await expect(page.getByTestId("approval-confirm")).toBeVisible();
    await shots(page, "inbox-destructive-opens-task", async () => {
      await expect(taskCard).toBeVisible();
      if ((await page.getByTestId("approval-confirm").count()) === 0) await taskCard.getByTestId("approval-run").click();
      await expect(page.getByTestId("approval-confirm")).toBeVisible();
    });
    if ((await page.getByTestId("approval-confirm").count()) === 1) await page.getByTestId("approval-back").click();
    expect(await destructiveDecidedFromInbox(page, sid, destructive.approvalId)).toEqual([]);
    expect(readFileSync(join(dirty, "scratch.tmp"), "utf8")).toBe("untracked\n");

    // ---- The record survives a kill of the real Core: same entries, same read states (the Core's markers are on its log).
    await showInbox(page);
    const before = (await page.evaluate(() => window.modbit.inboxUpdates())).updates.map((u) => [u.id, u.kind, u.taskId, u.read]);
    expect(before.length).toBe(2);
    const info = await page.evaluate(() => window.modbit.debugCoreInfo());
    process.kill(info!.pid, "SIGKILL");
    await expect(page.getByTestId("core-status")).not.toContainText(`pid ${info!.pid}`, { timeout: 90_000 });
    await expect(page.getByTestId("core-status")).toContainText("Core connected", { timeout: 90_000 });
    await expect.poll(() => page.evaluate(() => window.modbit.inboxUpdates().then((v) => v.updates.map((u) => [u.id, u.kind, u.taskId, u.read]))), { timeout: 30_000 }).toEqual(before);

    // ---- And a restart of the app: the record is read back from the data directory.
    await closeApp(app);
    ({ app, page } = await launch(dataDir, { env }));
    await setContentSize(app, page, 1440, 900);
    await showInbox(page);
    await expect(page.getByTestId("inbox-update")).toHaveCount(2, { timeout: 30_000 });
    await expect.poll(() => page.evaluate(() => window.modbit.inboxUpdates().then((v) => v.updates.map((u) => [u.id, u.kind, u.taskId, u.read]))), { timeout: 30_000 }).toEqual(before);
    expect(await readDrift(page, sid)).toEqual([]);
    expect(recordDrift(record, [SAID, SUMMARY])).toEqual([]);
    await expect(page.locator(`[data-testid="inbox-approval"][data-approval-id="${destructive.approvalId}"]`)).toBeVisible({ timeout: 30_000 });
    await shots(page, "inbox-after-restart", async () => {
      await showInbox(page);
      await expect(page.getByTestId("inbox-update")).toHaveCount(2);
    });

    // ---- Retained mutations.
    // An update that holds transcript text fails the record's shape.
    const onDisk = JSON.parse(readFileSync(record, "utf8")) as { entries: Record<string, unknown>[] };
    const mutated = join(tmp("mutated"), "inbox-updates.json");
    writeFileSync(mutated, JSON.stringify({ ...onDisk, entries: onDisk.entries.map((e, i) => (i === 0 ? { ...e, text: SAID } : e)) }));
    expect(recordDrift(mutated, [SAID, SUMMARY])).toEqual(["an update holds text beside its ids, title and time", `the record holds transcript text: "${SAID}"`]);
    // Inbox's count drawn from the renderer's own list (every Needs you entry) instead of the Core's attention count fails.
    await page.getByTestId("open-inbox-count").evaluate((el) => (el.textContent = String(document.querySelectorAll('[data-testid="attention-item"]').length + 1)));
    expect((await countDrift(page, sid)).join("")).toMatch(/Inbox's count is \d+; the Core's attention count is \d+/);
    // A control nobody proves fails the census.
    await page.getByTestId("inbox").evaluate((el) => {
      const b = document.createElement("button");
      b.type = "button";
      b.textContent = "Snooze";
      b.setAttribute("data-testid", "inbox-snooze");
      el.querySelector(".inbox-head")!.appendChild(b);
    });
    expect(await inertControls(page)).toEqual(["control inbox-snooze is drawn and nothing proves what it does"]);
    await page.getByTestId("inbox-snooze").evaluate((el) => el.remove());
    // A destructive effect decided while Inbox showed it (as a list that allowed it would): the check fails, and the Core ran it.
    await page.evaluate(([s, a, h]) => window.modbit.resolveApproval(s!, a!, true, "mutation: allowed from a list", h!), [sid, destructive.approvalId, destructive.intentHash]);
    expect(await destructiveDecidedFromInbox(page, sid, destructive.approvalId)).toContain("the destructive effect is no longer pending: it was decided while Inbox showed it");

    // ---- The cap: the record is filled past it on disk; read back it keeps the newest 200, and a new update still lands within it.
    await closeApp(app);
    const real = (JSON.parse(readFileSync(record, "utf8")) as { entries: { atMs: number }[] }).entries;
    const oldest = Math.min(...real.map((e) => e.atMs)) - 10_000_000;
    const filler = Array.from({ length: 205 }, (_, i) => ({ id: (i + 1).toString(16).padStart(32, "0"), kind: "FAILED", taskId: (0xf000 + i).toString(16).padStart(32, "0"), title: `Filler task ${i}`, atMs: oldest + i, offset: String(i + 1) }));
    writeFileSync(record, JSON.stringify({ ...(JSON.parse(readFileSync(record, "utf8")) as object), entries: [...filler, ...real] }));
    ({ app, page } = await launch(dataDir, { env }));
    await setContentSize(app, page, 1440, 900);
    await showInbox(page);
    await expect(page.getByTestId("inbox-view-updates")).toHaveText("Updates · 200", { timeout: 30_000 });
    for (const t of [ids[READY]!, ids[ADD]!]) await expect(page.locator(`[data-testid="inbox-update"][data-task-id="${t}"]`)).toHaveCount(1);
    // The destructive task, allowed by the mutation above, is resumed and reaches review: a new update at the cap.
    await page.evaluate(([s, t]) => window.modbit.startTask(s!, t!), [sid, ids[CLEAN]]);
    await expect(page.locator(`[data-testid="inbox-update"][data-task-id="${ids[CLEAN]}"]`)).toHaveCount(1, { timeout: 90_000 });
    await expect(page.getByTestId("inbox-view-updates")).toHaveText("Updates · 200");
    const capped = JSON.parse(readFileSync(record, "utf8")) as { entries: { id: string }[] };
    expect(capped.entries.length).toBe(200);
    expect(capped.entries.some((e) => e.id === filler[7]!.id)).toBe(false);
    expect(recordDrift(record, [SAID, SUMMARY])).toEqual([]);

    // ---- Mark all read moves the Core's markers; Clear asks once and empties the record.
    await expect(page.getByTestId("inbox-mark-all-read")).toBeEnabled();
    await page.getByTestId("inbox-mark-all-read").click();
    await expect(page.locator('[data-testid="inbox-update"][data-read="false"]')).toHaveCount(0, { timeout: 30_000 });
    expect(await readDrift(page, sid)).toEqual([]);
    await expect(page.getByTestId("inbox-mark-all-read")).toBeDisabled();
    expect(await page.getByTestId("inbox-mark-all-read").getAttribute("title")).toBe("Every update is read");
    await page.getByTestId("inbox-clear").click();
    await page.getByTestId("inbox-clear-cancel").click();
    await expect(page.getByTestId("inbox-update")).toHaveCount(200);
    await page.getByTestId("inbox-clear").click();
    await page.getByTestId("inbox-clear-confirm").click();
    await expect(page.getByTestId("inbox-update")).toHaveCount(0);
    await expect(page.getByTestId("inbox-updates-empty")).toBeVisible();
    expect((JSON.parse(readFileSync(record, "utf8")) as { entries: unknown[] }).entries).toEqual([]);
    await expect(agentRow(page, ids[CLEAN]!)).toBeVisible();
  } finally {
    readyGate.open();
    hold.open();
    await closeApp(app);
    model.server.closeAllConnections();
    model.server.close();
  }
});
