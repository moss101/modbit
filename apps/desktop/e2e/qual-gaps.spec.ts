/**
 * Negative and failure proofs QUAL-PX-054, -055 and -062 name, against a real
 * Core process, a real git worktree and the preload bridge. Only the model is
 * scripted. Every wait is for a state the Core or the page reports.
 */
import { expect, test } from "@playwright/test";
import { createServer } from "node:http";
import { mkdtempSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { accessible, closeApp, launch, makeRepo, setContentSize } from "./support/ui-harness.ts";
import { createTask, Gate, openConversation, sequencedModel, startAndOpen, typeAndPress } from "./support/composer-harness.ts";
import { routedModel, sessionOf, sha256File, type ModelPart } from "./support/conversation-harness.ts";

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

const planCall = { call: { name: "plan.update", args: { outcome: "read the notes", expected_files: [] } } };
const doneCall = { call: { name: "task.complete", args: { summary: "done", self_review: { findings: [] } } } };
const readNotes: ModelPart = { call: { name: "fs.read", args: { path: "notes.txt" } } };

test("PX-056: a pin sent from a session that does not own the task is refused by the Core and the task's preference is unchanged", async () => {
  test.setTimeout(240_000);
  const repo = makeRepo(mkdtempSync(join(tmpdir(), "modbit-gap-repo-")));
  const model = await sequencedModel([]);
  const { app, page } = await launch(mkdtempSync(join(tmpdir(), "modbit-e2e-gap-pin-")), { env: { MODBIT_OPENAI_BASE_URL: model.url } });
  try {
    await setContentSize(app, page, 1280, 800);
    const taskId = await createTask(page, "A task of the first session", repo);
    const other = await page.evaluate(() => window.modbit.createSession());
    const result = await page.evaluate(([s, t]) => window.modbit.composerSetPreference(s!, t!, { pin: { endpoint: "openai", model: "gpt-5" } }).then(() => "accepted", (e: Error) => e.message), [other, taskId]);
    expect(result).not.toBe("accepted");
    const pref = await page.evaluate((t) => window.modbit.composerPosture(t).then((p) => p.preference), taskId);
    expect(pref.pinModel).toBe("");
  } finally {
    await closeApp(app);
    model.server.close();
  }
});

test("PX-060: after a compaction the ring and the tray still equal the Core's accounting, and the Core recorded the epoch", async () => {
  test.setTimeout(300_000);
  const repo = makeRepo(mkdtempSync(join(tmpdir(), "modbit-gap-repo-")));
  const long = (n: number) => `Turn ${n}. ` + "The notes were read and the transcript keeps growing with every sentence the model writes. ".repeat(160);
  // Chosen by request index, not by the tool results in the request: a compaction replaces earlier results with a summary.
  const model = await sequencedModel([{ parts: [long(0), planCall] }, { parts: [long(1), readNotes] }, { parts: [long(2), readNotes] }, { parts: [long(3), readNotes] }, { parts: ["Finished.", doneCall] }]);
  const { app, page } = await launch(mkdtempSync(join(tmpdir(), "modbit-e2e-gap-compact-")), { env: { MODBIT_OPENAI_BASE_URL: model.url, MODBIT_COMPACTION_TOKEN_BUDGET: "2500" } });
  try {
    await setContentSize(app, page, 1280, 800);
    const taskId = await startAndOpen(page, "read the notes until compaction", repo);
    await expect(page.getByTestId("review")).toBeVisible({ timeout: 120_000 });
    await page.keyboard.press("Escape");
    const inspector = await page.evaluate((t) => window.modbit.contextInspector(t), taskId);
    expect(inspector.compactionEpochs, "the Core compacted the transcript").toBeGreaterThan(0);
    const acct = await page.evaluate((t) => window.modbit.contextAccounting(t), taskId);
    const ring = page.getByTestId("context-ring");
    await expect(ring).toHaveAttribute("data-total", String(acct.totalTokens), { timeout: 30_000 });
    await ring.click();
    const tray = page.getByTestId("context-tray");
    await expect(tray).toHaveAttribute("data-total", String(acct.totalTokens));
    await expect(tray).toHaveAttribute("data-sums", "true");
    const tokens = await page.getByTestId("context-category").evaluateAll((els) => els.map((e) => BigInt(e.getAttribute("data-tokens") ?? "0")));
    expect(tokens.reduce((a, b) => a + b, 0n)).toBe(BigInt(acct.totalTokens));
  } finally {
    await closeApp(app);
    model.server.close();
  }
});

test("PX-060: the ring shows the Core's figure for the task's own model across a model switch and never the other model's numbers", async () => {
  test.setTimeout(300_000);
  const repoA = makeRepo(mkdtempSync(join(tmpdir(), "modbit-gap-repo-")));
  const repoB = makeRepo(mkdtempSync(join(tmpdir(), "modbit-gap-repo-")));
  const model = await routedModel(
    async (_goal, results): Promise<ModelPart[]> => (results === 0 ? ["Planning.", planCall] : results === 1 ? ["Reading.", readNotes] : ["Finished.", doneCall]),
    (results) => 40_000 + results * 40_000,
  );
  const { app, page } = await launch(mkdtempSync(join(tmpdir(), "modbit-e2e-gap-switch-")), { env: { MODBIT_OPENAI_BASE_URL: model.url } });
  try {
    await setContentSize(app, page, 1280, 800);
    const ring = page.getByTestId("context-ring");
    const done = async () => {
      await expect(page.getByTestId("review")).toBeVisible({ timeout: 180_000 });
      await page.keyboard.press("Escape");
    };
    // Task B is created first (the task form is on the Fleet) and switched to another model before it starts.
    const taskB = await createTask(page, "second task on a switched model", repoB);
    // Task A runs on the default model.
    const taskA = await startAndOpen(page, "first task on the default model", repoA);
    await done();
    const sid = await sessionOf(page);
    const a = await page.evaluate((t) => window.modbit.contextAccounting(t), taskA);
    expect(a.model).toContain("gpt-5");
    await expect(ring).toHaveAttribute("data-total", String(a.totalTokens), { timeout: 30_000 });
    // Task B's request is counted against the model it was switched to, and that model's window.
    await page.evaluate(([s, t]) => window.modbit.composerSetPreference(s!, t!, { pin: { endpoint: "openai", model: "gpt-4.1" } }), [sid, taskB]);
    await openConversation(page, taskB);
    await page.getByTestId("conv-start").click();
    await done();
    const b = await page.evaluate((t) => window.modbit.contextAccounting(t), taskB);
    expect(b.model, "the Core counted the request against the switched model").toContain("gpt-4.1");
    expect(b.windowTokens, "and against its window").not.toBe(a.windowTokens);
    await expect(ring).toHaveAttribute("data-total", String(b.totalTokens), { timeout: 30_000 });
    await expect(ring).toHaveAttribute("data-percent", String(Math.floor(b.usedBp / 100)));
    await ring.click();
    await expect(page.getByTestId("context-tray")).toContainText("gpt-4.1");
    await expect(page.getByTestId("context-tray")).not.toContainText("gpt-5");
    await page.getByTestId("tray-dismiss").click();
    // Back on task A: its own figure again, none of B's.
    await openConversation(page, taskA);
    await expect(ring).toHaveAttribute("data-total", String(a.totalTokens), { timeout: 30_000 });
    await expect(ring).toHaveAttribute("data-percent", String(Math.floor(a.usedBp / 100)));
    await ring.click();
    await expect(page.getByTestId("context-tray")).toContainText("gpt-5");
    await expect(page.getByTestId("context-tray")).not.toContainText("gpt-4.1");
  } finally {
    await closeApp(app);
    model.server.close();
  }
});

test("PX-060: a provider failure shows the Core's cause with Retry; Retry (even double-clicked) starts one new run that goes on from the log", async () => {
  test.setTimeout(300_000);
  const repo = makeRepo(mkdtempSync(join(tmpdir(), "modbit-gap-repo-")));
  const inner = await sequencedModel([{ parts: ["Planning.", planCall] }, { parts: ["Finished.", doneCall] }]);
  let failing = true;
  let refused = 0;
  const proxy = createServer((req, res) => {
    const chunks: Buffer[] = [];
    req.on("data", (c: Buffer) => chunks.push(c));
    req.on("end", () => {
      if (failing) {
        refused++;
        res.writeHead(500, { "content-type": "application/json" });
        res.end('{"error":{"message":"upstream unavailable"}}');
        return;
      }
      void fetch(inner.url + (req.url ?? ""), { method: req.method ?? "POST", headers: { "content-type": "application/json" }, body: new Uint8Array(Buffer.concat(chunks)) }).then(async (r) => {
        res.writeHead(r.status, { "content-type": r.headers.get("content-type") ?? "text/event-stream" });
        for await (const part of r.body as unknown as AsyncIterable<Uint8Array>) res.write(part);
        res.end();
      });
    });
  });
  await new Promise<void>((r) => proxy.listen(0, "127.0.0.1", r));
  const url = `http://127.0.0.1:${(proxy.address() as { port: number }).port}`;
  const { app, page } = await launch(mkdtempSync(join(tmpdir(), "modbit-e2e-gap-retry-")), { env: { MODBIT_OPENAI_BASE_URL: url } });
  try {
    await setContentSize(app, page, 1280, 800);
    await startAndOpen(page, "fail then retry", repo);
    await expect(page.getByTestId("conv-failure")).toBeVisible({ timeout: 180_000 });
    await expect(page.getByTestId("conv-failure-cause")).toContainText("provider");
    await accessible(page, "the failure block with Retry");
    expect(refused).toBeGreaterThan(0);
    expect(inner.requests()).toBe(0);
    failing = false;
    await page.getByTestId("conv-retry").dblclick();
    await expect(page.getByTestId("review")).toBeVisible({ timeout: 180_000 });
    await page.keyboard.press("Escape");
    // One run served by the model: the plan and the completion, nothing twice.
    expect(inner.requests()).toBe(2);
    await expect(page.getByTestId("conv-failure")).toHaveCount(0);
  } finally {
    failing = false;
    await closeApp(app);
    proxy.close();
    inner.server.close();
  }
});

test("PX-062: once the agent has worked on after a restore, no redo is offered and the Core refuses the stale redo", async () => {
  test.setTimeout(300_000);
  const repo = makeRepo(mkdtempSync(join(tmpdir(), "modbit-gap-repo-")));
  const gate = new Gate();
  const plan = { call: { name: "plan.update", args: { outcome: "write three files", expected_files: ["a.txt", "b.txt", "c.txt"] } } };
  const create = (f: string) => ({ call: { name: "change.apply", args: { path: f, op: "create", content: `${f}\n` } } });
  const model = await sequencedModel([{ parts: ["Planning.", plan] }, { parts: ["A.", create("a.txt")] }, { parts: ["B.", create("b.txt")] }, { parts: ["Starting C. ", { wait: gate }, create("c.txt")] }, { parts: ["C.", create("c.txt")] }, { parts: [doneCall] }]);
  const { app, page } = await launch(mkdtempSync(join(tmpdir(), "modbit-e2e-gap-moved-")), { env: { MODBIT_OPENAI_BASE_URL: model.url } });
  try {
    await setContentSize(app, page, 1280, 800);
    const taskId = await startAndOpen(page, "three files then a restore", repo);
    const sid = await sessionOf(page);
    await expect(page.getByTestId("turn-checkpoint")).toHaveCount(3, { timeout: 120_000 });
    await expect.poll(() => model.requests(), { timeout: 60_000 }).toBe(4);
    await page.getByTestId("composer-stop").click();
    await expect(page.getByTestId("stopped-tray")).toBeVisible({ timeout: 30_000 });
    // Restore to the end of turn 2: a redo is offered while nothing has happened since.
    await page.locator('[data-testid="turn-checkpoint"][data-turn-ordinal="2"]').getByTestId("restore-turn").click();
    await expect(page.getByTestId("restore-summary")).toBeVisible({ timeout: 30_000 });
    await page.getByTestId("restore-continue").click();
    await expect(page.getByTestId("redo-bar")).toBeVisible({ timeout: 60_000 });
    const pre = await page.evaluate((t) => window.modbit.checkpoints(t).then((l) => l.checkpoints.find((c) => c.retention.includes("PRE_RESTORE"))!.checkpointId), taskId);
    // The agent goes on from the restored files: the state a redo would restore is no longer the one that follows, so the redo is gone.
    gate.open();
    await page.getByTestId("stopped-continue").click().catch(async () => page.getByTestId("conv-start").click());
    await expect(page.getByTestId("review")).toBeVisible({ timeout: 180_000 });
    await page.keyboard.press("Escape");
    await expect(page.getByTestId("redo-bar")).toHaveCount(0);
    const refused = await page.evaluate(([s, t, c]) => window.modbit.restoreCheckpoint(s!, t!, { checkpointId: c! }, { redo: true }).then((r) => (r.restored ? "restored" : `refused:${r.refusal}`), (e: Error) => `rejected:${e.message}`), [sid, taskId, pre]);
    expect(refused).toMatch(/^(refused|rejected):/);
  } finally {
    gate.open();
    await closeApp(app);
    model.server.close();
  }
});

test("PX-060: a budget change the Core does not allow is refused with its typed reason and the caps stay as they were", async () => {
  test.setTimeout(240_000);
  const repo = makeRepo(mkdtempSync(join(tmpdir(), "modbit-gap-repo-")));
  const model = await sequencedModel([]);
  const { app, page } = await launch(mkdtempSync(join(tmpdir(), "modbit-e2e-gap-budget-")), { env: { MODBIT_OPENAI_BASE_URL: model.url } });
  try {
    await setContentSize(app, page, 1280, 800);
    const taskId = await createTask(page, "A task that ends", repo);
    const sid = await sessionOf(page);
    await page.evaluate(([s, t]) => window.modbit.setTaskBudgets(s!, t!, { maxWallMs: "60000" }), [sid, taskId]);
    await page.evaluate(([s, t]) => window.modbit.cancelTask(s!, t!), [sid, taskId]);
    const refused = await page.evaluate(([s, t]) => window.modbit.setTaskBudgets(s!, t!, { maxWallMs: "99999000" }).then(() => "accepted", (e: Error) => e.message), [sid, taskId]);
    expect(refused).toContain("TASK_ENDED");
    // A cap that is not a decimal count never reaches the Core.
    const malformed = await page.evaluate(([s, t]) => window.modbit.setTaskBudgets(s!, t!, { maxWallMs: "-5" }).then(() => "accepted", (e: Error) => e.message), [sid, taskId]);
    expect(malformed).not.toBe("accepted");
    const acct = await page.evaluate((t) => window.modbit.contextAccounting(t), taskId);
    expect(acct.budgets?.maxWallMs).toBe("60000");
  } finally {
    await closeApp(app);
    model.server.close();
  }
});
