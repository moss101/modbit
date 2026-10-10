/**
 * REQ-PX-060 (QUAL-PX-060): the context ring, the usage summary and the docked
 * usage, limit, offline and policy trays, against a real Core and a scripted
 * OpenAI-compatible model that reports the token counts the test chooses. The
 * Core's accounting, budgets and typed stops are real; only the provider is
 * scripted. No test waits on a clock for a state: a Gate holds the model where
 * a test needs the run to stand still.
 */
import { expect, test, type Page } from "@playwright/test";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { accessible, closeApp, launch, makeRepo, setContentSize } from "./support/ui-harness.ts";
import { Gate } from "./support/stream-model.ts";
import { createTask, openConversation, routedModel, sessionOf, startAndOpen, type ModelPart } from "./support/conversation-harness.ts";

const readNotes: ModelPart = { call: { name: "fs.read", args: { path: "notes.txt" } } };
const plan: ModelPart = { call: { name: "plan.update", args: { outcome: "read the notes", expected_files: [] } } };
const complete: ModelPart = { call: { name: "task.complete", args: { summary: "done", self_review: { findings: [] } } } };
const newRepo = () => makeRepo(mkdtempSync(join(tmpdir(), "modbit-ctx-repo-")));

/** The numbers the open breakdown shows: its categories and its total, as the Core counted them. */
async function breakdown(page: Page): Promise<{ total: bigint; sum: bigint; rows: number; sums: string | null; detail: string }> {
  // One synchronous pass over the DOM: the total and the categories must come from the same render. Read apart, a refresh
  // of the tray between the two calls pairs a total with another snapshot's categories (CI, Windows: sum 7861 against total 7747).
  const snap = await page.evaluate(() => {
    const tray = document.querySelector('[data-testid="context-tray"]');
    const cats = [...document.querySelectorAll('[data-testid="context-category"]')];
    return {
      total: tray?.getAttribute("data-total") ?? "0",
      sums: tray?.getAttribute("data-sums") ?? null,
      cats: cats.map((e) => `${e.textContent?.trim().replace(/\s+/g, " ").slice(0, 40)}=${e.getAttribute("data-tokens") ?? "0"}`),
    };
  });
  const tokens = snap.cats.map((c) => c.slice(c.lastIndexOf("=") + 1));
  return {
    total: BigInt(snap.total),
    sum: tokens.reduce((s, t) => s + BigInt(t), 0n),
    rows: tokens.length,
    sums: snap.sums,
    detail: `total ${snap.total}, core says sums=${snap.sums}: ${snap.cats.join("; ")}`,
  };
}

test("PX-060: the ring shows the Core's usage; its tray lists the nine categories that sum to the total; it updates after a turn; the usage summary and the budget caps appear as set", async () => {
  test.setTimeout(240_000);
  const repo = newRepo();
  const reached = new Gate();
  const release = new Gate();
  let held = false;
  const model = await routedModel(
    async (_goal, results): Promise<ModelPart[]> => {
      if (results === 0) return ["Planning.", plan];
      if (results === 1) {
        if (!held) {
          held = true;
          reached.open();
          await release.opened;
        }
        return ["Reading.", readNotes];
      }
      return ["Finished.", complete];
    },
    // The provider reports 40,000 input tokens for the first request and 340,000 after (the model window is 400,000).
    (results) => (results === 0 ? 40_000 : 340_000),
  );
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-ctx-"));
  const { app, page } = await launch(dataDir, { env: { MODBIT_OPENAI_BASE_URL: model.url } });
  try {
    await setContentSize(app, page, 1280, 800);
    const taskId = await startAndOpen(page, "read the notes", repo);
    const sid = await sessionOf(page);
    await reached.opened;
    // Caps set through the Core appear beside the context numbers.
    await page.evaluate(([s, t]) => window.modbit.setTaskBudgets(s!, t!, { maxCostMinor: "1000", maxWallMs: "3600000" }), [sid, taskId]);

    // The ring is in the status row; its tray lists the categories, summing to the total.
    const ring = page.getByTestId("context-ring");
    await expect(ring).toBeVisible();
    await expect(ring).toHaveAttribute("data-total", /^\d+$/, { timeout: 30_000 });
    await ring.click();
    await expect(page.getByTestId("context-tray")).toBeVisible();
    const first = await breakdown(page);
    expect(first.rows, "the Core's nine categories").toBe(9);
    expect(first.sum, `the categories sum to the total: ${first.detail}`).toBe(first.total);
    expect(first.sums).toBe("true");
    expect(first.total).toBeGreaterThan(0n);
    await expect(page.getByTestId("context-source")).toContainText("estimate");
    await expect(page.getByTestId("context-budget").filter({ hasText: "Time cap" })).toBeVisible({ timeout: 30_000 });
    await expect(page.getByTestId("context-budget").filter({ hasText: "Cost cap" })).toBeVisible();
    // Not near a limit and set to auto: no usage summary.
    await expect(page.getByTestId("usage-summary")).toHaveCount(0);
    await accessible(page, "the context breakdown tray");
    // The person's choice: always show it.
    await page.getByTestId("usage-setting").selectOption("always");
    await expect(page.getByTestId("usage-summary")).toBeVisible();
    await expect(page.getByTestId("usage-cost")).toHaveText(/^(\$\d|not priced)/);
    await page.getByTestId("usage-setting").selectOption("auto");
    await expect(page.getByTestId("usage-summary")).toHaveCount(0);

    // The run goes on; after the turns the Core reports 340,000 of the 400,000-token window: the ring and the tray follow.
    release.open();
    await expect(page.getByTestId("review")).toBeVisible({ timeout: 120_000 });
    await page.keyboard.press("Escape");
    await expect(ring).toHaveAttribute("data-percent", "85", { timeout: 30_000 });
    await expect(ring).toHaveAttribute("data-level", "warn");
    await expect(ring).toHaveAttribute("data-total", "340000");
    await expect(page.getByTestId("context-ring-text")).toHaveText("85%");
    const second = await breakdown(page);
    expect(second.total).toBe(340_000n);
    expect(second.sum, second.detail).toBe(second.total);
    expect(second.rows).toBe(9);
    expect(second.total).not.toBe(first.total);
    await expect(page.getByTestId("context-source")).toContainText("provider reported");
    // Near a limit and set to auto: the usage summary shows by itself.
    await expect(page.getByTestId("usage-summary")).toBeVisible();
    await expect(page.getByTestId("usage-setting")).toHaveValue("auto");
    await accessible(page, "the context breakdown near the limit, with the usage summary");
    // The ring is a button: Escape from the tray's own Dismiss closes the tray.
    await page.getByTestId("tray-dismiss").click();
    await expect(page.getByTestId("context-tray")).toHaveCount(0);
    await expect(ring).toHaveAttribute("aria-expanded", "false");
  } finally {
    release.open();
    await closeApp(app);
    model.server.close();
  }
});

test("PX-060: BUDGET_EXHAUSTED docks a limit tray naming what ran out; raising the cap resumes the run with its work kept", async () => {
  test.setTimeout(240_000);
  const repo = newRepo();
  const release = new Gate();
  const model = await routedModel(async (_goal, results): Promise<ModelPart[]> => {
    if (results === 0) {
      await release.opened;
      return ["Planning.", plan];
    }
    if (results === 1) return ["Reading.", readNotes];
    return ["Finished.", complete];
  });
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-budget-"));
  const { app, page } = await launch(dataDir, { env: { MODBIT_OPENAI_BASE_URL: model.url } });
  try {
    await setContentSize(app, page, 1280, 800);
    // One millisecond of wall clock for the whole task, set before it starts (a run reads its budgets when it begins): the next turn boundary finds it spent.
    const taskId = await createTask(page, "read the notes twice", repo);
    const sid = await sessionOf(page);
    await page.evaluate(([s, t]) => window.modbit.setTaskBudgets(s!, t!, { maxWallMs: "1" }), [sid, taskId]);
    await page.getByTestId("task-card").filter({ hasText: "read the notes twice" }).getByTestId("task-start").click();
    await openConversation(page, taskId);
    release.open();
    const tray = page.getByTestId("tray-budget");
    await expect(tray).toBeVisible({ timeout: 120_000 });
    await expect(page.getByTestId("tray-budget-text")).toContainText("max_wall_ms");
    await expect(page.getByTestId("tray-budget-bar").first()).toContainText("Time cap");
    await accessible(page, "the budget-exhausted tray");
    // A cap that is not a whole number is refused where it is typed.
    await page.getByTestId("tray-budget-wall").fill("ten");
    await page.getByTestId("tray-budget-raise").click();
    await expect(page.getByTestId("tray-budget-note")).toContainText("whole number");
    // Raise it: the Core records the new cap and the run resumes; the tray goes away with its cause.
    await page.getByTestId("tray-budget-wall").fill("3600");
    await page.getByTestId("tray-budget-raise").click();
    await expect(page.getByTestId("review")).toBeVisible({ timeout: 120_000 });
    await page.keyboard.press("Escape");
    await expect(page.getByTestId("tray-budget")).toHaveCount(0);
    const acct = await page.evaluate((t) => window.modbit.contextAccounting(t), taskId);
    expect(acct.budgets?.maxWallMs).toBe("3600000");
  } finally {
    release.open();
    await closeApp(app);
    model.server.close();
  }
});

test("PX-060: a Core that goes away docks an offline tray while it is gone, keeps the conversation readable, and takes the tray away when it is back", async () => {
  test.setTimeout(240_000);
  const repo = newRepo();
  const model = await routedModel(async (): Promise<ModelPart[]> => ["Hello."]);
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-offline-"));
  const { app, page } = await launch(dataDir, { env: { MODBIT_OPENAI_BASE_URL: model.url } });
  try {
    await setContentSize(app, page, 1280, 800);
    await startAndOpen(page, "say hello", repo);
    await expect(page.getByTestId("conv-assistant").first()).toBeVisible({ timeout: 60_000 });
    // Record every tray that ever shows: the outage may be shorter than a poll.
    await page.evaluate(() => {
      const w = window as unknown as { __trays: string[] };
      w.__trays = [];
      const note = () => {
        for (const el of document.querySelectorAll("[data-tray-id]")) {
          const id = el.getAttribute("data-tray-id")!;
          if (!w.__trays.includes(id)) w.__trays.push(id);
        }
      };
      new MutationObserver(note).observe(document.body, { childList: true, subtree: true });
    });
    const info = await page.evaluate(() => window.modbit.debugCoreInfo());
    process.kill(info!.pid, "SIGKILL");
    await expect.poll(() => page.evaluate(() => (window as unknown as { __trays: string[] }).__trays.includes("core-offline")), { timeout: 90_000 }).toBe(true);
    await expect(page.getByTestId("core-status")).toContainText("Core connected", { timeout: 90_000 });
    await expect(page.locator('[data-tray-id="core-offline"]')).toHaveCount(0, { timeout: 30_000 });
    // The conversation stayed readable throughout.
    await expect(page.getByTestId("conv-assistant").first()).toBeVisible();
  } finally {
    await closeApp(app);
    model.server.close();
  }
});

test("PX-060: when policy blocks the model the turn fails before any token and a tray names the cause in the Core's words", async () => {
  test.setTimeout(240_000);
  const repo = newRepo();
  const model = await routedModel(async (): Promise<ModelPart[]> => ["This must never be asked."]);
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-policy-"));
  const { app, page } = await launch(dataDir, { env: { MODBIT_OPENAI_BASE_URL: model.url, MODBIT_MODEL_POLICY: "block=*" } });
  try {
    await setContentSize(app, page, 1280, 800);
    await startAndOpen(page, "ask the blocked model", repo);
    await expect(page.getByTestId("tray-policy-text")).toBeVisible({ timeout: 90_000 });
    await expect(page.getByTestId("tray-policy-text")).toContainText("Your prompt is kept");
    await accessible(page, "the policy-blocked tray");
    expect(model.requests(), "no request reached the model").toBe(0);
  } finally {
    await closeApp(app);
    model.server.close();
  }
});
