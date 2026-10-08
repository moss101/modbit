/**
 * REQ-PX-068 (QUAL-PX-068): worktree management and the apply-back conflict
 * modal against a real Core, real Git repositories and worktrees, the real
 * filesystem, the preload bridge and the renderer. The provider is a scripted
 * OpenAI-compatible server and only writes the files a task is meant to
 * produce. Everything the panel and the modal show is the Core's: the list
 * and its eligibility come from ListWorktrees, a refusal is the Core's typed
 * code, the conflict plan and its options are the Core's, and the checkout's
 * bytes are compared on disk by hash. No wait is on a clock.
 */
import { expect, test, type Page } from "@playwright/test";
import { existsSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { basename, join } from "node:path";
import { accessible, closeApp, git, launch, makeRepo, setContentSize } from "./support/ui-harness.ts";
import { routedModel, sha256File, type ModelPart } from "./support/conversation-harness.ts";
import { Gate } from "./support/stream-model.ts";

const rowOf = (page: Page, taskId: string) => page.locator(`[data-testid="agent-row"][data-agent-id="${taskId}"]`);
const wtRow = (page: Page, taskId: string) => page.locator(`[data-testid="worktree-row"][data-task-id="${taskId}"]`);
const hex32 = () => Array.from(crypto.getRandomValues(new Uint8Array(16)), (x) => x.toString(16).padStart(2, "0")).join("");

const A_TXT = (first: string) => `${first}\ntwo\nthree\nfour\nfive\nsix\nseven\neight\nnine\nten\n`;

function repoWithFiles(): string {
  const repo = makeRepo(mkdtempSync(join(tmpdir(), "modbit-wt-repo-")));
  writeFileSync(join(repo, "a.txt"), A_TXT("one"));
  git(repo, "add", "-A");
  git(repo, "-c", "user.name=t", "-c", "user.email=t@e", "commit", "-q", "-m", "add a.txt");
  return repo;
}

async function session(page: Page): Promise<string> {
  return (await page.evaluate(() => window.modbit.localState())).sessionId!;
}

/** An isolated task through the bridge: its own worktree, the checkout untouched. */
async function isolatedTask(page: Page, goal: string, repo: string): Promise<string> {
  const sid = await session(page);
  await page.evaluate(([s, r]) => window.modbit.trustRepository(s!, r!), [sid, repo]);
  const t = await page.evaluate(([s, g, c, r]) => window.modbit.createTask(s!, g!, c!, r!, undefined, { isolation: "WORKTREE" }), [sid, goal, hex32(), repo]);
  return t.taskId;
}

const start = async (page: Page, taskId: string) => {
  const sid = await session(page);
  await page.evaluate(([s, t]) => window.modbit.startTask(s!, t!), [sid, taskId]);
};

const plan = (files: string[]): ModelPart => ({ call: { name: "plan.update", args: { outcome: "change the files", expected_files: files } } });
const replace = (path: string, content: string): ModelPart => ({ call: { name: "change.apply", args: { path, op: "replace", content } } });
const create = (path: string, content: string): ModelPart => ({ call: { name: "change.apply", args: { path, op: "create", content } } });
const read = (path: string): ModelPart => ({ call: { name: "fs.read", args: { path } } });
const complete: ModelPart = { call: { name: "task.complete", args: { summary: "done", self_review: { findings: [] } } } };

const NO_REAPER = { MODBIT_WORKTREE_CLEANUP_CATCHUP_MS: "3600000", MODBIT_WORKTREE_CLEANUP_INTERVAL_MS: "3600000" };

async function openWorktrees(page: Page): Promise<void> {
  await page.getByTestId("agents-worktrees").click();
  await expect(page.getByTestId("worktrees-page")).toHaveAttribute("data-state", "ready");
}

test("PX-068: the list shows real worktrees with the Core's states; a running or unapplied one is refused removal with the Core's typed explanation and nothing is touched; a clean one is removed after the Core says what is lost; the reaper's attention and a discard are the Core's", async () => {
  test.setTimeout(240_000);
  const repo = repoWithFiles();
  const gate = new Gate();
  const model = await routedModel(async (goal, results) => {
    if (goal === "Running worktree") {
      await gate.opened;
      return ["Done waiting."];
    }
    if (goal === "Unapplied worktree") return [[plan(["agent.txt"])], [create("agent.txt", "from the task\n")], [complete]][results] ?? ["Nothing further."];
    return ["Nothing further."];
  });
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-wt-"));
  const { app, page } = await launch(dataDir, { env: { MODBIT_OPENAI_BASE_URL: model.url, MODBIT_WORKTREE_PROTECT_MS: "1", ...NO_REAPER } });
  try {
    await setContentSize(app, page, 1280, 800);
    // ---- The Fleet form offers the isolation (PX-118): this task is created in a worktree of its own.
    await page.getByTestId("goal").fill("Clean worktree");
    await page.getByTestId("workspace").fill(repo);
    await page.getByTestId("isolate").check();
    await page.getByTestId("run").click();
    const clean = (await page.getByTestId("task-card").filter({ hasText: "Clean worktree" }).first().getAttribute("data-task-id"))!;
    const sid = await session(page);
    await page.evaluate(([s, t]) => window.modbit.cancelTask(s!, t!), [sid, clean]);
    const running = await isolatedTask(page, "Running worktree", repo);
    await start(page, running);
    await expect(rowOf(page, running)).toHaveAttribute("data-class", "RUNNING", { timeout: 60_000 });
    const unapplied = await isolatedTask(page, "Unapplied worktree", repo);
    await start(page, unapplied);
    await expect(rowOf(page, unapplied)).toHaveAttribute("data-class", /READY_FOR_REVIEW/, { timeout: 90_000 });
    // Isolated tasks are listed under the checkout they came from, not under their worktree folders.
    await expect(page.locator('[data-testid="agents-section"]').filter({ hasText: basename(repo) })).toHaveCount(1);

    // ---- The list: one group for the repository, the Core's states, sizes and ages.
    await openWorktrees(page);
    await expect(page.getByTestId("worktrees-group")).toHaveCount(1);
    await expect(page.getByTestId("worktree-row")).toHaveCount(3);
    await expect(wtRow(page, clean)).toHaveAttribute("data-states", "removable");
    await expect(wtRow(page, running)).toHaveAttribute("data-states", "running");
    // The task finished and waits for the person's review: the Core counts it as not ended.
    await expect(wtRow(page, unapplied)).toHaveAttribute("data-states", "review dirty unapplied");
    await expect(wtRow(page, unapplied).getByTestId("worktree-facts")).toContainText("1 changed file");
    for (const t of [clean, running, unapplied]) {
      await expect(wtRow(page, t).getByTestId("worktree-size")).toHaveText(/\d+(\.\d)? (B|KB|MB)/);
      await expect(wtRow(page, t).getByTestId("worktree-age")).toHaveText(/just now|\d+ (min|h|days)/);
      expect(existsSync(await wtRow(page, t).getByTestId("worktree-path").innerText())).toBe(true);
    }
    // The list is the Core's: what it holds is what the Core lists, with the same flags.
    const core = await page.evaluate(() => window.modbit.listWorktrees({}));
    expect(core.worktrees.map((w) => [w.taskId, w.removable, w.taskRunning, w.unapplied]).sort()).toEqual(
      [
        [clean, true, false, false],
        [running, false, true, false],
        [unapplied, false, true, true],
      ].sort(),
    );
    // While a task has not ended nothing needs a decision yet: the reaper raises no attention line.
    await expect(page.getByTestId("worktrees-attention-line")).toHaveCount(0);
    await accessible(page, "the worktree list", '[data-testid="worktrees-page"]');

    // ---- A running worktree: the Core's typed refusal, explained; there is no way to confirm.
    const runningPath = await wtRow(page, running).getByTestId("worktree-path").innerText();
    await wtRow(page, running).getByTestId("worktree-remove").click();
    const dialog = page.getByTestId("worktree-remove-dialog");
    await expect(dialog.getByTestId("worktree-remove-refusal")).toHaveAttribute("data-code", "TASK_RUNNING");
    await expect(dialog.getByTestId("worktree-remove-code")).toHaveText("TASK_RUNNING");
    await expect(dialog.getByTestId("worktree-remove-reason")).toContainText("its task has not ended");
    await expect(dialog.getByTestId("worktree-remove-confirm")).toHaveCount(0);
    await accessible(page, "a removal refusal", '[data-testid="worktree-remove-dialog"]');
    await dialog.getByTestId("worktree-remove-cancel").click();
    await expect(dialog).toHaveCount(0);
    expect(existsSync(runningPath)).toBe(true);

    // ---- One in review is held too; once the task is stopped, the unapplied result needs a decision, and the file is as it was.
    const unappliedPath = await wtRow(page, unapplied).getByTestId("worktree-path").innerText();
    const agentFile = join(unappliedPath, "agent.txt");
    const agentHash = sha256File(agentFile);
    await wtRow(page, unapplied).getByTestId("worktree-remove").click();
    await expect(dialog.getByTestId("worktree-remove-refusal")).toHaveAttribute("data-code", "TASK_RUNNING");
    await dialog.getByTestId("worktree-remove-cancel").click();
    await page.evaluate(([s, t]) => window.modbit.cancelTask(s!, t!), [sid, unapplied]);
    await expect(wtRow(page, unapplied)).toHaveAttribute("data-states", "dirty unapplied", { timeout: 30_000 });
    // The reaper's attention line for what only a person can decide.
    await expect(page.getByTestId("worktrees-attention-line")).toHaveCount(1);
    await expect(page.getByTestId("worktrees-attention-line")).toContainText("dirty");
    await expect(page.getByTestId("worktrees-attention-line")).toContainText("ApplyWorktree or DiscardWorktree decides it");
    await wtRow(page, unapplied).getByTestId("worktree-remove").click();
    await expect(dialog.getByTestId("worktree-remove-refusal")).toHaveAttribute("data-code", "WORKTREE_NEEDS_DECISION");
    await expect(dialog.getByTestId("worktree-remove-reason")).toContainText("dirty");
    await expect(dialog.getByTestId("worktree-remove-confirm")).toHaveCount(0);
    await dialog.getByTestId("worktree-remove-cancel").click();
    expect(sha256File(agentFile)).toBe(agentHash);

    // ---- A clean one: the Core says what is lost; keeping it first changes nothing; then it is removed.
    const cleanPath = await wtRow(page, clean).getByTestId("worktree-path").innerText();
    await wtRow(page, clean).getByTestId("worktree-remove").click();
    await expect(dialog.getByTestId("worktree-remove-loses")).toContainText(cleanPath);
    await expect(dialog.getByTestId("worktree-remove-why")).toContainText("holds nothing beyond its base");
    await accessible(page, "a removal preview", '[data-testid="worktree-remove-dialog"]');
    await expect(dialog.getByTestId("worktree-remove-cancel")).toBeFocused();
    await page.keyboard.press("Enter");
    await expect(dialog).toHaveCount(0);
    expect(existsSync(cleanPath)).toBe(true);
    await wtRow(page, clean).getByTestId("worktree-remove").click();
    await dialog.getByTestId("worktree-remove-confirm").click();
    await expect(dialog).toHaveCount(0);
    await expect(wtRow(page, clean)).toHaveCount(0);
    expect(existsSync(cleanPath)).toBe(false);
    expect(existsSync(runningPath) && existsSync(unappliedPath)).toBe(true);

    // ---- Cleanup: a preview then a run, each reported as the Core reports it.
    await page.getByTestId("worktrees-dry-run").click();
    await expect(page.getByTestId("worktrees-report")).toHaveAttribute("data-status", "DRY_RUN", { timeout: 30_000 });
    await expect(page.getByTestId("worktrees-report")).toContainText("Dry run");
    await page.getByTestId("worktrees-cleanup").click();
    await expect(page.getByTestId("worktrees-report")).toHaveAttribute("data-status", "COMPLETED", { timeout: 30_000 });
    await expect(page.getByTestId("worktrees-report")).toContainText("Completed: 0 removed");
    expect(existsSync(unappliedPath)).toBe(true);

    // ---- Discard: the branch name must be typed; the Core checks it; then the worktree is removable.
    await wtRow(page, unapplied).getByTestId("worktree-discard").click();
    const discard = page.getByTestId("worktree-discard-dialog");
    await expect(discard.getByTestId("worktree-discard-confirm")).toBeDisabled();
    await discard.getByTestId("worktree-discard-confirm-input").fill("not-the-branch");
    await discard.getByTestId("worktree-discard-confirm").click();
    await expect(discard.getByTestId("worktree-discard-problem")).toContainText("DISCARD_NOT_CONFIRMED");
    const branch = await wtRow(page, unapplied).getByTestId("worktree-branch").innerText();
    await discard.getByTestId("worktree-discard-confirm-input").fill(branch);
    await discard.getByTestId("worktree-discard-confirm").click();
    await expect(discard).toHaveCount(0);
    await expect(wtRow(page, unapplied)).toHaveAttribute("data-states", /removable/, { timeout: 30_000 });
    await expect(wtRow(page, unapplied).getByTestId("worktree-disposition")).toHaveText("Discarded");
    await expect(page.getByTestId("worktrees-attention-line")).toHaveCount(0);
    expect(existsSync(unappliedPath)).toBe(true);

    // ---- Release the running task; its worktree is then decided by what it holds, not by a stale flag.
    gate.open();
    await expect(rowOf(page, running)).toHaveAttribute("data-class", /READY_FOR_REVIEW|COMPLETED|WAITING|NEEDS_ATTENTION|FAILED/, { timeout: 60_000 });
  } finally {
    gate.open();
    await closeApp(app);
    model.server.close();
  }
});

test("PX-068: a real apply-back conflict through the modal - Cancel is the default and changes nothing, an overwrite needs its files typed (and the Core refuses it otherwise), the pre-apply bytes come back exactly with Undo, and only a non-destructive choice is remembered", async () => {
  test.setTimeout(300_000);
  const repo = repoWithFiles();
  const model = await routedModel((goal, results) => {
    if (goal !== "Apply back") return ["Nothing further."];
    // The Core wants a file read before it is changed (RETRIEVAL_REQUIRED), as a person would.
    return [[plan(["a.txt", "new/dir/b.txt"])], [read("a.txt")], [replace("a.txt", A_TXT("ONE"))], [create("new/dir/b.txt", "added by the task\n")], [complete]][results] ?? ["Nothing further."];
  });
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-apply-"));
  const { app, page } = await launch(dataDir, { env: { MODBIT_OPENAI_BASE_URL: model.url, MODBIT_WORKTREE_PROTECT_MS: "1", ...NO_REAPER } });
  try {
    await setContentSize(app, page, 1280, 800);
    // The first task creates the session through the Fleet form; the task under test is isolated.
    await page.getByTestId("goal").fill("Warm up");
    await page.getByTestId("workspace").fill(repo);
    await page.getByTestId("run").click();
    await expect(page.getByTestId("task-card").filter({ hasText: "Warm up" })).toBeVisible();
    const sid = await session(page);
    const task = await isolatedTask(page, "Apply back", repo);
    await start(page, task);
    await expect(rowOf(page, task)).toHaveAttribute("data-class", /READY_FOR_REVIEW/, { timeout: 90_000 });

    // The user edits the same line in the checkout while the task worked.
    const checkoutA = join(repo, "a.txt");
    writeFileSync(checkoutA, A_TXT("uno"));
    writeFileSync(join(repo, "scratch.txt"), "untracked\n");
    const userHash = sha256File(checkoutA);
    const scratchHash = sha256File(join(repo, "scratch.txt"));
    const worktreePath = (await page.evaluate(([t]) => window.modbit.listWorktrees({ taskId: t! }), [task])).worktrees[0]!.path;
    const taskHash = sha256File(join(worktreePath, "a.txt"));
    expect(taskHash).not.toBe(userHash);

    await openWorktrees(page);
    await expect(wtRow(page, task)).toHaveAttribute("data-states", "review dirty unapplied");
    const modal = page.getByTestId("apply-modal");

    // ---- The Core classifies; the modal shows its conflict plan and its options. Cancel is selected.
    await wtRow(page, task).getByTestId("worktree-apply").click();
    await expect(modal.getByTestId("apply-conflict")).toBeVisible();
    await expect(modal.getByTestId("apply-path").filter({ hasText: "a.txt" })).toContainText("conflict: both modified");
    await expect(modal.getByTestId("apply-path").filter({ hasText: "new/dir/b.txt" })).toBeVisible();
    await expect(modal.locator('[data-testid="apply-option-CANCEL"] input')).toBeChecked();
    await expect(modal.getByTestId("apply-continue")).toBeDisabled();
    await expect(modal.getByTestId("apply-dirty")).toContainText("scratch.txt");
    await expect(modal.getByText("saves a checkpoint of every file it will touch")).toBeVisible();
    // Only what the Core offers: unavailable options carry its reason and cannot be chosen.
    await expect(modal.locator('[data-testid="apply-option-UNDO_AND_APPLY"] input')).toBeDisabled();
    await expect(modal.getByTestId("apply-option-UNDO_AND_APPLY")).toContainText("no earlier apply of this worktree is in force");
    await expect(modal.getByTestId("apply-option-OVERWRITE")).toHaveAttribute("data-destructive", "true");
    await accessible(page, "the conflict modal", '[data-testid="apply-modal"]');
    // Cancel changes nothing, and neither did classifying.
    await modal.getByTestId("apply-cancel").click();
    await expect(modal).toHaveCount(0);
    expect(sha256File(checkoutA)).toBe(userHash);
    expect(await page.evaluate((s) => window.modbit.dockApprovals(s), sid)).toHaveLength(0);

    // ---- The Core refuses an overwrite whose files were not typed, whatever the renderer sent.
    const refused = await page.evaluate(([s, t]) => window.modbit.applyWorktree(s!, t!, { option: "OVERWRITE" }), [sid, task]);
    expect(refused.status).toBe("REFUSED");
    expect(refused.code).toBe("OVERWRITE_NOT_CONFIRMED");
    expect(sha256File(checkoutA)).toBe(userHash);

    // ---- Overwrite through the modal: the files are typed, the exact plan is approved, the bytes land.
    await wtRow(page, task).getByTestId("worktree-apply").click();
    await expect(modal.getByTestId("apply-conflict")).toBeVisible();
    await modal.locator('[data-testid="apply-option-OVERWRITE"] input').check();
    await expect(modal.getByTestId("apply-remember")).toHaveCount(0);
    await expect(modal.getByTestId("apply-continue")).toBeDisabled();
    const required = await modal.getByTestId("apply-required").locator("code").allInnerTexts();
    expect(required).toEqual(["a.txt"]);
    await modal.getByTestId("apply-confirm-input").fill("something else");
    await expect(modal.getByTestId("apply-continue")).toBeDisabled();
    await modal.getByTestId("apply-confirm-input").fill(required.join("\n"));
    await expect(modal.getByTestId("apply-continue")).toBeEnabled();
    await accessible(page, "the conflict modal with an overwrite chosen", '[data-testid="apply-modal"]');
    await modal.getByTestId("apply-continue").click();
    await expect(modal.getByTestId("apply-approve")).toBeVisible();
    await expect(modal.getByTestId("apply-approve-option")).toHaveText("Overwrite conflicting files");
    await expect(modal.getByTestId("apply-approve-intent")).toHaveText(/^[0-9a-f]{12}$/);
    await expect(modal.getByTestId("apply-deny")).toBeFocused();
    // Nothing is written before the approval.
    expect(sha256File(checkoutA)).toBe(userHash);
    await accessible(page, "the approval step", '[data-testid="apply-modal"]');
    await modal.getByTestId("apply-approve-button").click();
    await expect(modal.getByTestId("apply-done")).toBeVisible();
    await expect(modal.getByTestId("apply-done-words")).toHaveText("Applied 2 files to your checkout.");
    await expect(modal.getByTestId("apply-receipts")).toContainText("effect receipt");
    expect(sha256File(checkoutA)).toBe(taskHash);
    expect(sha256File(join(repo, "scratch.txt"))).toBe(scratchHash);
    expect(sha256File(join(repo, "new", "dir", "b.txt"))).toBe(sha256File(join(worktreePath, "new", "dir", "b.txt")));
    await accessible(page, "the applied step", '[data-testid="apply-modal"]');

    // ---- Undo restores the exact bytes: the user's version, the file the task added gone, the untracked file untouched.
    await modal.getByTestId("apply-undo").click();
    await expect(modal.getByTestId("apply-approve")).toBeVisible();
    await modal.getByTestId("apply-approve-button").click();
    await expect(modal.getByTestId("apply-done-words")).toContainText("Restored");
    expect(sha256File(checkoutA)).toBe(userHash);
    expect(sha256File(join(repo, "scratch.txt"))).toBe(scratchHash);
    expect(existsSync(join(repo, "new", "dir", "b.txt"))).toBe(false);
    await modal.getByTestId("apply-close").click();
    await expect(modal).toHaveCount(0);

    // ---- A destructive choice is never remembered, even when asked: the next classification has no remembered option.
    const pending = await page.evaluate(([s, t]) => window.modbit.applyWorktree(s!, t!, { option: "OVERWRITE", confirmPaths: ["a.txt"], remember: true }), [sid, task]);
    expect(pending.status).toBe("APPROVAL_PENDING");
    expect(pending.detail).toContain("never remembered");
    await page.evaluate(([s, a, h]) => window.modbit.resolveApproval(s!, a!, false, "not now", h!), [sid, pending.approvalId, pending.intentHash]);
    const again = await page.evaluate(([s, t]) => window.modbit.applyWorktree(s!, t!, {}), [sid, task]);
    expect(again.status).toBe("CONFLICT");
    expect(again.conflict?.rememberedOption).toBe("");
    expect(sha256File(checkoutA)).toBe(userHash);

    // ---- A non-destructive choice may be remembered: Merge manually leaves markers, Undo puts the bytes back,
    // and the next conflict goes straight to the approval of that remembered choice.
    await wtRow(page, task).getByTestId("worktree-apply").click();
    await expect(modal.getByTestId("apply-conflict")).toBeVisible();
    await modal.locator('[data-testid="apply-option-MERGE_MANUALLY"] input').check();
    await modal.getByTestId("apply-remember").check();
    await modal.getByTestId("apply-continue").click();
    await modal.getByTestId("apply-approve-button").click();
    await expect(modal.getByTestId("apply-done")).toBeVisible();
    await expect(modal.getByTestId("apply-done-words")).toContainText("conflict markers to resolve");
    await expect(modal.getByTestId("apply-unresolved")).toContainText("a.txt");
    expect(sha256File(checkoutA)).not.toBe(userHash);
    expect(readFileSync(checkoutA, "utf8")).toContain("<<<<<<< checkout");
    await modal.getByTestId("apply-undo").click();
    await modal.getByTestId("apply-approve-button").click();
    await expect(modal.getByTestId("apply-done-words")).toContainText("Restored");
    expect(sha256File(checkoutA)).toBe(userHash);
    await modal.getByTestId("apply-close").click();

    await wtRow(page, task).getByTestId("worktree-apply").click();
    await expect(modal.getByTestId("apply-approve")).toBeVisible();
    await expect(modal.getByTestId("apply-approve-option")).toHaveText("Merge manually");
    await expect(modal.getByTestId("apply-conflict")).toHaveCount(0);
    // Denying changes nothing.
    await modal.getByTestId("apply-deny").click();
    await expect(modal.getByTestId("apply-stopped")).toHaveAttribute("data-reason", "DENIED");
    expect(sha256File(checkoutA)).toBe(userHash);
    await modal.getByTestId("apply-close").click();
  } finally {
    await closeApp(app);
    model.server.close();
  }
});

test("PX-068: the main process rejects invalid worktree arguments before the Core sees them", async () => {
  test.setTimeout(120_000);
  const repo = repoWithFiles();
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-wtargs-"));
  const { app, page } = await launch(dataDir);
  try {
    await page.getByTestId("goal").fill("Args");
    await page.getByTestId("workspace").fill(repo);
    await page.getByTestId("run").click();
    await expect(page.getByTestId("task-card").filter({ hasText: "Args" })).toBeVisible();
    const sid = await session(page);
    const call = (expr: string) => page.evaluate(`(async () => { try { await (${expr}); return "accepted"; } catch (e) { return e.message; } })()`) as Promise<string>;
    const tid = "a".repeat(32);
    expect(await call(`window.modbit.removeWorktree("${sid}", "", true)`)).toContain("BAD_ARGUMENT: worktreeId");
    expect(await call(`window.modbit.removeWorktree("${sid}", "w", "yes")`)).toContain("BAD_ARGUMENT: dryRun");
    expect(await call(`window.modbit.applyWorktree("${sid}", "${tid}", { option: "RM_RF" })`)).toContain("BAD_ARGUMENT: option");
    expect(await call(`window.modbit.applyWorktree("${sid}", "${tid}", { option: "OVERWRITE", confirmPaths: "a.txt" })`)).toContain("BAD_ARGUMENT: confirmPaths");
    expect(await call(`window.modbit.discardWorktree("${sid}", "${tid}", "x", "")`)).toContain("BAD_ARGUMENT: confirm");
    // Well-formed requests for things that do not exist are the Core's typed refusals.
    expect(await call(`window.modbit.removeWorktree("${sid}", "no-such-worktree", true)`)).toContain("UNKNOWN_WORKTREE");
    expect(await call(`window.modbit.applyWorktree("${sid}", "${tid}", {})`)).toContain("UNKNOWN_TASK");
  } finally {
    await closeApp(app);
  }
});
