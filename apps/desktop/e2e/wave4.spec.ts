/**
 * Wave 4 combined (REQ-PX-054..058, -060, -062): one task from the composer to a
 * restored checkpoint, against a real Core, its event log, the real shell
 * effector, a real git worktree, the preload bridge and the renderer. Only the
 * provider is scripted (a sequenced OpenAI-compatible server). A Gate holds the
 * first stream until the person sends the queued message now; every other wait
 * is for a state the Core or the page reports.
 *
 * What it proves together: a message with an @ file chip and an attachment
 * reaches the model; a second message queues, is edited and sent now (the
 * stream ends USER_INTERRUPT); two approvals are decided, one from the docked
 * tray by keyboard and one from the inline conversation card, each resolving
 * exactly once with the other surface following; the tray precedence is
 * approvals over the queue; the receipts show in Evidence; the context ring
 * opens; and the turn's checkpoint is restored and redone (file bytes compared
 * on disk).
 */
import { expect, test, type Page } from "@playwright/test";
import { existsSync, mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { accessible, appDir, closeApp, git, launch, makeRepo, setContentSize } from "./support/ui-harness.ts";
import { createTask, Gate, openConversation, sequencedModel, typeAndPress } from "./support/composer-harness.ts";
import { sessionOf, sha256File, type ModelPart } from "./support/conversation-harness.ts";

const URL1 = "https://example.invalid/one.git";
const URL2 = "https://example.invalid/two.git";
const remoteAdd = (name: string, url: string): ModelPart => ({ call: { name: "shell.exec", args: { argv: ["git", "remote", "add", name, url] } } });
const create = (f: string, content: string): ModelPart => ({ call: { name: "change.apply", args: { path: f, op: "create", content } } });
const plan: ModelPart = { call: { name: "plan.update", args: { outcome: "write a.txt and b.txt and add two remotes", expected_files: ["a.txt", "b.txt"] } } };
const complete: ModelPart = { call: { name: "task.complete", args: { summary: "done", self_review: { findings: [] } } } };
const remotes = (repo: string): string[] => git(repo, "remote").split("\n").filter(Boolean);
const NAMES = ["a.txt", "b.txt", "notes.txt"] as const;
const snap = (repo: string) => Object.fromEntries(NAMES.map((n) => [n, existsSync(join(repo, n)) ? sha256File(join(repo, n)) : null]));
const FIRST = "The first answer begins here and keeps going, ";

const abortSources = (page: Page, taskId: string) =>
  page.evaluate(
    (t) =>
      window.modbit.transcript(t, { density: "DETAILED" }).then((p) => {
        const out: { phase: string; abortSource: string }[] = [];
        const walk = (rows: typeof p.rows) => {
          for (const r of rows) {
            if (r.kind === "ASSISTANT_MESSAGE" && r.facts.type === "stream") out.push({ phase: r.facts.phase, abortSource: r.facts.abortSource });
            walk(r.children);
          }
        };
        walk(p.rows);
        return out;
      }),
    taskId,
  );

const approvalKey = (id: string) => id.replace(/^(.{8})(.{4})(.{4})(.{4})(.{12})$/, "$1-$2-$3-$4-$5");

test("Wave 4: compose with an @ chip and an attachment, queue-edit-send-now, decide two approvals on both surfaces, read the receipts in Evidence, open the context ring, restore and redo a checkpoint", async () => {
  test.setTimeout(420_000);
  const repo = makeRepo(mkdtempSync(join(tmpdir(), "modbit-w4-repo-")));
  const original = sha256File(join(repo, "notes.txt"));
  const gate = new Gate();
  const model = await sequencedModel([
    { parts: [FIRST, { wait: gate }, "then it ends."] },
    { parts: ["Planning.", plan] },
    { parts: ["A.", create("a.txt", "alpha\n")] },
    { parts: ["First remote.", remoteAdd("origin1", URL1)] },
    { parts: ["Second remote.", remoteAdd("origin2", URL2)] },
    { parts: ["B.", create("b.txt", "bravo\n")] },
    { parts: [complete] },
  ]);
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-w4-"));
  const { app, page } = await launch(dataDir, { env: { MODBIT_OPENAI_BASE_URL: model.url } });
  try {
    await setContentSize(app, page, 1280, 800);
    const taskId = await createTask(page, "Wave four end to end", repo);
    await openConversation(page, taskId);

    // ---- 1. Compose: an @ file chip and an attachment, sent with Enter (the task starts from the composer).
    const input = page.getByTestId("composer-input");
    await input.fill("please read @no");
    await expect(page.getByTestId("mention-menu")).toBeVisible();
    await expect(page.locator('[data-testid="mention-option"][data-option-id="file:notes.txt"]')).toBeVisible();
    await page.keyboard.press("Enter");
    await expect(input).toHaveValue("please read @notes.txt ");
    await expect(page.getByTestId("mention-chip")).toHaveText(/file: notes\.txt/);
    await page.keyboard.type("and look at the label");
    await page.getByTestId("attach-input").setInputFiles(resolve(appDir, "..", "..", "tests", "fixtures", "media", "label.png"));
    await expect(page.getByTestId("attachment-chip")).toHaveCount(1);
    await input.press("Enter");
    await expect(page.getByTestId("conv-assistant")).toHaveAttribute("data-message-state", "streaming", { timeout: 90_000 });
    expect(model.text(0)).toContain("image_url");
    expect(model.text(0)).toContain("notes.txt");
    expect(model.text(0)).toContain("and look at the label");

    // ---- 2. While it runs: queue a second message, edit it, Send now (the stream ends as a user interrupt).
    await typeAndPress(page, "second: use spaces", "Enter");
    // The first queueing shows the first-use education with the queue, in the one tray that owns the keys.
    await expect(page.getByTestId("education-tray")).toBeVisible();
    await expect(page.getByTestId("queue-tray")).toBeVisible();
    await page.getByTestId("education-keep").click();
    await expect(page.getByTestId("education-tray")).toHaveCount(0);
    await expect(page.getByTestId("queue-tray")).toBeVisible();
    await expect(page.getByTestId("queue-header")).toHaveText("1 queued");
    // The tray host is docked directly above the composer, inside the conversation.
    const host = page.getByTestId("conversation").getByTestId("tray-host");
    await expect(host).toHaveCount(1);
    const hostBox = (await host.boundingBox())!;
    const composerBox = (await page.getByTestId("composer-region").boundingBox())!;
    expect(hostBox.y + hostBox.height, "the tray host sits above the composer").toBeLessThanOrEqual(composerBox.y + 1);
    await page.getByTestId("queue-row").first().getByTestId("queue-edit").click();
    await page.getByTestId("queue-edit-input").fill("second: use tabs");
    await page.getByTestId("queue-edit-save").click();
    await expect(page.getByTestId("queue-line")).toContainText("second: use tabs");
    await page.getByTestId("queue-send-now").click();
    await expect.poll(async () => (await abortSources(page, taskId))[0]?.abortSource, { timeout: 60_000 }).toBe("USER_INTERRUPT");
    await expect.poll(() => model.requests(), { timeout: 60_000 }).toBeGreaterThanOrEqual(2);
    expect(model.text(1)).toContain("second: use tabs");
    expect(model.text(1)).not.toContain("second: use spaces");

    // ---- 3. The first approval docks in the tray with the exact intent; nothing has run.
    const trayCard = page.getByTestId("approval-card");
    await expect(trayCard).toBeVisible({ timeout: 120_000 });
    await expect(page.getByTestId("approval-command")).toHaveText(`git remote add origin1 ${URL1}`);
    await expect(page.getByTestId("approval-tool")).toHaveText("shell.exec");
    const sid = await sessionOf(page);
    const pending1 = await page.evaluate((s) => window.modbit.dockApprovals(s).then((l) => l.map((a) => a.approvalId)), sid);
    expect(pending1.length).toBe(1);
    const id1 = pending1[0]!;
    const hash1 = (await trayCard.getAttribute("data-intent-hash"))!;
    expect(remotes(repo)).toEqual([]);
    await expect(page.getByTestId("conv-approval-decision")).toHaveAttribute("data-approval-id", id1);
    await accessible(page, "the docked approval above the composer");

    // Precedence: a message queued while an approval waits is only a one-line row; the approval keeps the keys.
    await typeAndPress(page, "third: keep this for later", "Enter");
    await expect(page.locator('[data-testid="tray-pending"]')).toContainText("Queued messages");
    await expect(page.getByTestId("queue-tray")).toHaveCount(0);
    await expect(trayCard).toBeVisible();
    // The person can bring it forward and delete it; the approval is the active tray again afterwards.
    await page.locator('[data-testid="tray-pending"]').click();
    await expect(page.getByTestId("queue-tray")).toBeVisible();
    await page.getByTestId("queue-delete").click();
    await expect(page.getByTestId("queue-tray")).toHaveCount(0);
    await expect(trayCard).toBeVisible();

    // ---- 4. Approve it from the tray by keyboard: it resolves once, and the inline card follows.
    await trayCard.focus();
    await page.keyboard.press("Enter");
    await expect.poll(() => remotes(repo), { timeout: 90_000 }).toContain("origin1");
    await expect(page.locator(`[data-testid="conv-approval-decision"][data-approval-id="${id1}"]`)).toHaveCount(0, { timeout: 30_000 });
    // A second decision for the same approval (here a Deny, from the stale surface) is the Core's idempotent report of the first: it changes nothing and no effect or receipt is added.
    const receiptsBefore = (await page.evaluate((t) => window.modbit.effectReceipts(t), taskId)).receipts.length;
    const again = await page.evaluate(([s, a, h]) => window.modbit.resolveApproval(s!, a!, false, "again", h!).then((r) => r.status, (e: Error) => e.message), [sid, id1, hash1]);
    expect(again, "the Core reports the existing resolution").toMatch(/^(APPROVED|GRANTED|RESOLVED|EXECUTED|CONSUMED)/);
    expect(again).not.toMatch(/DENIED|REQUESTED/);
    expect((await page.evaluate((t) => window.modbit.effectReceipts(t), taskId)).receipts.length).toBe(receiptsBefore);
    expect(remotes(repo).filter((r) => r === "origin1").length).toBe(1);

    // ---- 5. The second approval is decided from the inline conversation card; the tray follows.
    const inline = page.getByTestId("conv-approval-decision");
    await expect(inline).toBeVisible({ timeout: 120_000 });
    const id2 = (await inline.getAttribute("data-approval-id"))!;
    expect(id2).not.toBe(id1);
    await expect(trayCard).toHaveAttribute("data-approval-id", id2);
    await expect(page.getByTestId("approval-command")).toHaveText(`git remote add origin2 ${URL2}`);
    await page.getByTestId("conv-approve").click();
    await expect.poll(() => remotes(repo).sort(), { timeout: 90_000 }).toEqual(["origin1", "origin2"]);
    await expect(page.getByTestId("approval-card")).toHaveCount(0, { timeout: 30_000 });

    // ---- 6. The task finishes; its review opens by itself and is left.
    await expect(page.getByTestId("review")).toBeVisible({ timeout: 120_000 });
    await page.keyboard.press("Escape");
    await expect(page.getByTestId("review")).toHaveCount(0);
    await expect(page.getByTestId("conversation")).toHaveAttribute("data-task-id", taskId);
    const end = snap(repo);
    expect(end["a.txt"] && end["b.txt"], "the agent made both files").toBeTruthy();
    expect(end["notes.txt"]).toBe(original);

    // ---- 7. Evidence: each approval left exactly one authorising receipt and its result receipt.
    if ((await page.getByTestId("region-apps").count()) === 0) await page.getByTestId("toggle-apps-panel").click();
    await page.getByTestId("tab-evidence").click();
    await expect(page.getByTestId("app-evidence")).toBeVisible();
    const fromCore = await page.evaluate((t) => window.modbit.effectReceipts(t), taskId);
    for (const id of [id1, id2]) {
      const mine = fromCore.receipts.filter((r) => r.policyDecision === `approval:${approvalKey(id)}`);
      const authorised = mine.filter((r) => r.status === "AUTHORIZED");
      expect(authorised.length, `one protected effect, one authorising receipt (${id})`).toBe(1);
      expect(mine.filter((r) => r.status === "SUCCESS").length, `and its one result receipt (${id})`).toBe(1);
    }
    await expect(page.getByTestId("evidence-receipt")).toHaveCount(fromCore.receipts.length, { timeout: 30_000 });
    await expect(page.locator('[data-testid="evidence-receipt"][data-status="AUTHORIZED"]').first()).toBeVisible();
    await expect(page.locator('[data-testid="evidence-receipt"][data-status="SUCCESS"]').first()).toBeVisible();
    await expect(page.getByTestId("evidence-chain")).toHaveAttribute("data-valid", "true");

    // ---- 8. The context ring opens and its categories sum to the Core's total.
    const ring = page.getByTestId("context-ring");
    await expect(ring).toHaveAttribute("data-total", /^\d+$/, { timeout: 30_000 });
    await ring.click();
    const ctxTray = page.getByTestId("context-tray");
    await expect(ctxTray).toBeVisible();
    const total = BigInt((await ctxTray.getAttribute("data-total")) ?? "0");
    const tokens = await page.getByTestId("context-category").evaluateAll((els) => els.map((e) => e.getAttribute("data-tokens") ?? "0"));
    expect(tokens.length).toBe(9);
    expect(tokens.reduce((s, t) => s + BigInt(t), 0n)).toBe(total);
    await accessible(page, "the context breakdown");
    await ring.click();
    await expect(ctxTray).toHaveCount(0);

    // ---- 9. Restore the turn that had a.txt and not b.txt, then redo: bytes on disk.
    const markers = await page.getByTestId("turn-checkpoint").evaluateAll((els) => els.map((e) => e.getAttribute("data-turn-ordinal")));
    expect(markers.length, "every turn has a marker").toBeGreaterThanOrEqual(5);
    const target = Number(process.env.W4_RESTORE_TURN ?? "3");
    await page.locator(`[data-testid="turn-checkpoint"][data-turn-ordinal="${target}"]`).getByTestId("restore-turn").click();
    await expect(page.getByTestId("restore-summary")).toBeVisible({ timeout: 30_000 });
    const paths = await page.getByTestId("restore-file").evaluateAll((els) => els.map((e) => e.getAttribute("data-path")));
    expect(paths, "exactly what the Core's preview says changes").toEqual(["b.txt"]);
    await page.getByTestId("restore-continue").click();
    await expect(page.getByTestId("redo-bar")).toBeVisible({ timeout: 60_000 });
    expect(snap(repo)).toEqual({ "a.txt": end["a.txt"], "b.txt": null, "notes.txt": original });
    await page.getByTestId("redo-checkpoint").click();
    await expect(page.getByTestId("restore-summary")).toBeVisible({ timeout: 30_000 });
    await page.getByTestId("restore-continue").click();
    await expect(page.getByTestId("redo-bar")).toHaveCount(0, { timeout: 60_000 });
    expect(snap(repo), "redo restores the pre-restore bytes exactly").toEqual(end);
  } finally {
    gate.open();
    await closeApp(app);
    model.server.close();
  }
});
