/**
 * REQ-PX-062 (QUAL-PX-062): checkpoint restore, redo, edit in place and fork in
 * the conversation, against a real Core, a real git worktree and a scripted
 * OpenAI-compatible model. Every claim about files is a SHA-256 compare on
 * disk. The Core, its checkpoint engine, the preload bridge and the renderer
 * are real; only the provider is scripted. No test waits on a clock for a
 * state: a Gate holds the model where a test needs the run to stand still.
 */
import { expect, test, type Page } from "@playwright/test";
import { existsSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { accessible, closeApp, launch, makeRepo, MOD, setContentSize } from "./support/ui-harness.ts";
import { streamingModel } from "./support/stream-model.ts";
import { Gate } from "./support/stream-model.ts";
import { openConversation, routedModel, sessionOf, sha256File, startAndOpen, type ModelPart } from "./support/conversation-harness.ts";

const NAMES = ["a.txt", "b.txt", "c.txt", "notes.txt"] as const;
type Snap = Record<"a" | "b" | "c" | "notes", string | null>;
/** The SHA-256 of each file the task touches, null where it does not exist. */
const snap = (repo: string): Snap => Object.fromEntries(NAMES.map((n) => [n.replace(".txt", ""), existsSync(join(repo, n)) ? sha256File(join(repo, n)) : null])) as Snap;

const create = (f: string, content: string): ModelPart => ({ call: { name: "change.apply", args: { path: f, op: "create", content } } });
const editNotes: ModelPart = { call: { name: "change.apply", args: { path: "notes.txt", op: "edit", text_edits: [{ old: "line 1", new: "agent edit" }] } } };
const plan: ModelPart = { call: { name: "plan.update", args: { outcome: "write the files", expected_files: ["a.txt", "b.txt", "notes.txt", "c.txt"] } } };
const complete: ModelPart = { call: { name: "task.complete", args: { summary: "done", self_review: { findings: [] } } } };
const readNotes: ModelPart = { call: { name: "fs.read", args: { path: "notes.txt" } } };
/** Seven turns: the plan, a.txt, b.txt, a read of notes.txt (an existing file is read before it is edited), the edit, c.txt, the completion. */
const FOUR_FILES = [{ parts: ["Planning.", plan] }, { parts: ["A.", create("a.txt", "alpha\n")] }, { parts: ["B.", create("b.txt", "bravo\n")] }, { parts: ["Reading the notes.", readNotes] }, { parts: ["Notes.", editNotes] }, { parts: ["C.", create("c.txt", "charlie\n")] }, { parts: [complete] }];
const TURNS = FOUR_FILES.length;

/** Runs the task to the review (which opens by itself, as it does for a person) and leaves it. */
async function runToReview(page: Page, goal: string, repo: string): Promise<string> {
  const taskId = await startAndOpen(page, goal, repo);
  await expect(page.getByTestId("review")).toBeVisible({ timeout: 120_000 });
  await page.keyboard.press("Escape");
  await expect(page.getByTestId("review")).toHaveCount(0);
  await expect(page.getByTestId("conversation")).toHaveAttribute("data-task-id", taskId);
  await expect(page.getByTestId("turn-checkpoint")).toHaveCount(TURNS, { timeout: 60_000 });
  return taskId;
}

const marker = (page: Page, ordinal: number) => page.locator(`[data-testid="turn-checkpoint"][data-turn-ordinal="${ordinal}"]`);

test("PX-062: Restore puts the bytes of a turn's checkpoint back (Cancel changes nothing), Redo restores the exact state, Undo all needs a second click; all by hash on disk and by keyboard", async () => {
  test.setTimeout(300_000);
  const repo = makeRepo(mkdtempSync(join(tmpdir(), "modbit-ckpt-repo-")));
  const original = sha256File(join(repo, "notes.txt"));
  const model = await streamingModel(FOUR_FILES);
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-ckpt-"));
  const { app, page } = await launch(dataDir, { env: { MODBIT_OPENAI_BASE_URL: model.url } });
  try {
    await setContentSize(app, page, 1280, 800);
    const taskId = await runToReview(page, "write the four files", repo);
    const end = snap(repo);
    expect(end.a && end.b && end.c, "the agent made three files").toBeTruthy();
    expect(end.notes).not.toBe(original);

    // Restore to the end of turn 3 (a.txt and b.txt exist; c.txt and the notes edit do not yet).
    await marker(page, 3).getByTestId("restore-turn").click();
    await expect(page.getByTestId("restore-dialog")).toBeVisible();
    await expect(page.getByTestId("restore-summary")).toBeVisible({ timeout: 30_000 });
    const paths = await page.getByTestId("restore-file").evaluateAll((els) => els.map((e) => e.getAttribute("data-path")));
    expect(paths.sort(), "exactly the files the Core's preview says it will change").toEqual(["c.txt", "notes.txt"]);
    await expect(page.getByTestId("restore-intro")).toContainText("end of turn 3");
    await accessible(page, "the restore dialog");
    // Cancel changes nothing.
    await page.getByTestId("restore-cancel").click();
    await expect(page.getByTestId("restore-dialog")).toHaveCount(0);
    expect(snap(repo)).toEqual(end);
    expect(await page.getByTestId("redo-bar").count()).toBe(0);

    // Continue restores: the bytes are the checkpoint's.
    await marker(page, 3).getByTestId("restore-turn").click();
    await expect(page.getByTestId("restore-summary")).toBeVisible({ timeout: 30_000 });
    const before = await page.evaluate((t) => window.modbit.checkpoints(t).then((l) => l.checkpoints.filter((c) => c.retention.includes("PRE_RESTORE")).length), taskId);
    // A Continue clicked twice restores once: the second click finds the dialog busy or gone, and one pre-restore state is recorded.
    await page.getByTestId("restore-continue").dblclick();
    await expect(page.getByTestId("redo-bar")).toBeVisible({ timeout: 60_000 });
    expect(snap(repo)).toEqual({ a: end.a, b: end.b, c: null, notes: original });
    expect(await page.evaluate((t) => window.modbit.checkpoints(t).then((l) => l.checkpoints.filter((c) => c.retention.includes("PRE_RESTORE")).length), taskId)).toBe(before + 1);
    await expect(page.getByTestId("checkpoint-note")).toContainText("Restored 2 files");

    // Redo, by keyboard alone: it is worded as a redo of the exact pre-restore state, and it restores that state exactly.
    await page.getByTestId("redo-checkpoint").focus();
    await page.keyboard.press("Enter");
    await expect(page.getByTestId("restore-body")).toHaveAttribute("data-redo", "true");
    await expect(page.getByTestId("restore-intro")).toContainText("exactly the state it was in just before you restored");
    await expect(page.getByTestId("restore-summary")).toBeVisible({ timeout: 30_000 });
    await accessible(page, "the redo dialog");
    await page.keyboard.press("Tab");
    await expect(page.getByTestId("restore-continue")).toBeFocused();
    await page.keyboard.press("Enter");
    await expect(page.getByTestId("redo-bar")).toHaveCount(0, { timeout: 60_000 });
    expect(snap(repo), "redo restores the pre-restore state exactly").toEqual(end);

    // Undo all needs a second click: the first one arms it and says what it will do; nothing changes until the second.
    const undo = page.getByTestId("undo-all");
    await expect(undo).toHaveAttribute("data-armed", "false");
    await undo.click();
    await expect(undo).toHaveAttribute("data-armed", "true");
    await expect(page.getByTestId("undo-all-armed")).toContainText("4 files are put back to the last commit");
    expect(snap(repo)).toEqual(end);
    await accessible(page, "Undo all, armed");
    await undo.click();
    await expect.poll(() => snap(repo), { timeout: 60_000 }).toEqual({ a: null, b: null, c: null, notes: original });
    // And it is redone like any restore.
    await expect(page.getByTestId("redo-bar")).toBeVisible({ timeout: 60_000 });
    await page.getByTestId("redo-checkpoint").click();
    await expect(page.getByTestId("restore-summary")).toBeVisible({ timeout: 30_000 });
    await page.getByTestId("restore-continue").click();
    await expect(page.getByTestId("redo-bar")).toHaveCount(0, { timeout: 60_000 });
    expect(snap(repo)).toEqual(end);
  } finally {
    await closeApp(app);
    model.server.close();
  }
});

test("PX-062: a file the person edited needs their own choice and is kept; a renderer killed with the dialog open leaves the workspace unchanged; a restore is refused while the agent works", async () => {
  test.setTimeout(300_000);
  const repo = makeRepo(mkdtempSync(join(tmpdir(), "modbit-ckpt-repo-")));
  const original = sha256File(join(repo, "notes.txt"));
  const gate = new Gate();
  const model = await streamingModel([...FOUR_FILES.slice(0, TURNS - 1), { parts: [{ wait: gate }, complete] }]);
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-ckpt-edit-"));
  const { app, page } = await launch(dataDir, { env: { MODBIT_OPENAI_BASE_URL: model.url } });
  try {
    await setContentSize(app, page, 1280, 800);
    const taskId = await startAndOpen(page, "write the four files", repo);
    // While the agent works the controls are off, with the reason; the Core would refuse the restore too.
    await expect(page.getByTestId("turn-checkpoint")).toHaveCount(TURNS - 1, { timeout: 120_000 });
    await expect(marker(page, 2).getByTestId("restore-turn")).toBeDisabled();
    await expect(marker(page, 2).getByTestId("restore-turn")).toHaveAttribute("title", /agent is working/);
    const sid = await sessionOf(page);
    const refused = await page.evaluate(([s, t]) => window.modbit.restoreCheckpoint(s!, t!, { turnOrdinal: 2 }, {}).then((r) => (r.restored ? "restored" : r.refusal), (e: Error) => e.message), [sid, taskId]);
    expect(refused).toContain("TASK_RUNNING");
    gate.open();
    await expect(page.getByTestId("review")).toBeVisible({ timeout: 120_000 });
    await page.keyboard.press("Escape");
    await expect(page.getByTestId("turn-checkpoint")).toHaveCount(TURNS, { timeout: 60_000 });
    const end = snap(repo);

    // The person edits notes.txt by hand after the agent's last turn.
    writeFileSync(join(repo, "notes.txt"), "my own edit\n");
    const mine = sha256File(join(repo, "notes.txt"));

    // A renderer killed with the dialog open: nothing was written.
    await marker(page, 2).getByTestId("restore-turn").click();
    await expect(page.getByTestId("restore-edited")).toBeVisible({ timeout: 30_000 });
    await page.reload();
    await expect(page.getByTestId("core-status")).toContainText("Core connected", { timeout: 60_000 });
    expect(snap(repo)).toEqual({ ...end, notes: mine });
    await openConversation(page, taskId);
    await expect(page.getByTestId("restore-dialog")).toHaveCount(0);

    // The edited file blocks Continue until the person chooses, and there is no do-not-ask-again.
    await marker(page, 2).getByTestId("restore-turn").click();
    await expect(page.getByTestId("restore-edited-file")).toHaveAttribute("data-path", "notes.txt", { timeout: 30_000 });
    await expect(page.getByTestId("restore-continue")).toBeDisabled();
    await expect(page.getByTestId("restore-dialog")).not.toContainText(/ask again/i);
    await accessible(page, "the restore dialog with a file the person edited");
    await page.getByTestId("restore-keep").check();
    await expect(page.getByTestId("restore-continue")).toBeEnabled();
    await page.getByTestId("restore-continue").click();
    await expect(page.getByTestId("redo-bar")).toBeVisible({ timeout: 60_000 });
    // b.txt and c.txt (the agent's later files) are gone; the person's edit is untouched.
    expect(snap(repo)).toEqual({ a: end.a, b: null, c: null, notes: mine });
    await expect(page.getByTestId("checkpoint-note")).toContainText("kept 1 you edited");
  } finally {
    gate.open();
    await closeApp(app);
    model.server.close();
  }
});

test("PX-062: Fork from a turn opens the child task with that turn's files and leaves the parent alone", async () => {
  test.setTimeout(300_000);
  const repo = makeRepo(mkdtempSync(join(tmpdir(), "modbit-ckpt-repo-")));
  const model = await streamingModel(FOUR_FILES);
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-ckpt-fork-"));
  const { app, page } = await launch(dataDir, { env: { MODBIT_OPENAI_BASE_URL: model.url } });
  try {
    await setContentSize(app, page, 1280, 800);
    const parent = await runToReview(page, "write the four files", repo);
    const end = snap(repo);
    await marker(page, 3).getByTestId("fork-turn").click();
    // The child's conversation opens.
    await expect(page.getByTestId("conversation")).not.toHaveAttribute("data-task-id", parent, { timeout: 60_000 });
    const child = (await page.getByTestId("conversation").getAttribute("data-task-id"))!;
    const sid = await sessionOf(page);
    const header = await page.evaluate(([s, c]) => window.modbit.agentHeaders(s!).then((h) => h.headers.find((x) => x.taskId === c) ?? null), [sid, child]);
    expect(header, "the child is a task of the session").not.toBeNull();
    expect(header!.workspaceRoot).not.toBe(repo);
    // The child's worktree has turn 3's files and not what came after.
    const root = header!.workspaceRoot;
    expect(existsSync(join(root, "a.txt")) && existsSync(join(root, "b.txt")), "turn 3's files").toBe(true);
    expect(sha256File(join(root, "a.txt"))).toBe(end.a);
    expect(sha256File(join(root, "b.txt"))).toBe(end.b);
    expect(existsSync(join(root, "c.txt")), "what came after turn 3 is not in the child").toBe(false);
    expect(sha256File(join(root, "notes.txt")), "the notes edit came after turn 3").not.toBe(end.notes);
    // The parent is untouched.
    expect(snap(repo)).toEqual(end);
    // A checkpoint of one task cannot be restored into another: the Core refuses it, and nothing is written to either workspace.
    const parentCheckpoint = await page.evaluate((t) => window.modbit.checkpoints(t).then((l) => l.checkpoints.find((c) => c.turnOrdinal === 2)!.checkpointId), parent);
    const crossed = await page.evaluate(([s, c, cp]) => window.modbit.restoreCheckpoint(s!, c!, { checkpointId: cp! }, {}).then((r) => (r.restored ? "restored" : r.refusal), (e: Error) => e.message), [sid, child, parentCheckpoint]);
    expect(crossed).not.toBe("restored");
    expect(crossed).not.toBe("");
    expect(snap(repo)).toEqual(end);
    expect(existsSync(join(root, "c.txt"))).toBe(false);
    await accessible(page, "the forked task's conversation");
  } finally {
    await closeApp(app);
    model.server.close();
  }
});

test("PX-062: Restore checkpoint on a message makes it editable in place; editing an older message asks Revert or Keep; Redo returns the files", async () => {
  test.setTimeout(420_000);
  const repo = makeRepo(mkdtempSync(join(tmpdir(), "modbit-ckpt-repo-")));
  const reached = new Gate();
  const release = new Gate();
  let held = false;
  const STEER = "please also write c.txt (STEERED)";
  const model = await routedModel(async (_goal, results, body): Promise<ModelPart[]> => {
    if (results === 0) return ["Planning.", plan];
    if (results === 1) return ["A.", create("a.txt", "alpha\n")];
    if (results === 2) {
      if (!held) {
        held = true;
        reached.open();
        await release.opened;
      }
      return ["B.", create("b.txt", "bravo\n")];
    }
    if (results === 3 && body.includes("(STEERED)")) return ["C.", create("c.txt", "charlie\n")];
    if (results <= 4) return ["Waiting on you.", { call: { name: "user.ask", args: { question: "Anything else?", options: [{ id: "no", label: "no, that is all" }], reason: "protected_effect" } } }];
    return ["done"];
  });
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-ckpt-msg-"));
  const { app, page } = await launch(dataDir, { env: { MODBIT_OPENAI_BASE_URL: model.url } });
  try {
    await setContentSize(app, page, 1280, 800);
    const taskId = await startAndOpen(page, "write the files", repo);
    await reached.opened;
    // A second message, said while the run is on its way: the agent takes it into the next turn.
    // Wave 4 (deliberate change to this test): the steer box is the composer now; plain Enter queues behind the run, the primary-modifier Enter steers (AFW-E05).
    // The composer has no "steered" note; the send is taken when the box clears, and the transcript assertions below prove the message arrived once.
    await page.getByTestId("composer-input").fill(STEER);
    await page.getByTestId("composer-input").press(`${MOD}+Enter`);
    await expect(page.getByTestId("composer-input")).toHaveValue("");
    release.open();
    await expect(page.getByTestId("conv-question")).toBeVisible({ timeout: 120_000 });
    await expect.poll(() => snap(repo).c, { timeout: 60_000 }).not.toBeNull();
    const end = snap(repo);
    expect(end.a && end.b && end.c).toBeTruthy();
    const users = page.getByTestId("conv-user");
    await expect(users).toHaveCount(2);
    const steered = users.nth(1);
    await expect(steered).toContainText(STEER);

    // The first message has no checkpoint before it, so it offers Edit but not Restore.
    await users.nth(0).hover();
    await expect(users.nth(0).getByTestId("restore-checkpoint")).toHaveCount(0);

    // Restore checkpoint on the second message: the files are as they were when it was sent (a.txt only).
    await steered.hover();
    await steered.getByTestId("restore-checkpoint").click();
    await expect(page.getByTestId("restore-intro")).toContainText("before your message");
    await expect(page.getByTestId("restore-summary")).toBeVisible({ timeout: 30_000 });
    await page.getByTestId("restore-continue").click();
    await expect(page.getByTestId("redo-bar")).toBeVisible({ timeout: 60_000 });
    expect(snap(repo)).toEqual({ a: end.a, b: null, c: null, notes: end.notes });
    // The message is now editable where it stands, with its own text, and nothing is sent until the person sends.
    await expect(page.getByTestId("edit-text")).toHaveValue(STEER);
    await accessible(page, "a message being edited in place");
    await page.getByTestId("edit-text").press("Escape");
    await expect(page.getByTestId("edit-box")).toHaveCount(0);
    await expect(users).toHaveCount(2);

    // Redo brings the files back byte for byte.
    await page.getByTestId("redo-checkpoint").click();
    await expect(page.getByTestId("restore-summary")).toBeVisible({ timeout: 30_000 });
    await page.getByTestId("restore-continue").click();
    await expect(page.getByTestId("redo-bar")).toHaveCount(0, { timeout: 60_000 });
    expect(snap(repo)).toEqual(end);

    // Editing the older message while the files differ from its checkpoint asks Revert or Keep. Keep (Shift+Enter) sends it and leaves the files.
    await steered.hover();
    await steered.getByTestId("edit-message").click();
    await page.getByTestId("edit-text").fill("please also write d.txt instead");
    await page.getByTestId("edit-send").click();
    await expect(page.getByTestId("edit-files-dialog")).toBeVisible();
    await expect(page.getByTestId("edit-files-text")).toContainText("differ from how it was when you sent this message");
    await accessible(page, "Revert or keep the files");
    await page.keyboard.press("Shift+Enter");
    await expect(page.getByTestId("edit-files-dialog")).toHaveCount(0);
    await expect(users).toHaveCount(3, { timeout: 30_000 });
    await expect(users.nth(2)).toContainText("d.txt instead");
    expect(snap(repo), "Keep leaves the files as they were").toEqual(end);

    // Revert restores the message's checkpoint first, then sends the edit.
    await steered.hover();
    await steered.getByTestId("edit-message").click();
    await page.getByTestId("edit-text").fill("and a different edit");
    await page.getByTestId("edit-send").click();
    await page.getByTestId("edit-files-revert").click();
    await expect(page.getByTestId("restore-summary")).toBeVisible({ timeout: 30_000 });
    await page.getByTestId("restore-continue").click();
    await expect(users).toHaveCount(4, { timeout: 60_000 });
    expect(snap(repo)).toEqual({ a: end.a, b: null, c: null, notes: end.notes });
    void taskId;
  } finally {
    release.open();
    await closeApp(app);
    model.server.close();
  }
});
