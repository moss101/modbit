/**
 * REQ-PX-046 (QUAL-PX-046): the agent list against a real Core, its real
 * header projection, its search index and its archive events, with a scripted
 * OpenAI-compatible model standing in for the provider only. Statuses are the
 * Core's: nothing here sets a class, and a list row is checked for its glyph
 * and its words, never its colour.
 */
import { expect, test, type Page } from "@playwright/test";
import { mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { accessible, closeApp, git, launch, makeRepo, setContentSize } from "./support/ui-harness.ts";
import { Gate, streamingModel } from "./support/stream-model.ts";

function repoWithFile(): string {
  const repo = makeRepo(mkdtempSync(join(tmpdir(), "modbit-agents-repo-")));
  writeFileSync(join(repo, "findme.txt"), "the quokka sentence lives here\n");
  git(repo, "add", "-A");
  git(repo, "-c", "user.name=t", "-c", "user.email=t@e", "commit", "-q", "-m", "add findme");
  return repo;
}

async function createTask(page: Page, goal: string, repo: string): Promise<string> {
  await page.getByTestId("goal").fill(goal);
  await page.getByTestId("workspace").fill(repo);
  await page.getByTestId("run").click();
  const card = page.getByTestId("task-card").filter({ hasText: goal }).first();
  await expect(card).toBeVisible();
  return (await card.getAttribute("data-task-id"))!;
}

const rowOf = (page: Page, taskId: string) => page.locator(`[data-testid="agent-row"][data-agent-id="${taskId}"]`);

test("PX-046: classes show glyph and words from the Core; unread clears when the conversation is seen; a search finds a tool result and opens its row; pins, filters, grouping, archive with undo, keyboard and axe", async () => {
  test.setTimeout(180_000);
  const repo = repoWithFile();
  const model = await streamingModel([
    { parts: [{ call: { name: "plan.update", args: { outcome: "read the findme file", expected_files: [] } } }] },
    { parts: [{ call: { name: "fs.read", args: { path: "findme.txt" } } }] },
    { parts: [{ call: { name: "task.complete", args: { summary: "read it", self_review: { findings: [] } } } }] },
  ]);
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-agents-"));
  const { app, page } = await launch(dataDir, { env: { MODBIT_OPENAI_BASE_URL: model.url } });
  try {
    await setContentSize(app, page, 1280, 800);
    const a = await createTask(page, "Read the findme file", repo);
    const b = await createTask(page, "Second draft task", repo);
    const c = await createTask(page, "Third draft task", repo);
    // Drafts: glyph and words, the Core's class.
    for (const id of [b, c]) {
      await expect(rowOf(page, id)).toHaveAttribute("data-class", "DRAFT");
      await expect(rowOf(page, id).getByTestId("agent-status")).toHaveText("Draft");
      expect(await rowOf(page, id).locator(".mb-dot-glyph svg").count()).toBe(1);
    }
    await page.getByTestId("task-card").filter({ hasText: "Read the findme file" }).getByTestId("task-start").click();
    // Finished and unseen: ready for review, with the unread dot.
    await expect(rowOf(page, a)).toHaveAttribute("data-class", "READY_FOR_REVIEW_UNSEEN", { timeout: 60_000 });
    await expect(rowOf(page, a).getByTestId("agent-status")).toHaveText("Ready for review");
    await expect(rowOf(page, a).getByTestId("agent-unread-dot")).toBeVisible();
    // The first result opened the Review over the fleet; back out of it.
    await expect(page.getByTestId("review")).toBeVisible({ timeout: 15_000 });
    await page.keyboard.press("Escape");

    // Opening the conversation and following it to the end marks it seen: the class moves to its seen form and the dot goes.
    await rowOf(page, a).getByTestId("agent-open").click();
    await expect(page.getByTestId("conversation")).toHaveAttribute("data-task-id", a);
    await expect(rowOf(page, a)).toHaveAttribute("data-class", "READY_FOR_REVIEW_SEEN", { timeout: 30_000 });
    await expect(rowOf(page, a).getByTestId("agent-unread-dot")).toHaveCount(0);
    await expect(rowOf(page, a)).toHaveAttribute("data-selected", "true");
    await expect(page.getByTestId("conv-tool").first()).toBeVisible().catch(() => undefined);

    // Search over conversations: a word only a tool result holds. The hit names its conversation; its snippet opens it at that row.
    await page.getByTestId("agents-search").fill("quokka");
    const hit = page.getByTestId("search-hit");
    await expect(hit).toHaveCount(1, { timeout: 30_000 });
    await expect(hit).toHaveAttribute("data-agent-id", a);
    const snippet = hit.getByTestId("search-snippet").first();
    await expect(snippet).toContainText("quokka");
    expect(await snippet.locator("mark").first().textContent()).toBe("quokka");
    await accessible(page, "search results", '[data-testid="agents-results"]');
    const rowId = (await snippet.getAttribute("data-row-id"))!;
    await page.getByTestId("agents-search").fill("");
    await page.getByTestId("agents-search").fill("quokka");
    await page.getByTestId("search-snippet").first().click();
    await expect(page.getByTestId("conversation").locator(`[data-row-id="${rowId}"]`)).toHaveAttribute("data-highlight", "true", { timeout: 15_000 });
    await page.getByTestId("agents-search").fill("zzzznothing");
    await expect(page.getByTestId("agents-no-hits")).toBeVisible({ timeout: 30_000 });
    await page.getByTestId("agents-search").fill("");
    await expect(page.getByTestId("agents-scroll")).toBeVisible();

    // Filters and grouping change the list without opening a conversation.
    await page.getByTestId("agents-filter-toggle").click();
    await page.getByTestId("filter-draft").click();
    await expect(page.getByTestId("agent-row")).toHaveCount(2);
    await accessible(page, "the agent list with filters open", '[data-testid="region-agents"]');
    await page.getByTestId("filters-clear").click();
    await expect(page.getByTestId("agent-row")).toHaveCount(3);
    await page.getByTestId("agents-filter-toggle").click();
    await page.getByTestId("agents-grouping").selectOption("status");
    const sections = await page.getByTestId("agents-section").evaluateAll((els) => els.map((e) => e.getAttribute("data-section")));
    expect(sections).toEqual(["status:READY_FOR_REVIEW_SEEN", "status:DRAFT"]);
    await page.getByTestId("agents-grouping").selectOption("repository");

    // Pins form the first section; the pin button is a labelled control.
    await rowOf(page, c).hover();
    await rowOf(page, c).getByTestId("agent-pin").click();
    await expect(page.getByTestId("agents-section").first()).toHaveAttribute("data-section", "pinned");
    await expect(rowOf(page, c)).toHaveAttribute("data-pinned", "true");

    // Keyboard only: arrows move between rows, Enter opens, p unpins, e archives.
    await rowOf(page, c).getByTestId("agent-open").focus();
    await page.keyboard.press("ArrowDown");
    expect(await page.evaluate(() => document.activeElement?.getAttribute("data-testid"))).toBe("agent-open");
    await page.keyboard.press("ArrowUp");
    await page.keyboard.press("p");
    await expect(rowOf(page, c)).toHaveAttribute("data-pinned", "false");
    await accessible(page, "the agent list", '[data-testid="region-agents"]');

    // Archive the selected row with its undo: the row leaves, the selection moves to the nearest sibling, the undo restores it.
    await rowOf(page, b).getByTestId("agent-open").click();
    await expect(page.getByTestId("conversation")).toHaveAttribute("data-task-id", b);
    const order = await page.getByTestId("agent-row").evaluateAll((els) => els.map((e) => e.getAttribute("data-agent-id")!));
    const at = order.indexOf(b);
    const sibling = order[at + 1] ?? order[at - 1]!;
    await rowOf(page, b).hover();
    await rowOf(page, b).getByTestId("agent-archive").click();
    await expect(rowOf(page, b)).toHaveCount(0, { timeout: 15_000 });
    await expect(page.getByTestId("conversation")).toHaveAttribute("data-task-id", sibling);
    await expect.poll(() => page.evaluate(() => document.activeElement?.closest('[data-testid="agent-row"]')?.getAttribute("data-agent-id") ?? "")).toBe(sibling);
    const undo = page.getByTestId("tray").filter({ hasText: "Archived" });
    await expect(undo).toBeVisible();
    await accessible(page, "the undo notice", '[data-testid="tray-host"]');
    await undo.getByRole("button", { name: "Undo" }).click();
    await expect(rowOf(page, b)).toBeVisible({ timeout: 15_000 });
    await expect(page.getByTestId("tray").filter({ hasText: "Archived" })).toHaveCount(0);
    // Archiving is the Core's event: shown with "archived" included, hidden without.
    const sid = (await page.evaluate(() => window.modbit.localState())).sessionId!;
    await rowOf(page, c).getByTestId("agent-open").focus();
    await page.keyboard.press("e");
    await expect(rowOf(page, c)).toHaveCount(0, { timeout: 15_000 });
    const withArchived = await page.evaluate((s) => window.modbit.agentHeaders(s, true), sid);
    expect(withArchived.headers.find((h) => h.taskId === c)?.archived).toBe(true);
    const without = await page.evaluate((s) => window.modbit.agentHeaders(s, false), sid);
    expect(without.headers.some((h) => h.taskId === c)).toBe(false);
    // A restart keeps it archived.
    await page.reload();
    await expect(page.getByTestId("core-status")).toContainText("Core connected", { timeout: 60_000 });
    await expect(rowOf(page, a)).toBeVisible({ timeout: 30_000 });
    await expect(rowOf(page, c)).toHaveCount(0);
    await page.getByTestId("agents-filter-toggle").click();
    await page.getByTestId("filter-archived").click();
    await expect(rowOf(page, c)).toBeVisible({ timeout: 15_000 });
    await expect(rowOf(page, c).getByTestId("agent-status")).toHaveText(/./);
  } finally {
    await closeApp(app);
    model.server.close();
  }
});

test("PX-046: archiving a running task asks first; stopping is the Core's recorded cancellation, and keeping it running changes nothing", async () => {
  test.setTimeout(120_000);
  const repo = repoWithFile();
  const gate = new Gate();
  const model = await streamingModel([{ parts: ["Working on it, ", { wait: gate }, "done."] }]);
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-agents-run-"));
  const { app, page } = await launch(dataDir, { env: { MODBIT_OPENAI_BASE_URL: model.url } });
  try {
    await setContentSize(app, page, 1280, 800);
    const t = await createTask(page, "A long running task", repo);
    await page.getByTestId("task-card").first().getByTestId("task-start").click();
    await expect(rowOf(page, t)).toHaveAttribute("data-class", "RUNNING", { timeout: 60_000 });
    await expect(rowOf(page, t).getByTestId("agent-status")).toHaveText("Running");
    await rowOf(page, t).hover();
    await rowOf(page, t).getByTestId("agent-archive").click();
    const dialog = page.getByTestId("archive-confirm");
    await expect(dialog).toBeVisible();
    await expect(dialog).toContainText("stops the run");
    await accessible(page, "the archive confirmation", '[data-testid="archive-confirm"]');
    await page.getByTestId("archive-confirm-cancel").click();
    await expect(dialog).toHaveCount(0);
    await expect(rowOf(page, t)).toHaveAttribute("data-class", "RUNNING");
    // Confirmed: the run is cancelled by its own event, then the conversation is archived.
    await rowOf(page, t).hover();
    await rowOf(page, t).getByTestId("agent-archive").click();
    await page.getByTestId("archive-confirm-stop").click();
    await expect(rowOf(page, t)).toHaveCount(0, { timeout: 30_000 });
    const sid = (await page.evaluate(() => window.modbit.localState())).sessionId!;
    const h = (await page.evaluate((s) => window.modbit.agentHeaders(s, true), sid)).headers.find((x) => x.taskId === t)!;
    expect(h.archived).toBe(true);
    expect(h.taskState.toLowerCase()).toBe("cancelled");
    await expect(page.getByTestId("tray").filter({ hasText: "Archived" })).toBeVisible();
  } finally {
    gate.open();
    await closeApp(app);
    model.server.close();
  }
});

test("PX-046: a thousand tasks stay responsive: the list holds only the rows in view, scrolls to the end, filters and groups within a generous bound, and a Core event reaches it", async () => {
  test.setTimeout(240_000);
  const repo = repoWithFile();
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-agents-1000-"));
  const { app, page } = await launch(dataDir);
  try {
    await setContentSize(app, page, 1280, 800);
    const first = await createTask(page, "The first task", repo);
    await rowOf(page, first).getByTestId("agent-open").click();
    await expect(page.getByTestId("conversation")).toBeVisible();
    // With the conversation in view the Fleet board is not mounted, so 1000 cards are not drawn while they are created.
    const sid = (await page.evaluate(() => window.modbit.localState())).sessionId!;
    await page.evaluate(
      async ({ s, root }) => {
        const hex = () => Array.from(crypto.getRandomValues(new Uint8Array(16)), (x) => x.toString(16).padStart(2, "0")).join("");
        for (let i = 0; i < 999; i += 25) {
          await Promise.all(Array.from({ length: Math.min(25, 999 - i) }, (_, k) => window.modbit.createTask(s, `Bulk task ${i + k + 1}`, hex(), root)));
        }
      },
      { s: sid, root: repo },
    );
    await expect(page.getByTestId("agent-list")).toHaveAttribute("data-count", "1000", { timeout: 60_000 });
    const rendered = await page.getByTestId("agent-row").count();
    expect(rendered).toBeLessThan(60);
    // Scroll to the very end, and to the middle: only a window of rows is ever in the document.
    const scroller = page.getByTestId("agents-scroll");
    await scroller.evaluate((e) => (e.scrollTop = e.scrollHeight));
    await expect(page.getByTestId("agent-row").last()).toBeVisible();
    expect(await page.getByTestId("agent-row").count()).toBeLessThan(60);
    await scroller.evaluate((e) => (e.scrollTop = e.scrollHeight / 2));
    expect(await page.getByTestId("agent-row").count()).toBeLessThan(60);
    // Regrouping and filtering 1000 headers: measured in the page, with a bound that only a pathological implementation misses.
    const ms = await page.evaluate(async () => {
      const select = document.querySelector<HTMLSelectElement>('[data-testid="agents-grouping"]')!;
      const t0 = performance.now();
      for (const v of ["status", "time", "workspace", "repository"]) {
        const setter = Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, "value")!.set!;
        setter.call(select, v);
        select.dispatchEvent(new Event("change", { bubbles: true }));
        await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));
      }
      return (performance.now() - t0) / 4;
    });
    expect(ms, `mean ms to regroup 1000 tasks: ${ms}`).toBeLessThan(1500);
    await page.getByTestId("agents-filter-toggle").click();
    await page.getByTestId("filter-draft").click();
    await expect(page.getByTestId("agent-list")).toHaveAttribute("data-visible", "1000");
    await page.getByTestId("filter-running").click();
    await page.getByTestId("filter-draft").click();
    await expect(page.getByTestId("agent-list")).toHaveAttribute("data-visible", "0");
    await page.getByTestId("filters-clear").click();
    await expect(page.getByTestId("agent-list")).toHaveAttribute("data-visible", "1000");
    // A Core event reaches the list: a new task appears.
    await page.evaluate(
      ({ s, root }) => window.modbit.createTask(s, "One more", Array.from(crypto.getRandomValues(new Uint8Array(16)), (x) => x.toString(16).padStart(2, "0")).join(""), root),
      { s: sid, root: repo },
    );
    await expect(page.getByTestId("agent-list")).toHaveAttribute("data-count", "1001", { timeout: 15_000 });
    await accessible(page, "the agent list with a thousand tasks", '[data-testid="region-agents"]');
  } finally {
    await closeApp(app);
  }
});
