/**
 * QUAL-PX-204 / PX-E2E-204 (DR-PX-2026-11-11-021 MWS-11): split view on the real Electron build, a real Core and a scripted
 * model. Three tasks: one that finished, one that asks four times to run a protected command (adding a git remote that is a
 * real bare repository on this machine), and one that keeps running (its model call is held open).
 *
 * Alt+click on a row puts a task beside the one in the centre; each column has its own title bar, transcript, composer and
 * approval card, and the one with the keyboard shows it. Enter, Escape and the approve and deny keys pressed in the running
 * task's column decide nothing (the Core still lists the other task's approval as pending); pressed in the waiting task's
 * column they decide its approval (Enter on the card, then y, then x, each read back from the Core). Each composer queues to
 * its own task, read back from the Core's queue and transcript. A third task is refused with its reason (Alt+click and the
 * preview card), and a store that names three reads as none. The split (both tasks, the ratio and which has the keyboard)
 * survives a reload. Below the width two columns need, split view is refused with its reason and an open one is paused with
 * the focused task alone, and comes back when the window is wider. Close leaves split view with the other task. Axe light and
 * dark. Retained mutations, each asserted to fail: a key that decides the unfocused task's approval; a message that reaches
 * the other task; a third column drawn. With MODBIT_SHOTS_DIR set, the run writes the evidence screenshots.
 */
import { expect, test, type Locator, type Page } from "@playwright/test";
import { execFileSync } from "node:child_process";
import { mkdirSync, mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { Gate } from "./support/stream-model.ts";
import { routedModel, sessionOf, type ModelPart } from "./support/conversation-harness.ts";
import { coreQueue, userMessages } from "./support/composer-harness.ts";
import { agentRow, expectTaskState, openTask, runTask, setDensity } from "./support/new-task.ts";
import { accessible, closeApp, launch, makeRepo, setContentSize } from "./support/ui-harness.ts";

const SHOTS = process.env.MODBIT_SHOTS_DIR ?? "";
const GOAL_A = "Add the release remotes";
const GOAL_B = "Count the catalog entries";
const GOAL_C = "Note the build date";
const REMOTES = ["origin", "mirror", "backup", "spare"];
const FOR_A = "Only for the remotes task";
const FOR_B = "Only for the counting task";

async function setTheme(page: Page, theme: "light" | "dark"): Promise<void> {
  await page.evaluate((t) => document.documentElement.setAttribute("data-theme", t), theme);
  await page.evaluate(() => new Promise<void>((done) => requestAnimationFrame(() => requestAnimationFrame(() => done()))));
}

/** An evidence screenshot of the running app, light and dark (only when asked for). */
async function shots(page: Page, name: string): Promise<void> {
  if (!SHOTS) return;
  mkdirSync(SHOTS, { recursive: true });
  for (const theme of ["light", "dark"] as const) {
    await setTheme(page, theme);
    await page.waitForTimeout(400);
    await page.screenshot({ path: join(SHOTS, `${name}-${theme}.png`) });
  }
  await setTheme(page, "light");
}

interface Pending {
  approvalId: string;
  intentHash: string;
  taskId: string;
}
const pendingOf = (page: Page, sessionId: string): Promise<Pending[]> => page.evaluate((s) => window.modbit.pendingApprovals(s), sessionId) as Promise<Pending[]>;
const same = (a: string, b: string) => a.replace(/-/g, "").toLowerCase() === b.replace(/-/g, "").toLowerCase();

const column = (page: Page, taskId: string): Locator => page.locator(`[data-testid="split-column"][data-task-id="${taskId}"]`);

/**
 * What split view draws against its rules (empty when it holds): exactly two columns, each a different task with its own title
 * bar, conversation and composer, the conversation the column's task; exactly one with the keyboard, marked in the page and on
 * its conversation, and the other not.
 */
async function splitDrift(page: Page, tasks: readonly [string, string], focused: string): Promise<string[]> {
  return page.evaluate(
    ([t, f]) => {
      const bad: string[] = [];
      const cols = [...document.querySelectorAll<HTMLElement>('[data-testid="split-column"]')];
      if (cols.length !== 2) bad.push(`${cols.length} columns are drawn`);
      const ids = cols.map((c) => c.getAttribute("data-task-id"));
      if (new Set(ids).size !== ids.length) bad.push("a task is in two columns");
      for (const id of t!) if (!ids.includes(id)) bad.push(`${id.slice(0, 8)} has no column`);
      for (const c of cols) {
        const id = c.getAttribute("data-task-id")!;
        const convs = c.querySelectorAll('[data-testid="conversation"]');
        if (c.querySelectorAll('[data-testid="top-bar"]').length !== 1) bad.push(`${id.slice(0, 8)}: not one title bar`);
        if (convs.length !== 1 || convs[0]!.getAttribute("data-task-id") !== id) bad.push(`${id.slice(0, 8)}: its conversation is not its own`);
        if (c.querySelectorAll('[data-testid="composer-region"]').length !== 1) bad.push(`${id.slice(0, 8)}: not one composer`);
        const has = c.getAttribute("data-focused") === "true";
        const keys = convs[0]?.getAttribute("data-key-focus") === "true";
        if (has !== (id === f)) bad.push(`${id.slice(0, 8)} ${has ? "shows" : "does not show"} the keyboard`);
        if (keys !== (id === f)) bad.push(`${id.slice(0, 8)} ${keys ? "takes" : "does not take"} the keys`);
      }
      if (document.querySelectorAll('[data-testid="conversation"][data-key-focus="true"]').length !== 1) bad.push("not exactly one conversation takes the keys");
      return bad;
    },
    [tasks, focused] as const,
  );
}

/** Why a message typed in one task's composer is not on that task alone (empty when it is): read from the Core's queue and transcript of both. */
async function misrouted(page: Page, typedIn: string, other: string, text: string): Promise<string[]> {
  const on = async (t: string) => [...(await coreQueue(page, t)).map((q) => q.text), ...(await userMessages(page, t))];
  const bad: string[] = [];
  if (!(await on(typedIn)).includes(text)) bad.push(`“${text}” is not on the task it was typed for`);
  if ((await on(other)).includes(text)) bad.push(`“${text}” reached the other task`);
  return bad;
}

/** Why the unfocused task's approval is no longer the Core's to ask (empty while it still is). */
async function unfocusedDecided(page: Page, sessionId: string, approvalId: string): Promise<string[]> {
  return (await pendingOf(page, sessionId)).some((p) => same(p.approvalId, approvalId)) ? [] : ["the unfocused task's approval was decided"];
}

/** The approval the Core lists for `taskId` and its card in the task's column, once there. */
async function waitingApproval(page: Page, sessionId: string, taskId: string, notId?: string): Promise<Pending> {
  let found: Pending | undefined;
  await expect.poll(async () => {
    found = (await pendingOf(page, sessionId)).find((p) => same(p.taskId, taskId) && (!notId || !same(p.approvalId, notId)));
    return !!found;
  }, { timeout: 90_000, message: `an approval for ${taskId.slice(0, 8)}` }).toBe(true);
  return found!;
}

/** Puts the keyboard in a column's transcript (a region that is not a control and not a text field). */
async function focusTranscript(page: Page, taskId: string): Promise<void> {
  const scroll = column(page, taskId).getByTestId("conv-scroll");
  await scroll.click({ position: { x: 12, y: 12 } });
  await expect(column(page, taskId)).toHaveAttribute("data-focused", "true");
  await expect.poll(() => page.evaluate(() => document.activeElement?.getAttribute("data-testid") ?? "")).toBe("conv-scroll");
}

/** Answers the confirmation an irreversible effect asks for, wherever it stands (the card's second step or the keyboard's dialog), with Enter. */
async function confirmIfAsked(page: Page, card: Locator): Promise<void> {
  await page.waitForTimeout(300);
  if ((await card.getByTestId("approval-confirm").isVisible().catch(() => false)) || (await page.getByTestId("confirm").isVisible().catch(() => false))) await page.keyboard.press("Enter");
}

test("PX-204: two tasks side by side; keys decide only in the focused task; each composer queues to its own task; a third is refused; a reload restores it; narrow windows refuse or pause it", async ({}, testInfo) => {
  test.setTimeout(600_000);
  void testInfo;
  const repo = mkdtempSync(join(tmpdir(), "modbit-px204-repo-"));
  makeRepo(repo);
  const bare = mkdtempSync(join(tmpdir(), "modbit-px204-remote-"));
  execFileSync("git", ["init", "--bare", "-q", bare]);
  // Each task's model answers by its goal. B's first call and C's are held open, so both stay Running; A asks four times and
  // is then held too (nothing in this spec needs a task to finish, and a finished task would open its Review by itself).
  const hold = new Gate();
  const model = await routedModel(async (goal, results): Promise<ModelPart[]> => {
    if (goal === GOAL_A && results < REMOTES.length) return [`Adding ${REMOTES[results]}.`, { call: { name: "shell.exec", args: { argv: ["git", "remote", "add", REMOTES[results]!, bare] } } }];
    if (goal === GOAL_A || goal === GOAL_B || goal === GOAL_C) await hold.opened;
    return ["Nothing further."];
  });
  const { app, page } = await launch(mkdtempSync(join(tmpdir(), "modbit-e2e-px204-")), { env: { MODBIT_OPENAI_BASE_URL: model.url } });
  try {
    await setContentSize(app, page, 1440, 900);
    await setTheme(page, "light");
    // ---- Three tasks: C finishes, A waits on its first approval, B keeps running.
    const c = await runTask(page, GOAL_C, repo);
    await expectTaskState(page, c, "Running");
    const a = await runTask(page, GOAL_A, repo);
    const sessionId = await sessionOf(page);
    const first = await waitingApproval(page, sessionId, a);
    await expectTaskState(page, a, "Waiting");
    const b = await runTask(page, GOAL_B, repo);
    await expectTaskState(page, b, "Running");
    await expect(page.getByTestId("split-view")).toHaveCount(0);

    // ---- Alt+click on A's row: A beside B, with the keyboard.
    await agentRow(page, a).getByTestId("agent-open").click({ modifiers: ["Alt"] });
    await expect(page.getByTestId("split-view")).toBeVisible();
    await expect(page.getByTestId("shell")).toHaveAttribute("data-split", "shown");
    await expect(page.getByTestId("split-column")).toHaveCount(2);
    await expect(page.getByTestId("split-column").nth(0)).toHaveAttribute("data-task-id", b);
    await expect(page.getByTestId("split-column").nth(1)).toHaveAttribute("data-task-id", a);
    expect(await splitDrift(page, [b, a], a)).toEqual([]);
    // Each column has its own title bar with its task's title and Close; the shell's own title bar is not drawn.
    await expect(column(page, a).getByTestId("top-bar-title")).toHaveText(GOAL_A);
    await expect(column(page, b).getByTestId("top-bar-title")).toHaveText(GOAL_B);
    await expect(column(page, a).getByTestId("split-close")).toHaveAccessibleName("Close: leave split view");
    await expect(page.locator('[data-testid="top-bar"]')).toHaveCount(2);
    // A's approval card is in A's column only; B's column docks no card for it.
    await expect(column(page, a).getByTestId("approval-card")).toHaveAttribute("data-approval-id", first.approvalId, { timeout: 30_000 });
    await expect(column(page, b).getByTestId("approval-card")).toHaveCount(0);
    for (const id of [a, b]) await expect(column(page, id).getByTestId("conversation")).toHaveAttribute("data-loaded", "true", { timeout: 30_000 });
    // The columns share the task area evenly (board C2) and no details rail stands beside them.
    const widths = async () => page.getByTestId("split-column").evaluateAll((els) => els.map((e) => Math.round(e.getBoundingClientRect().width)));
    expect(await widths()).toEqual([576, 576]);
    await expect(page.getByTestId("details-rail")).toHaveCount(0);
    // Both rows are marked open in the list.
    for (const id of [a, b]) await expect(agentRow(page, id)).toHaveAttribute("data-in-split", "true");
    // Focus is visible: an accent rule along the focused column's top and its name says so; the other has neither.
    const rule = (id: string) => column(page, id).evaluate((e) => getComputedStyle(e).boxShadow);
    expect(await rule(a)).toMatch(/inset/);
    expect(await rule(b)).toBe("none");
    await expect(column(page, a)).toHaveAttribute("aria-label", `${GOAL_A}, has the keyboard`);
    await expect(column(page, b)).toHaveAttribute("aria-label", GOAL_B);

    // ---- A third task is refused with its reason: Alt+click and the preview card.
    await agentRow(page, c).getByTestId("agent-open").click({ modifiers: ["Alt"] });
    await expect(page.getByTestId("shell-notice")).toContainText("Split view holds two tasks");
    await expect(page.getByTestId("split-column")).toHaveCount(2);
    await agentRow(page, c).getByTestId("agent-open").focus();
    await page.keyboard.press("Space");
    await expect(page.getByTestId("agent-preview")).toBeVisible();
    await page.getByTestId("agent-preview-split").click();
    await expect(page.getByTestId("shell-notice")).toContainText("Split view holds two tasks");
    expect(await splitDrift(page, [b, a], a), "the third task was not accepted").toEqual([]);
    const stored = () => page.evaluate(() => (JSON.parse(localStorage.getItem("modbit.ui.v1") ?? "{}") as { split?: { tasks: string[]; ratio: number; focus: number } | null }).split ?? null);
    expect((await stored())?.tasks).toEqual([b, a]);
    // The retained mutation: a third column drawn fails the check.
    await page.evaluate(() => {
      const extra = document.querySelector('[data-testid="split-column"]')!.cloneNode(false) as HTMLElement;
      extra.id = "px204-third";
      extra.setAttribute("data-task-id", "third");
      document.querySelector('[data-testid="split-view"]')!.appendChild(extra);
    });
    const third = await splitDrift(page, [b, a], a);
    expect(third, "a third column fails").toContain("3 columns are drawn");
    await page.evaluate(() => document.getElementById("px204-third")?.remove());
    expect(await splitDrift(page, [b, a], a)).toEqual([]);

    // ---- Each composer queues to its own task.
    await column(page, a).getByTestId("composer-input").fill(FOR_A);
    await column(page, a).getByTestId("composer-input").press("Enter");
    await expect(column(page, a)).toHaveAttribute("data-focused", "true");
    await column(page, b).getByTestId("composer-input").fill(FOR_B);
    await expect(column(page, b), "typing in a column gives it the keyboard").toHaveAttribute("data-focused", "true");
    await column(page, b).getByTestId("composer-input").press("Enter");
    await expect.poll(() => misrouted(page, a, b, FOR_A), { timeout: 30_000 }).toEqual([]);
    await expect.poll(() => misrouted(page, b, a, FOR_B), { timeout: 30_000 }).toEqual([]);
    // Each queue docks above its own composer.
    await expect(column(page, b).locator('[data-tray-id="composer-queue"]')).toContainText(FOR_B, { timeout: 30_000 });
    await expect(column(page, b).locator('[data-tray-id="composer-queue"]')).not.toContainText(FOR_A);
    // The retained mutation: a message that reaches the other task fails the check.
    const stray = "Typed in the counting task's box";
    const strayId = await page.evaluate(([s, t, text]) => window.modbit.composerQueueInput(s!, t!, text!, "DEFAULT", `px204-stray-${Date.now()}`).then((r) => r.inputId), [sessionId, a, stray] as const);
    expect(await misrouted(page, b, a, stray), "a message on the other task fails").toEqual([`“${stray}” is not on the task it was typed for`, `“${stray}” reached the other task`]);
    await page.evaluate(([s, t, id]) => window.modbit.composerRemoveQueued(s!, t!, id!), [sessionId, a, strayId] as const);
    await expect.poll(async () => (await coreQueue(page, a)).map((q) => q.text)).not.toContain(stray);

    // ---- The evidence: two tasks with one waiting on approval, focus visible, each composer with its queued message.
    await focusTranscript(page, a);
    await expect(column(page, a).locator('[data-tray-id="composer-queue"], [data-tray-id="approvals"]').first()).toBeVisible();
    // The refusal notice has gone (it stays 8 s), so the screenshot shows the two tasks as they are.
    await expect(page.getByTestId("shell-notice")).toHaveText("", { timeout: 15_000 });
    await shots(page, "split-comfortable");
    await accessible(page, "split view, light");
    await setTheme(page, "dark");
    await accessible(page, "split view, dark");
    await setTheme(page, "light");
    await setDensity(page, "compact");
    await expect(page.getByTestId("split-view")).toBeVisible();
    await expect(column(page, a).getByTestId("approval-card")).toBeVisible({ timeout: 30_000 });
    await focusTranscript(page, a);
    for (const id of [a, b]) await expect(column(page, id).getByTestId("conversation")).toHaveAttribute("data-loaded", "true", { timeout: 30_000 });
    await shots(page, "split-compact");
    await accessible(page, "split view, compact");
    await setDensity(page, "comfortable");
    await expect(page.getByTestId("split-view")).toBeVisible();

    // ---- Keys pressed in B's column decide nothing of A's: Enter, Escape, y and x, in B's transcript and with nothing focused.
    await expect(column(page, a).getByTestId("approval-card")).toHaveAttribute("data-approval-id", first.approvalId, { timeout: 30_000 });
    await focusTranscript(page, b);
    for (const key of ["Enter", "Escape", "y", "x"]) {
      await focusTranscript(page, b);
      await page.keyboard.press(key);
      await page.waitForTimeout(400);
      expect(await unfocusedDecided(page, sessionId, first.approvalId), `${key} in the running task's column`).toEqual([]);
    }
    await page.evaluate(() => (document.activeElement as HTMLElement | null)?.blur());
    for (const key of ["y", "x", "Enter"]) {
      await page.keyboard.press(key);
      await page.waitForTimeout(400);
      expect(await unfocusedDecided(page, sessionId, first.approvalId), `${key} with nothing focused, B has the keyboard`).toEqual([]);
    }
    await expect(column(page, a).getByTestId("approval-card")).toHaveAttribute("data-approval-id", first.approvalId);
    expect(await splitDrift(page, [b, a], b)).toEqual([]);

    // ---- In A's column, Enter decides A's approval on its card: the Core runs it and asks for the next remote.
    await focusTranscript(page, a);
    await page.keyboard.press("Enter");
    await confirmIfAsked(page, column(page, a).getByTestId("approval-card"));
    await expect.poll(async () => (await pendingOf(page, sessionId)).some((p) => same(p.approvalId, first.approvalId)), { timeout: 30_000 }).toBe(false);
    const second = await waitingApproval(page, sessionId, a, first.approvalId);
    await expect(column(page, a).getByTestId("approval-card")).toHaveAttribute("data-approval-id", second.approvalId, { timeout: 30_000 });
    // y in B's column decides nothing; y in A's approves A's.
    await focusTranscript(page, b);
    await page.keyboard.press("y");
    await page.waitForTimeout(400);
    expect(await unfocusedDecided(page, sessionId, second.approvalId)).toEqual([]);
    await focusTranscript(page, a);
    await page.keyboard.press("y");
    await confirmIfAsked(page, column(page, a).getByTestId("approval-card"));
    await expect.poll(async () => (await pendingOf(page, sessionId)).some((p) => same(p.approvalId, second.approvalId)), { timeout: 30_000 }).toBe(false);
    const thirdAsk = await waitingApproval(page, sessionId, a, second.approvalId);
    // x in B's column decides nothing; x in A's denies A's.
    await focusTranscript(page, b);
    await page.keyboard.press("x");
    await page.waitForTimeout(400);
    expect(await unfocusedDecided(page, sessionId, thirdAsk.approvalId)).toEqual([]);
    await focusTranscript(page, a);
    await page.keyboard.press("x");
    await expect.poll(async () => (await pendingOf(page, sessionId)).some((p) => same(p.approvalId, thirdAsk.approvalId)), { timeout: 30_000 }).toBe(false);
    const fourth = await waitingApproval(page, sessionId, a, thirdAsk.approvalId);
    await expect(column(page, a).getByTestId("approval-card")).toHaveAttribute("data-approval-id", fourth.approvalId, { timeout: 30_000 });
    // What the Core did: origin and mirror added (approved), backup not (denied).
    const remotes = () => execFileSync("git", ["-C", repo, "remote"], { encoding: "utf8" }).split("\n").filter(Boolean).sort();
    await expect.poll(remotes, { timeout: 30_000 }).toEqual(["mirror", "origin"]);
    await expectTaskState(page, b, "Running");

    // ---- A reload restores the split: the same two tasks in the same columns, the same ratio and the same focus.
    await focusTranscript(page, b);
    await page.getByTestId("splitter-split").focus();
    await page.keyboard.press("ArrowRight");
    await page.keyboard.press("ArrowRight");
    await expect.poll(widths).toEqual([608, 544]);
    await focusTranscript(page, b);
    const before = await stored();
    expect(before?.tasks).toEqual([b, a]);
    expect(before?.focus).toBe(0);
    await page.reload();
    await expect(page.getByTestId("core-status")).toContainText("Core connected", { timeout: 60_000 });
    await expect(page.getByTestId("split-view")).toBeVisible({ timeout: 30_000 });
    await expect(page.getByTestId("split-column").nth(0)).toHaveAttribute("data-task-id", b);
    await expect(page.getByTestId("split-column").nth(1)).toHaveAttribute("data-task-id", a);
    await expect.poll(widths).toEqual([608, 544]);
    expect(await splitDrift(page, [b, a], b)).toEqual([]);
    await expect(column(page, a).getByTestId("approval-card")).toHaveAttribute("data-approval-id", fourth.approvalId, { timeout: 30_000 });
    // The keys still go to the focused task after the reload: Enter in B decides nothing of A's.
    await focusTranscript(page, b);
    await page.keyboard.press("Enter");
    await page.waitForTimeout(400);
    expect(await unfocusedDecided(page, sessionId, fourth.approvalId)).toEqual([]);

    // ---- The retained mutation: the keyboard's owner moved to A while B's transcript has focus: Enter decides A's approval, and the check fails.
    await page.evaluate(([fb, fa]) => {
      document.querySelector(`[data-testid="conversation"][data-task-id="${fb}"]`)!.setAttribute("data-key-focus", "false");
      document.querySelector(`[data-testid="conversation"][data-task-id="${fa}"]`)!.setAttribute("data-key-focus", "true");
    }, [b, a] as const);
    await page.keyboard.press("Enter");
    await confirmIfAsked(page, column(page, a).getByTestId("approval-card"));
    await expect.poll(() => unfocusedDecided(page, sessionId, fourth.approvalId), { timeout: 30_000, message: "a key routed to the unfocused task fails the check" }).toEqual(["the unfocused task's approval was decided"]);
    await focusTranscript(page, a);
    await focusTranscript(page, b);
    expect(await splitDrift(page, [b, a], b)).toEqual([]);

    // ---- A store that names three tasks reads as none: nothing of it is drawn after a reload.
    await page.evaluate(([x, y, z]) => {
      const p = JSON.parse(localStorage.getItem("modbit.ui.v1") ?? "{}") as Record<string, unknown>;
      localStorage.setItem("modbit.ui.v1", JSON.stringify({ ...p, split: { tasks: [x, y, z], ratio: 0.5, focus: 0 } }));
    }, [b, a, c] as const);
    await page.reload();
    await expect(page.getByTestId("core-status")).toContainText("Core connected", { timeout: 60_000 });
    await expect(page.getByTestId("goal")).toBeVisible({ timeout: 30_000 });
    await expect(page.getByTestId("split-column")).toHaveCount(0);
    await expect(page.getByTestId("shell")).toHaveAttribute("data-split", "none");

    // ---- The preview card opens split view: Space on A's row, then Open in split view.
    await openTask(page, b);
    await agentRow(page, a).getByTestId("agent-open").focus();
    await page.keyboard.press("Space");
    await expect(page.getByTestId("agent-preview")).toHaveAttribute("data-task-id", a);
    await page.getByTestId("agent-preview-split").click();
    await expect(page.getByTestId("split-view")).toBeVisible();
    expect(await splitDrift(page, [b, a], a)).toEqual([]);

    // ---- Narrow: below 848 of task area an open split pauses with the focused task alone, and comes back when it is wider.
    await setContentSize(app, page, 1100, 800);
    await expect(page.getByTestId("shell")).toHaveAttribute("data-split", "paused");
    await expect(page.getByTestId("split-column")).toHaveCount(0);
    await expect(page.getByTestId("conversation")).toHaveAttribute("data-task-id", a);
    await expect(page.getByTestId("split-paused")).toContainText("Split view is paused: two tasks need 848 px beside the task list and this window gives 812.");
    await expect(page.getByTestId("split-paused")).toContainText(GOAL_B);
    await expect(page.getByTestId("conversation")).toHaveAttribute("data-loaded", "true", { timeout: 30_000 });
    await shots(page, "split-paused-narrow");
    await setContentSize(app, page, 1440, 900);
    await expect(page.getByTestId("split-view")).toBeVisible();
    expect(await splitDrift(page, [b, a], a)).toEqual([]);
    // Below it, split view is refused with its reason.
    await setContentSize(app, page, 1100, 800);
    await page.getByTestId("split-close").click();
    await expect(page.getByTestId("shell")).toHaveAttribute("data-split", "none");
    await expect(page.getByTestId("conversation")).toHaveAttribute("data-task-id", a);
    await agentRow(page, b).getByTestId("agent-open").click({ modifiers: ["Alt"] });
    await expect(page.getByTestId("shell-notice")).toHaveText("Split view needs 848 px beside the task list for two tasks; this window gives 812. Widen the window or hide the task list.");
    await expect(page.getByTestId("split-column")).toHaveCount(0);
    await expect(page.getByTestId("conversation")).toHaveAttribute("data-loaded", "true", { timeout: 30_000 });
    await shots(page, "split-refused-narrow");
    await setContentSize(app, page, 1440, 900);

    // ---- Close leaves split view with the other task in the centre.
    await agentRow(page, b).getByTestId("agent-open").click({ modifiers: ["Alt"] });
    await expect(page.getByTestId("split-view")).toBeVisible();
    await column(page, b).getByTestId("split-close").click();
    await expect(page.getByTestId("split-view")).toHaveCount(0);
    await expect(page.getByTestId("conversation")).toHaveAttribute("data-task-id", a);
    await expect(page.getByTestId("top-bar")).toHaveCount(1);
    expect(await stored()).toBeNull();
  } finally {
    hold.open();
    await closeApp(app);
    model.server.close();
  }
});
