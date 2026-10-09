/**
 * Negative and failure proofs QUAL-PX-054, -055 and -062 name, against a real
 * Core process, a real git worktree and the preload bridge. Only the model is
 * scripted. Every wait is for a state the Core or the page reports.
 */
import { expect, test } from "@playwright/test";
import { mkdtempSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { closeApp, launch, makeRepo, setContentSize } from "./support/ui-harness.ts";
import { createTask, Gate, openConversation, sequencedModel, startAndOpen, typeAndPress } from "./support/composer-harness.ts";
import { sessionOf, sha256File } from "./support/conversation-harness.ts";

const NOTES = "line 1\nline 2\nline 3\n";

test("PX-054: in Ask a write the model asks for is refused by the Core and nothing is written", async () => {
  test.setTimeout(240_000);
  const repo = makeRepo(mkdtempSync(join(tmpdir(), "modbit-gap-repo-")));
  const write = { call: { name: "change.apply", args: { path: "notes.txt", op: "replace", content: "changed in ask\n" } } };
  const model = await sequencedModel([{ parts: ["Trying a write.", write] }, { parts: ["The write was refused."] }]);
  const { app, page } = await launch(mkdtempSync(join(tmpdir(), "modbit-e2e-gap-ask-")), { env: { MODBIT_OPENAI_BASE_URL: model.url } });
  try {
    await setContentSize(app, page, 1280, 800);
    const taskId = await createTask(page, "Ask about the notes", repo);
    await openConversation(page, taskId);
    await page.getByTestId("composer-input").focus();
    for (let i = 0; i < 4; i++) await page.keyboard.press("Shift+Tab");
    await expect(page.getByTestId("mode-label")).toHaveText("Ask");
    await expect(page.getByTestId("composer")).toHaveAttribute("data-mode-status", "confirmed");
    expect(await page.evaluate((t) => window.modbit.composerPosture(t).then((p) => p.mode), taskId)).toBe("ASK");
    await page.getByTestId("conv-start").click();
    await expect.poll(() => model.requests(), { timeout: 90_000 }).toBeGreaterThanOrEqual(2);
    const offered = ((model.bodies()[0] as { tools?: { function: { name: string } }[] }).tools ?? []).map((t) => t.function.name);
    expect(offered).not.toContain("change.apply");
    expect(model.text(1)).toMatch(/MODE_POSTURE|TOOL_NOT_VISIBLE|TOOL_NOT_PROJECTED/);
    expect(readFileSync(join(repo, "notes.txt"), "utf8")).toBe(NOTES);
  } finally {
    await closeApp(app);
    model.server.close();
  }
});

test("PX-054: a mode the Core has not acknowledged is drawn unconfirmed and never as active; the Core's acknowledgement confirms it", async () => {
  test.setTimeout(240_000);
  const repo = makeRepo(mkdtempSync(join(tmpdir(), "modbit-gap-repo-")));
  const model = await sequencedModel([]);
  const { app, page } = await launch(mkdtempSync(join(tmpdir(), "modbit-e2e-gap-unconf-")), { env: { MODBIT_OPENAI_BASE_URL: model.url } });
  let pid = 0;
  try {
    await setContentSize(app, page, 1280, 800);
    const taskId = await createTask(page, "Unconfirmed mode", repo);
    await openConversation(page, taskId);
    await page.getByTestId("composer-input").focus();
    pid = (await page.evaluate(() => window.modbit.debugCoreInfo()))!.pid;
    // The Core stops answering (SIGSTOP): the request is out, no acknowledgement can come.
    process.kill(pid, "SIGSTOP");
    await page.keyboard.press("Shift+Tab");
    await expect(page.getByTestId("composer")).toHaveAttribute("data-mode-status", "unconfirmed");
    await expect(page.getByTestId("mode-unconfirmed")).toBeVisible();
    // The Core resumes and acknowledges: only now is the mode confirmed, and the Core's own field agrees.
    process.kill(pid, "SIGCONT");
    await expect(page.getByTestId("composer")).toHaveAttribute("data-mode-status", "confirmed", { timeout: 60_000 });
    await expect(page.getByTestId("mode-label")).toHaveText("Plan");
    expect(await page.evaluate((t) => window.modbit.composerPosture(t).then((p) => p.mode), taskId)).toBe("PLAN");
  } finally {
    if (pid) try { process.kill(pid, "SIGCONT"); } catch { /* already running or gone */ }
    await closeApp(app);
    model.server.close();
  }
});

test("PX-055: the send-behaviour education is shown once and its acknowledgement persists across a reload", async () => {
  test.setTimeout(240_000);
  const repo = makeRepo(mkdtempSync(join(tmpdir(), "modbit-gap-repo-")));
  const gate = new Gate();
  const model = await sequencedModel([{ parts: ["Working on it, ", { wait: gate }, "done."] }]);
  const { app, page } = await launch(mkdtempSync(join(tmpdir(), "modbit-e2e-gap-edu-")), { env: { MODBIT_OPENAI_BASE_URL: model.url } });
  try {
    await setContentSize(app, page, 1280, 800);
    const taskId = await startAndOpen(page, "Education once", repo);
    await expect(page.getByTestId("conv-assistant")).toHaveAttribute("data-message-state", "streaming", { timeout: 60_000 });
    await typeAndPress(page, "first queued", "Enter");
    await expect(page.getByTestId("education-tray")).toBeVisible();
    await page.getByTestId("education-keep").click();
    await expect(page.getByTestId("education-tray")).toHaveCount(0);
    await page.reload();
    await expect(page.getByTestId("core-status")).toContainText("Core connected", { timeout: 60_000 });
    await openConversation(page, taskId);
    await expect(page.getByTestId("queue-header")).toHaveText("1 queued");
    await typeAndPress(page, "second queued", "Enter");
    await expect(page.getByTestId("queue-header")).toHaveText("2 queued");
    await expect(page.getByTestId("education-tray")).toHaveCount(0);
  } finally {
    gate.open();
    await closeApp(app);
    model.server.close();
  }
});

test("PX-062: a checkpoint of another task is refused and nothing changes; a Continue replayed with its command id restores once", async () => {
  test.setTimeout(300_000);
  const repoA = makeRepo(mkdtempSync(join(tmpdir(), "modbit-gap-repo-")));
  const repoB = makeRepo(mkdtempSync(join(tmpdir(), "modbit-gap-repo-")));
  const plan = { call: { name: "plan.update", args: { outcome: "write a.txt", expected_files: ["a.txt"] } } };
  const create = { call: { name: "change.apply", args: { path: "a.txt", op: "create", content: "alpha\n" } } };
  const done = { call: { name: "task.complete", args: { summary: "done", self_review: { findings: [] } } } };
  const model = await sequencedModel([{ parts: ["Planning.", plan] }, { parts: ["A.", create] }, { parts: [done] }]);
  const { app, page } = await launch(mkdtempSync(join(tmpdir(), "modbit-e2e-gap-ckpt-")), { env: { MODBIT_OPENAI_BASE_URL: model.url } });
  try {
    await setContentSize(app, page, 1280, 800);
    const taskB = await createTask(page, "task B is untouched", repoB);
    const taskA = await startAndOpen(page, "task A writes a file", repoA);
    await expect(page.getByTestId("review")).toBeVisible({ timeout: 120_000 });
    await page.keyboard.press("Escape");
    await expect(page.getByTestId("turn-checkpoint")).toHaveCount(3, { timeout: 60_000 });
    const sid = await sessionOf(page);
    await page.getByTestId("nav-fleet").click().catch(() => undefined);
    const list = await page.evaluate((t) => window.modbit.checkpoints(t), taskA);
    const turn1 = list.checkpoints.find((c) => c.turnOrdinal === 1)!;
    const notesB = sha256File(join(repoB, "notes.txt"));

    // Another task's checkpoint, named by the renderer: the Core refuses and the workspace of B is untouched.
    const refused = await page.evaluate(([s, t, c]) => window.modbit.restoreCheckpoint(s!, t!, { checkpointId: c! }, {}).then((r) => (r.restored ? "restored" : `refused:${r.refusal}`), (e: Error) => `rejected:${e.message}`), [sid, taskB, turn1.checkpointId]);
    expect(refused).toMatch(/^(refused|rejected):/);
    expect(sha256File(join(repoB, "notes.txt"))).toBe(notesB);
    expect(readFileSync(join(repoB, "notes.txt"), "utf8")).toBe(NOTES);

    // The same Continue (same command id) sent twice restores once: one pre-restore state, and the replay says so.
    const commandId = "0123456789abcdef0123456789abcdef";
    const outcomes = await page.evaluate(
      async ([s, t, id]) => {
        const a = await window.modbit.restoreCheckpoint(s!, t!, { turnOrdinal: 1 }, { commandId: id! });
        const b = await window.modbit.restoreCheckpoint(s!, t!, { turnOrdinal: 1 }, { commandId: id! });
        const l = await window.modbit.checkpoints(t!);
        return { a: a.restored, b: b.restored, replayed: b.replayed, pre: l.checkpoints.filter((c) => c.retention.includes("PRE_RESTORE")).length };
      },
      [sid, taskA, commandId],
    );
    expect(outcomes).toMatchObject({ a: true, b: true, replayed: true, pre: 1 });
  } finally {
    await closeApp(app);
    model.server.close();
  }
});
