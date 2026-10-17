/**
 * REQ-PX-048 / QUAL-PX-048, the Changes tab's actions, on the real stack: the
 * packaged Electron app, a real Core over its socket, a real Git checkout with
 * a real bare remote, and a scripted OpenAI-compatible model that makes the
 * change set (the repo's standard model stand-in). In order, on one app:
 * revert one hunk (and undo it from the checkpoint the Core took first), a
 * stale revert refused, stage and unstage on the real index, a dirty conflict,
 * Commit and push asking for an approval that shows the exact intent and
 * running only after it, a denial, and the Core killed in the middle of a push
 * with the effect shown as in doubt after the restart and a retry converging.
 *
 * Every wait is for a condition the Core, the page or the git repository
 * reports; nothing sleeps for a fixed time.
 */
import { expect, test, type ElectronApplication, type Page } from "@playwright/test";
import { createHash } from "node:crypto";
import { mkdirSync, mkdtempSync, readFileSync, realpathSync, writeFileSync } from "node:fs";
import { createServer, type Server } from "node:http";
import type { Socket } from "node:net";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { accessible, closeApp, git, launch, MOD, scriptedModel, setContentSize } from "./support/ui-harness.ts";

test.describe.configure({ mode: "serial" });

const BASE = Array.from({ length: 24 }, (_, i) => `line ${i + 1}`).join("\n") + "\n";
const EDITED = BASE.replace("line 2\n", "line 2 changed\n").replace("line 22\n", "line 22 changed\n");
const FRESH = "brand new file\n";

let app: ElectronApplication;
let page: Page;
let model: { server: Server; url: string };
let repo: string;
let remote: string;
let dataDir: string;
let sessionId = "";
let taskId = "";
const hanging: { server?: Server; sockets: Socket[]; seen?: Promise<void>; url?: string } = { sockets: [] };

const text = (p: string) => readFileSync(join(repo, p), "utf8").replace(/\r\n/g, "\n");
const g = (...args: string[]) => git(repo, ...args).trim();
const rg = (...args: string[]) => git(remote, ...args).trim();
const rows = () => page.getByTestId("changes-row");
const row = (path: string) => page.locator(`[data-testid="changes-row"][data-path="${path}"]`);
/** The person edits a file outside the app and the app reads the Core again: waits until the row shows the new content's revision. */
async function editOutside(path: string, content: string): Promise<void> {
  writeFileSync(join(repo, path), content);
  await page.getByTestId("changes-refresh").click();
  await expect(row(path)).toHaveAttribute("data-file-revision", createHash("sha256").update(content).digest("hex"));
}

/** Toggle a row's stage box and wait for the Core's answer to show in the row (the box is the Core's state, so it flips when the Core says so). */
async function setStaged(path: string, staged: boolean): Promise<void> {
  await row(path).getByTestId("changes-stage").click();
  await expect(row(path)).toHaveAttribute("data-staged", staged ? "staged" : "none");
}
const select = (path: string) => page.locator(`[data-testid="changes-file"][data-path="${path}"]`).click();

test.beforeAll(async () => {
  repo = realpathSync(mkdtempSync(join(tmpdir(), "modbit-changes-repo-")));
  remote = realpathSync(mkdtempSync(join(tmpdir(), "modbit-changes-remote-")));
  writeFileSync(join(repo, "notes.txt"), BASE);
  writeFileSync(join(repo, "other.txt"), "other\n");
  writeFileSync(join(repo, ".env"), "SECRET=1\n");
  git(repo, "init", "-q", "-b", "main");
  git(repo, "config", "core.autocrlf", "false");
  mkdirSync(join(repo, ".modbit"), { recursive: true });
  writeFileSync(join(repo, ".modbit", "verification.json"), JSON.stringify({ commands: [{ id: "fixture-noop", argv: ["git", "--version"] }] }));
  git(repo, "add", "-A");
  git(repo, "-c", "user.name=t", "-c", "user.email=t@e", "commit", "-q", "-m", "base");
  git(remote, "init", "-q", "--bare", "-b", "main");
  git(repo, "remote", "add", "origin", remote);
  git(repo, "push", "-q", "origin", "main");
  // The identity a commit carries comes from the repository, as for the person.
  git(repo, "config", "user.name", "Test Person");
  git(repo, "config", "user.email", "person@example.com");

  model = await scriptedModel([
    { calls: [{ name: "plan.update", args: { outcome: "edit notes", expected_files: ["notes.txt", "fresh.txt"] } }] },
    { calls: [{ name: "fs.read", args: { path: "notes.txt" } }] },
    { calls: [{ name: "change.apply", args: { path: "notes.txt", op: "replace", content: EDITED } }] },
    { calls: [{ name: "change.apply", args: { path: "fresh.txt", op: "replace", content: FRESH } }] },
    { calls: [{ name: "task.complete", args: { summary: "edited", self_review: { findings: [] } } }] },
  ]);
  dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-changes-"));
  ({ app, page } = await launch(dataDir, { env: { MODBIT_OPENAI_BASE_URL: model.url } }));
  await setContentSize(app, page, 1280, 800);
});

test.afterAll(async () => {
  for (const s of hanging.sockets) s.destroy();
  hanging.server?.close();
  if (app) await closeApp(app);
  model?.server.close();
});

test("the Changes tab shows the finished task's change set with its index, branch and remote", async () => {
  await page.getByTestId("goal").fill("Edit the notes");
  await page.getByTestId("workspace").fill(repo);
  await page.getByTestId("run").click();
  const card = page.getByTestId("task-card").first();
  taskId = (await card.getAttribute("data-task-id"))!;
  sessionId = (await page.evaluate(() => window.modbit.localState()).then((s) => s.sessionId))!;
  await card.getByTestId("task-start").click();
  await expect(page.getByTestId("column-readyForReview").getByTestId("task-card")).toHaveCount(1, { timeout: 90_000 });
  // The first result opens the Review itself; the Changes tab is one keystroke from the task.
  await expect(page.getByTestId("review")).toBeVisible({ timeout: 15_000 });
  await page.getByTestId("review-close").click();
  await page.getByTestId("task-card").first().focus();
  await page.keyboard.press(`${MOD}+e`);
  await expect(page.getByTestId("app-changes")).toHaveAttribute("data-task-state", "ReadyForReview");
  await expect(rows()).toHaveCount(2);
  await expect(page.getByTestId("changes-branch")).toContainText("On main → origin");
  await expect(page.getByTestId("changes-branch")).toHaveAttribute("data-unpushed", "0");
  await select("notes.txt");
  await expect(page.getByTestId("changes-hunk")).toHaveCount(2);
  await expect(page.getByTestId("changes-commit")).toHaveAttribute("data-mode", "none");
  await expect(page.getByTestId("changes-commit-push")).toBeDisabled();
  await accessible(page, "changes app with its actions", '[data-testid="region-apps"]');
});

test("reverting one hunk goes through the Core, leaves the other, and a checkpoint undoes it", async () => {
  await select("notes.txt");
  const workspaceRevision = await page.getByTestId("app-changes").getAttribute("data-workspace-revision");
  await page.locator('[data-testid="changes-hunk"][data-index="0"]').getByTestId("changes-revert-hunk").click();
  await expect(page.getByTestId("changes-undo")).toBeVisible();
  await expect(page.getByTestId("changes-hunk")).toHaveCount(1);
  // The file on disk: the first edit is gone, the second stays. The Core did the write, one revision on.
  expect(text("notes.txt")).toBe(BASE.replace("line 22\n", "line 22 changed\n"));
  await expect(page.getByTestId("app-changes")).not.toHaveAttribute("data-workspace-revision", workspaceRevision!);
  const checkpoint = await page.getByTestId("changes-undo").getAttribute("data-checkpoint");
  expect(checkpoint).toMatch(/^[0-9a-f-]{32,36}$/);
  // The undo is the Core's restore of the checkpoint taken before the write.
  await page.getByTestId("changes-undo-button").click();
  await expect(page.getByTestId("changes-undo")).toHaveCount(0);
  await expect(page.getByTestId("changes-hunk")).toHaveCount(2);
  expect(text("notes.txt")).toBe(EDITED);
});

test("a revert of a file edited since the view was read is refused as stale and writes nothing", async () => {
  await select("notes.txt");
  const sneaky = EDITED.replace("line 12\n", "line 12 edited behind the app's back\n");
  writeFileSync(join(repo, "notes.txt"), sneaky);
  await page.locator('[data-testid="changes-hunk"][data-index="0"]').getByTestId("changes-revert-hunk").click();
  await expect(page.getByTestId("changes-refusal")).toHaveAttribute("data-code", "STALE_REVISION");
  expect(text("notes.txt"), "the stale revert wrote nothing").toBe(sneaky);
  await expect(page.getByTestId("changes-undo")).toHaveCount(0);
  // Reloading shows the file as it is now; a revert of the whole file then goes through.
  await page.getByTestId("changes-refusal-reload").click();
  await expect(page.getByTestId("changes-refusal")).toHaveCount(0);
  await expect(page.getByTestId("changes-hunk")).toHaveCount(3);
  writeFileSync(join(repo, "notes.txt"), EDITED);
  await page.getByTestId("changes-refresh").click();
  await expect(page.getByTestId("changes-hunk")).toHaveCount(2);
});

test("staging moves the real git index; a staged path cannot be reverted under it", async () => {
  expect(g("diff", "--cached", "--name-only")).toBe("");
  await setStaged("notes.txt", true);
  expect(g("diff", "--cached", "--name-only")).toBe("notes.txt");
  // A dirty conflict: the revert would fight the index, so the Core refuses and nothing moves.
  await row("notes.txt").getByTestId("changes-revert-file").click();
  await expect(page.getByTestId("changes-refusal")).toHaveAttribute("data-code", "DIRTY_CONFLICT");
  expect(text("notes.txt")).toBe(EDITED);
  await page.getByTestId("changes-refusal-reload").click();
  await setStaged("notes.txt", false);
  expect(g("diff", "--cached", "--name-only")).toBe("");
  // Whole-file revert of the new file deletes it; of the edited one restores HEAD.
  await row("fresh.txt").getByTestId("changes-revert-file").click();
  await expect(rows()).toHaveCount(1);
  expect(() => readFileSync(join(repo, "fresh.txt"))).toThrow();
  await page.getByTestId("changes-undo-button").click();
  await expect(rows()).toHaveCount(2);
  expect(text("fresh.txt")).toBe(FRESH);
});

test("Commit and push asks for an approval that shows the exact intent, and runs only after it", async () => {
  const head0 = g("rev-parse", "HEAD");
  await setStaged("notes.txt", true);
  await setStaged("fresh.txt", true);
  await expect(page.getByTestId("changes-commit")).toHaveAttribute("data-mode", "commit");
  await expect(page.getByTestId("changes-commit-push")).toBeDisabled();
  await page.getByTestId("changes-commit-message").fill("Change the notes and add a file");
  await page.getByTestId("changes-commit-push").click();

  // The card: the message, the files, the branch and the remote, and nothing has run.
  const approval = page.getByTestId("changes-approval");
  await expect(approval).toBeVisible();
  const intentHash = (await approval.getAttribute("data-intent-hash"))!;
  await expect(page.getByTestId("changes-approval-message")).toHaveText("Change the notes and add a file");
  await expect(page.getByTestId("changes-approval-files")).toContainText("fresh.txt");
  await expect(page.getByTestId("changes-approval-files")).toContainText("notes.txt");
  await expect(page.getByTestId("changes-approval-branch")).toHaveText("main");
  await expect(page.getByTestId("changes-approval-remote")).toHaveText("origin");
  await expect(page.getByTestId("changes-approval-intent")).toHaveText(intentHash.slice(0, 12));
  expect(g("rev-parse", "HEAD"), "no commit before the approval").toBe(head0);
  expect(rg("rev-parse", "main"), "no push before the approval").toBe(head0);
  await accessible(page, "the commit and push approval", '[data-testid="changes-approval"]');
  // The Core lists it as a pending approval for exactly this intent.
  const pending = await page.evaluate((s) => window.modbit.dockApprovals(s), sessionId);
  expect(pending.map((a) => a.toolName)).toContain("git.changes.commit_push");

  await page.getByTestId("changes-approve").click();
  await expect(page.getByTestId("changes-result")).toHaveAttribute("data-status", "COMPLETED", { timeout: 60_000 });
  const commit = g("rev-parse", "HEAD");
  expect(commit).not.toBe(head0);
  await expect(page.getByTestId("changes-result-commit")).toHaveText(commit.slice(0, 12));
  expect(rg("rev-parse", "main"), "the bare remote holds the commit").toBe(commit);
  expect(g("log", "-1", "--format=%s")).toBe("Change the notes and add a file");
  expect(g("log", "-1", "--format=%an <%ae>")).toBe("Test Person <person@example.com>");
  expect(g("show", "--name-only", "--format=", "HEAD").split("\n").sort()).toEqual(["fresh.txt", "notes.txt"]);
  // One protected effect: an AUTHORIZED receipt before it and a result receipt after it, shown in the app and on the log.
  await expect(page.getByTestId("changes-effect").first()).toHaveAttribute("data-state", "COMPLETED");
  await expect(page.getByTestId("changes-effect").first()).toHaveAttribute("data-has-result-receipt", "true");
  const receipts = await page.evaluate((t) => window.modbit.effectReceipts(t), taskId);
  const mine = receipts.receipts.filter((r) => r.intentHash === intentHash);
  expect(mine.length, "one protected effect, an authorization and a result").toBe(2);
  expect(mine.filter((r) => r.status === "AUTHORIZED")).toHaveLength(1);
  expect(mine.find((r) => r.status !== "AUTHORIZED")?.status).toBeTruthy();
  await expect(page.getByTestId("changes-branch")).toHaveAttribute("data-unpushed", "0");
  await page.getByTestId("changes-result-dismiss").click();
});

test("a denied approval commits and pushes nothing", async () => {
  const head = g("rev-parse", "HEAD");
  await editOutside("other.txt", "other, changed by the person\n");
  await setStaged("other.txt", true);
  await page.getByTestId("changes-commit-message").fill("This will be denied");
  await page.getByTestId("changes-commit-push").click();
  await expect(page.getByTestId("changes-approval")).toBeVisible();
  await page.getByTestId("changes-deny").click();
  await expect(page.getByTestId("changes-result")).toHaveAttribute("data-status", "DENIED");
  await expect(page.locator('[data-testid="changes-effect"][data-state="DENIED"]')).toHaveCount(1);
  expect(g("rev-parse", "HEAD")).toBe(head);
  expect(rg("rev-parse", "main")).toBe(head);
  await page.getByTestId("changes-result-dismiss").click();
  await setStaged("other.txt", false);
});

test("a protected path is refused by the Core, not hidden by the app", async () => {
  // The bridge is the only way a renderer reaches the Core: a protected file named through it is refused by the Core's path policy.
  const refused = await page.evaluate(
    async ([s, t]) => {
      const out: Record<string, string> = {};
      const ask = async (k: string, f: () => Promise<unknown>) => {
        try {
          await f();
          out[k] = "accepted";
        } catch (e) {
          out[k] = (e as Error).message;
        }
      };
      await ask("revert", () => window.modbit.revertChanges(s!, t!, { path: ".env", wholeFile: true, expectedFileRevision: "" }));
      await ask("stage", () => window.modbit.stageChanges(s!, t!, [{ path: ".env", expectedFileRevision: "" }], true));
      return out;
    },
    [sessionId, taskId],
  );
  expect(refused.revert).toMatch(/PROTECTED_PATH|STALE_REVISION|NO_SUCH_CHANGE/);
  expect(refused.stage).toMatch(/PROTECTED_PATH|STALE_REVISION|NO_SUCH_CHANGE/);
  expect(g("diff", "--cached", "--name-only")).toBe("");
});

test("the Core killed in the middle of a push: the effect is shown as in doubt after the restart, and the retry converges", async () => {
  // The remote is a server that takes the push's connection and never answers.
  let reached!: () => void;
  hanging.seen = new Promise<void>((r) => (reached = r));
  hanging.server = createServer((req) => {
    if (req.url?.includes("git-receive-pack")) reached();
  });
  hanging.server.on("connection", (s) => hanging.sockets.push(s));
  await new Promise<void>((r) => hanging.server!.listen(0, "127.0.0.1", r));
  hanging.url = `http://127.0.0.1:${(hanging.server.address() as { port: number }).port}/repo.git`;
  const before = rg("rev-parse", "main");
  git(repo, "remote", "set-url", "origin", hanging.url);

  await editOutside("other.txt", "other, pushed into the void\n");
  await setStaged("other.txt", true);
  await page.getByTestId("changes-commit-message").fill("Pushed into the void");
  await page.getByTestId("changes-commit-push").click();
  await expect(page.getByTestId("changes-approval")).toBeVisible();
  await expect(page.getByTestId("changes-approval-remote-url")).toContainText("127.0.0.1");
  const intent = (await page.getByTestId("changes-approval").getAttribute("data-intent-hash"))!;
  await page.getByTestId("changes-approve").click();

  // The server saw the push (the signal), then the Core is killed.
  await hanging.seen;
  const info = await page.evaluate(() => window.modbit.debugCoreInfo());
  process.kill(info!.pid, "SIGKILL");
  await expect(page.getByTestId("core-status")).not.toContainText(`pid ${info!.pid}`, { timeout: 90_000 });
  await expect(page.getByTestId("core-status")).toContainText("Core connected", { timeout: 90_000 });
  for (const s of hanging.sockets) s.destroy();

  // After the restart: the commit exists locally, the remote never got it, and the effect is authorized with no result.
  const effect = page.locator(`[data-testid="changes-effect"][data-state="AUTHORIZED_IN_DOUBT"]`);
  await expect(async () => {
    await page.getByTestId("changes-refresh").click({ timeout: 5_000 });
    await expect(effect).toHaveCount(1, { timeout: 5_000 });
  }).toPass({ timeout: 90_000 });
  await expect(effect).toHaveAttribute("data-has-result-receipt", "false");
  await expect(page.getByTestId("changes-effect-words").last()).toContainText("unknown");
  expect(g("log", "-1", "--format=%s")).toBe("Pushed into the void");
  expect(rg("rev-parse", "main"), "the remote never received it").toBe(before);
  const receipts = await page.evaluate((t) => window.modbit.effectReceipts(t), taskId);
  const forIntent = receipts.receipts.filter((r) => r.intentHash === intent);
  expect(forIntent.map((r) => r.status), "an authorization and no result").toEqual(["AUTHORIZED"]);
  await expect(page.getByTestId("changes-branch")).toHaveAttribute("data-unpushed", "1");

  // The retry is a push alone, a new call with its own approval, and it converges on the one commit.
  git(repo, "remote", "set-url", "origin", remote);
  await page.getByTestId("changes-refresh").click();
  await expect(page.getByTestId("changes-commit-push")).toHaveText("Push 1 commit");
  await page.getByTestId("changes-commit-push").click();
  await expect(page.getByTestId("changes-approval")).toBeVisible();
  expect(await page.getByTestId("changes-approval").getAttribute("data-intent-hash")).not.toBe(intent);
  await page.getByTestId("changes-approve").click();
  await expect(page.getByTestId("changes-result")).toHaveAttribute("data-status", "COMPLETED", { timeout: 60_000 });
  expect(rg("rev-parse", "main")).toBe(g("rev-parse", "HEAD"));
  expect(g("rev-list", "--count", `${before}..HEAD`), "one commit was made, not two").toBe("1");
  await expect(page.getByTestId("changes-branch")).toHaveAttribute("data-unpushed", "0");
});
