/**
 * REQ-PX-048, per-task panel state, on the real stack: two real tasks (each a
 * real Git checkout, run to review by a scripted OpenAI-compatible model). The
 * panel remembers its tab for each task by the task's id; selecting another
 * task shows that task's tab, never the first one's, and the memory survives a
 * restart of the app.
 */
import { expect, test, type Page } from "@playwright/test";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { closeApp, launch, makeRepo, scriptedModel, setContentSize } from "./support/ui-harness.ts";

const SCRIPT = [
  { calls: [{ name: "plan.update", args: { outcome: "edit notes", expected_files: ["notes.txt"] } }] },
  { calls: [{ name: "fs.read", args: { path: "notes.txt" } }] },
  { calls: [{ name: "change.apply", args: { path: "notes.txt", op: "replace", content: "line 1\nline 2 changed\nline 3\n" } }] },
  { calls: [{ name: "task.complete", args: { summary: "edited", self_review: { findings: [] } } }] },
];

async function runToReview(page: Page, goal: string, repo: string, nth: number): Promise<string> {
  await page.getByTestId("goal").fill(goal);
  await page.getByTestId("workspace").fill(repo);
  await page.getByTestId("run").click();
  const card = page.getByTestId("task-card").filter({ hasText: goal });
  await expect(card).toHaveCount(1);
  const taskId = (await card.getAttribute("data-task-id"))!;
  await card.getByTestId("task-start").click();
  await expect(page.getByTestId("column-readyForReview").getByTestId("task-card")).toHaveCount(nth, { timeout: 60_000 });
  // The first result opens the Review itself; leave it for the board.
  if ((await page.getByTestId("review").count()) > 0) await page.keyboard.press("Escape");
  await expect(page.getByTestId("review")).toHaveCount(0);
  return taskId;
}

const row = (page: Page, taskId: string) => page.locator(`[data-testid="agent-row"][data-agent-id="${taskId}"]`);

test("the panel keeps its own tab for each task, by the task's id, across selection and a restart of the app", async () => {
  test.setTimeout(240_000);
  const { server, url } = await scriptedModel(SCRIPT);
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-panel-state-"));
  const repoA = makeRepo(mkdtempSync(join(tmpdir(), "modbit-panel-a-")));
  const repoB = makeRepo(mkdtempSync(join(tmpdir(), "modbit-panel-b-")));
  let { app, page } = await launch(dataDir, { env: { MODBIT_OPENAI_BASE_URL: url } });
  try {
    await setContentSize(app, page, 1280, 800);
    const a = await runToReview(page, "Edit the notes A", repoA, 1);
    const b = await runToReview(page, "Edit the notes B", repoB, 2);
    expect(a).not.toBe(b);

    // Task A: Terminal. Task B: Evidence.
    await row(page, a).click();
    if ((await page.getByTestId("region-apps").count()) === 0) await page.getByTestId("toggle-apps-panel").click();
    await expect(page.getByTestId("apps-panel")).toHaveAttribute("data-task-id", a);
    await page.getByTestId("tab-terminal").click();
    await expect(page.getByTestId("app-terminal-empty")).toBeVisible();
    await row(page, b).click();
    if ((await page.getByTestId("region-apps").count()) === 0) await page.getByTestId("toggle-apps-panel").click();
    await expect(page.getByTestId("apps-panel")).toHaveAttribute("data-task-id", b);
    await expect(page.getByTestId("tab-terminal"), "task B starts with its own default tab, not task A's").toHaveAttribute("aria-selected", "false");
    await page.getByTestId("tab-evidence").click();
    await expect(page.getByTestId("app-evidence")).toBeVisible();

    // Back to A: its tab, not B's.
    await row(page, a).click();
    await expect(page.getByTestId("apps-panel")).toHaveAttribute("data-task-id", a);
    await expect(page.getByTestId("tab-terminal")).toHaveAttribute("aria-selected", "true");
    await expect(page.getByTestId("tab-evidence")).toHaveAttribute("aria-selected", "false");

    const stored = await page.evaluate(() => JSON.parse(localStorage.getItem("modbit.ui.v1")!).panelByTask as Record<string, { open: boolean; tab: string }>);
    expect(stored[a]).toMatchObject({ open: true, tab: "terminal" });
    expect(stored[b]).toMatchObject({ open: true, tab: "evidence" });
    expect(Object.keys(stored).sort()).toEqual([a, b].sort());

    // A restart: each task comes back with its own tab.
    await closeApp(app);
    ({ app, page } = await launch(dataDir, { env: { MODBIT_OPENAI_BASE_URL: url } }));
    await setContentSize(app, page, 1280, 800);
    await expect(page.getByTestId("column-readyForReview").getByTestId("task-card")).toHaveCount(2, { timeout: 30_000 });
    await row(page, b).click();
    await expect(page.getByTestId("apps-panel")).toHaveAttribute("data-task-id", b);
    await expect(page.getByTestId("tab-evidence")).toHaveAttribute("aria-selected", "true");
    await row(page, a).click();
    await expect(page.getByTestId("apps-panel")).toHaveAttribute("data-task-id", a);
    await expect(page.getByTestId("tab-terminal")).toHaveAttribute("aria-selected", "true");
    await expect(page.getByTestId("tab-evidence")).toHaveAttribute("aria-selected", "false");
  } finally {
    await closeApp(app);
    server.close();
  }
});
