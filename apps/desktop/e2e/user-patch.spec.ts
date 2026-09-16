/**
 * PX-005 / PX-E2E-005 (docs/20 "Constrained inline patch", docs/29): from the
 * review surface a person applies a one-hunk edit to the real worktree through
 * the Core's ChangeTransaction only — the revision the review showed is the
 * precondition, provenance `user_direct_edit` lands on the Core's log, the
 * workspace revision advances once — then a stale attempt and a protected
 * path are refused by the Core, and no client keeps an unsaved buffer: the
 * panel's text is gone after the edit and after a reload, and the file on
 * disk is exactly what the Core applied.
 */
import { _electron as electron, expect, test, type ElectronApplication, type Page } from "@playwright/test";
import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { createServer, type Server } from "node:http";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const appDir = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const coreBin = process.env.MODBIT_CORE_BIN ?? resolve(appDir, "..", "..", "target", "debug", process.platform === "win32" ? "modbit-core.exe" : "modbit-core");
const cliBin = process.env.MODBIT_CLI_BIN ?? resolve(dirname(coreBin), process.platform === "win32" ? "modbit-cli.exe" : "modbit-cli");

function scriptedModel(script: { text?: string; calls?: { name: string; args: unknown }[] }[]): Promise<{ server: Server; url: string }> {
  return new Promise((resolveServer) => {
    const server = createServer((req, res) => {
      let body = "";
      req.on("data", (c) => (body += c));
      req.on("end", () => {
        const parsed = JSON.parse(body || "{}") as { messages?: { role: string }[] };
        const results = (parsed.messages ?? []).filter((m) => m.role === "tool").length;
        const reply = script[results] ?? { text: "Nothing further." };
        res.writeHead(200, { "content-type": "text/event-stream" });
        const send = (o: unknown) => res.write(`data: ${JSON.stringify(o)}\n\n`);
        if (reply.text) send({ id: "c", model: "scripted", choices: [{ index: 0, delta: { content: reply.text }, finish_reason: null }] });
        (reply.calls ?? []).forEach((c, i) => send({ id: "c", model: "scripted", choices: [{ index: 0, delta: { tool_calls: [{ index: i, id: `call_${results}_${i}`, type: "function", function: { name: c.name, arguments: JSON.stringify(c.args) } }] }, finish_reason: null }] }));
        send({ id: "c", model: "scripted", choices: [{ index: 0, delta: {}, finish_reason: reply.calls?.length ? "tool_calls" : "stop" }], usage: { prompt_tokens: 10, completion_tokens: 5 } });
        res.write("data: [DONE]\n\n");
        res.end();
      });
    });
    server.listen(0, "127.0.0.1", () => {
      const addr = server.address() as { port: number };
      resolveServer({ server, url: `http://127.0.0.1:${addr.port}` });
    });
  });
}

function git(cwd: string, ...args: string[]): string {
  return execFileSync("git", ["-C", cwd, ...args], { encoding: "utf8" });
}

/** Bounded close: a hosted macOS runner once held a second instance's close past the test budget. */
async function closeApp(app: ElectronApplication): Promise<void> {
  const proc = app.process();
  await Promise.race([app.close(), new Promise<void>((r) => setTimeout(r, 15_000))]);
  if (proc.exitCode === null) proc.kill("SIGKILL");
}

async function launch(dataDir: string, extraEnv: Record<string, string>): Promise<{ app: ElectronApplication; page: Page }> {
  const app = await electron.launch({ args: [join(appDir, "dist", "main", "main.cjs")], env: { ...process.env, MODBIT_DATA_DIR: dataDir, MODBIT_CORE_BIN: coreBin, OPENAI_API_KEY: "", ANTHROPIC_API_KEY: "", ...extraEnv } });
  const page = await app.firstWindow();
  app.process().stderr?.on("data", (d: Buffer) => process.stderr.write(`[electron] ${d}`));
  app.process().stdout?.on("data", (d: Buffer) => process.stderr.write(`[electron] ${d}`));
  await expect(page.getByTestId("core-status")).toContainText("Core connected", { timeout: 60_000 });
  return { app, page };
}

test("inline patch: a one-hunk user edit lands through the Core's change transaction; stale and protected attempts are refused; no buffer survives", async () => {
  const repo = mkdtempSync(join(tmpdir(), "modbit-patch-repo-"));
  const original = Array.from({ length: 12 }, (_, i) => `line ${i + 1}`).join("\n") + "\n";
  writeFileSync(join(repo, "notes.txt"), original);
  writeFileSync(join(repo, ".env"), "SECRET=1\n");
  git(repo, "init", "-q", "-b", "main");
  git(repo, "config", "core.autocrlf", "false");
  git(repo, "add", "-A");
  git(repo, "-c", "user.name=t", "-c", "user.email=t@e", "commit", "-q", "-m", "base");
  const candidate = original.replace("line 2\n", "line 2 changed\n");
  const { server, url } = await scriptedModel([
    { calls: [{ name: "plan.update", args: { outcome: "edit notes", expected_files: ["notes.txt"] } }] },
    { calls: [{ name: "fs.read", args: { path: "notes.txt" } }] },
    { calls: [{ name: "change.apply", args: { path: "notes.txt", op: "replace", content: candidate } }] },
    { calls: [{ name: "task.complete", args: { summary: "edited", self_review: { findings: [] } } }] },
  ]);
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-patch-"));
  const env = { MODBIT_OPENAI_BASE_URL: url };
  try {
    const { app, page } = await launch(dataDir, env);
    await page.getByTestId("goal").fill("Edit the notes");
    await page.getByTestId("workspace").fill(repo);
    await page.getByTestId("run").click();
    const card = page.getByTestId("task-card").first();
    await expect(card.getByTestId("task-state")).toHaveText("Queued");
    await card.getByTestId("task-start").click();
    await expect(page.getByTestId("column-readyForReview").getByTestId("task-card")).toHaveCount(1, { timeout: 60_000 });
    // The first result opens the Review itself (docs/39 step 5).
    await expect(page.getByTestId("review")).toBeVisible({ timeout: 15_000 });
    await expect(page.getByTestId("review-meta")).toContainText("ReadyForReview");
    const metaBefore = await page.getByTestId("review-meta").textContent();
    const revisionBefore = Number(/workspace revision (\d+)/.exec(metaBefore ?? "")?.[1]);
    expect(Number.isInteger(revisionBefore)).toBe(true);
    // The edit: one hunk, at the revision the review shows.
    const file = page.locator('[data-testid="review-file"][data-path="notes.txt"]');
    await file.getByTestId("file-patch").click();
    await page.getByTestId("patch-old").fill("line 7\n");
    await page.getByTestId("patch-new").fill("line 7 edited by hand\n");
    await page.getByTestId("patch-apply").click();
    await expect(page.getByTestId("patch-result")).toContainText(`applied at workspace revision ${revisionBefore + 1} (was ${revisionBefore})`, { timeout: 30_000 });
    // The file on disk is exactly what the Core applied; the review re-read from the Core shows the new revision.
    expect(readFileSync(join(repo, "notes.txt"), "utf8")).toBe(candidate.replace("line 7\n", "line 7 edited by hand\n"));
    await expect(page.getByTestId("review-meta")).toContainText(`workspace revision ${revisionBefore + 1}`);
    // The panel is closed and empty: nothing of the edit lives in the client.
    await expect(page.getByTestId("patch-panel")).toHaveCount(0);
    await file.getByTestId("file-patch").click();
    await expect(page.getByTestId("patch-old")).toHaveValue("");
    await expect(page.getByTestId("patch-new")).toHaveValue("");
    // Protected: `.env` is not in the candidate, so the review offers no edit
    // for it; a client that names it anyway (through the same preload API the
    // panel uses) is refused by the Core's path policy, not by the renderer.
    const ids = await page.evaluate(async () => {
      const s = await window.modbit.localState();
      const t = document.querySelector('[data-testid="task-card"]')?.getAttribute("data-task-id");
      return { sessionId: s.sessionId ?? "", taskId: t ?? "" };
    });
    expect(ids.sessionId).not.toBe("");
    expect(ids.taskId).not.toBe("");
    const protectedAttempt = await page.evaluate(
      async ({ sessionId, taskId, revision }) => {
        try {
          await window.modbit.applyUserPatch(sessionId, taskId, { path: ".env", old: "SECRET=1", new: "SECRET=2", expectedWorkspaceRevision: revision, expectedFileRevision: "" });
          return "applied";
        } catch (e) {
          return (e as Error).message;
        }
      },
      { ...ids, revision: String(revisionBefore + 1) },
    );
    expect(protectedAttempt).toContain("PROTECTED_PATH");
    expect(readFileSync(join(repo, ".env"), "utf8")).toBe("SECRET=1\n");
    // From the CLI, on the same Core (it attaches through the profile's
    // core.ready): a second hand edit at the current revision lands and
    // moves the workspace on.
    await page.getByTestId("patch-old").fill("line 3\n");
    await page.getByTestId("patch-new").fill("line 3 twice\n");
    const cliOut = execFileSync(
      cliBin,
      ["--data-dir", dataDir, "task", "patch", "--session", ids.sessionId, "--task", ids.taskId, "--path", "notes.txt", "--revision", String(revisionBefore + 1), "--old", "line 9\n", "--new", "line 9 from the shell\n"],
      { encoding: "utf8", env: { ...process.env, MODBIT_CORE_BIN: coreBin, ...env } },
    );
    expect(cliOut).toContain(`patched workspace_revision=${revisionBefore + 2} (was ${revisionBefore + 1})`);
    expect(cliOut).toContain("tier=exact");
    // The review shown here is now behind: its edit is refused by the Core's
    // revision precondition (not by the renderer's memory), nothing written.
    await page.getByTestId("patch-apply").click();
    await expect(page.getByTestId("patch-result")).toContainText("refused", { timeout: 30_000 });
    await expect(page.getByTestId("patch-result")).toContainText("STALE_REVISION");
    // PX-023: the Review's state is the stale candidate revision, with the
    // Core's rejection as evidence and the reload as the next action.
    await expect(page.getByTestId("review-state")).toHaveAttribute("data-kind", "error");
    await expect(page.getByTestId("review-state-label")).toHaveText("stale candidate revision");
    await expect(page.getByTestId("review-state-next")).toContainText("reload the review");
    expect(readFileSync(join(repo, "notes.txt"), "utf8")).toBe(candidate.replace("line 7\n", "line 7 edited by hand\n").replace("line 9\n", "line 9 from the shell\n"));
    // The CLI against a stale revision is refused the same way, and against a protected path.
    for (const args of [
      ["--path", "notes.txt", "--revision", String(revisionBefore + 1), "--old", "line 3\n", "--new", "line 3 stale\n"],
      ["--path", ".env", "--revision", String(revisionBefore + 2), "--old", "SECRET=1", "--new", "SECRET=2"],
    ]) {
      let failure = "";
      try {
        execFileSync(cliBin, ["--data-dir", dataDir, "task", "patch", "--session", ids.sessionId, "--task", ids.taskId, ...args], { encoding: "utf8", stdio: ["ignore", "pipe", "pipe"], env: { ...process.env, MODBIT_CORE_BIN: coreBin, ...env } });
      } catch (e) {
        failure = String((e as { stderr?: string }).stderr ?? "") + String((e as { stdout?: string }).stdout ?? "");
      }
      expect(failure).toContain(args[1] === ".env" ? "PROTECTED_PATH" : "STALE_REVISION");
    }
    expect(readFileSync(join(repo, ".env"), "utf8")).toBe("SECRET=1\n");
    expect(readFileSync(join(repo, "notes.txt"), "utf8")).toBe(candidate.replace("line 7\n", "line 7 edited by hand\n").replace("line 9\n", "line 9 from the shell\n"));
    // After a reload the review is re-read from the Core: the new revision, no panel, no draft.
    await page.getByTestId("review-close").click();
    await page.getByTestId("task-review").click();
    await expect(page.getByTestId("review-meta")).toContainText(`workspace revision ${revisionBefore + 2}`);
    await expect(page.getByTestId("patch-panel")).toHaveCount(0);
    await closeApp(app);
  } finally {
    server.close();
  }
});
