/**
 * Review surface E2E (docs/20, docs/83 "Agent coding loop", docs/51 E2E-001
 * shape): the real Electron app drives the real Core, which runs the real
 * one-agent loop against a real Git checkout with a scripted wire-faithful
 * model. The user reviews the candidate per hunk, rejects one, accepts the
 * rest; the Git history reflects exactly that choice; the review survives an
 * app restart because every fact is re-read from the Core.
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

/** Scripted OpenAI-compatible model: the reply is chosen by the number of tool results in the request. */
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

async function launch(dataDir: string, extraEnv: Record<string, string>): Promise<{ app: ElectronApplication; page: Page }> {
  const app = await electron.launch({ args: [join(appDir, "dist", "main", "main.cjs")], env: { ...process.env, MODBIT_DATA_DIR: dataDir, MODBIT_CORE_BIN: coreBin, OPENAI_API_KEY: "", ANTHROPIC_API_KEY: "", ...extraEnv } });
  const page = await app.firstWindow();
  // Diagnosability: the Electron main process relays the Core's stderr; keep it in the test output.
  app.process().stderr?.on("data", (d: Buffer) => process.stderr.write(`[electron] ${d}`));
  app.process().stdout?.on("data", (d: Buffer) => process.stderr.write(`[electron] ${d}`));
  try {
    await expect(page.getByTestId("core-status")).toContainText("Core connected", { timeout: 60_000 });
  } catch (e) {
    const status = await page.evaluate(() => window.modbit.coreStatus()).catch(() => null);
    throw new Error(`Core did not connect: ${JSON.stringify(status)}; ${(e as Error).message}`);
  }
  return { app, page };
}

test("review surface: per-hunk decision commits exactly the user's choice and survives an app restart", async () => {
  const repo = mkdtempSync(join(tmpdir(), "modbit-review-repo-"));
  const original = Array.from({ length: 16 }, (_, i) => `line ${i + 1}`).join("\n") + "\n";
  writeFileSync(join(repo, "notes.txt"), original);
  git(repo, "init", "-q", "-b", "main");
  git(repo, "add", "-A");
  git(repo, "-c", "user.name=t", "-c", "user.email=t@e", "commit", "-q", "-m", "base");
  const candidate = original.replace("line 2\n", "line 2 changed\n").replace("line 15\n", "line 15 changed\n");
  const { server, url } = await scriptedModel([
    { calls: [{ name: "plan.update", args: { outcome: "edit notes", expected_files: ["notes.txt"] } }] },
    // Retrieve before edit (PX-015): the Core refuses a write to a file the task has not read.
    { calls: [{ name: "fs.read", args: { path: "notes.txt" } }] },
    { calls: [{ name: "change.apply", args: { path: "notes.txt", op: "replace", content: candidate } }] },
    { calls: [{ name: "task.complete", args: { summary: "edited", self_review: { findings: [] } } }] },
  ]);
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-review-"));
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
    await page.getByTestId("task-review").click();
    await expect(page.getByTestId("review-meta")).toContainText("ReadyForReview");
    await expect(page.getByTestId("review-hunk")).toHaveCount(2);
    await expect(page.getByTestId("review-verification")).toContainText("BASELINE");
    await expect(page.getByTestId("review-plan")).toContainText("edit notes");
    // Restart the whole app mid-review: the review is rebuilt from the Core, not from renderer state.
    await app.close();
    const again = await launch(dataDir, env);
    await expect(again.page.getByTestId("column-readyForReview").getByTestId("task-card")).toHaveCount(1, { timeout: 30_000 });
    await again.page.getByTestId("task-review").click();
    await expect(again.page.getByTestId("review-hunk")).toHaveCount(2);
    // Reject the second hunk, accept the rest with a commit message.
    const second = again.page.getByTestId("review-hunk").nth(1);
    await expect(second).toContainText("line 15 changed");
    await second.getByTestId("hunk-reject").check();
    await expect(second).toHaveAttribute("data-rejected", "true");
    await again.page.getByTestId("review-note").fill("Keep the second region");
    await again.page.getByTestId("review-accept").click();
    await expect(again.page.getByTestId("review-result")).toContainText("Accepted", { timeout: 30_000 });
    await expect(again.page.getByTestId("review-result")).toContainText("1 hunk(s) reverted");
    await again.page.getByTestId("review-close").click();
    await expect(again.page.getByTestId("column-completed").getByTestId("task-card")).toHaveCount(1, { timeout: 30_000 });
    // The Git diff matches the user's choice exactly (REQ-EV-0036) and the history reflects it (E2E-001).
    expect(readFileSync(join(repo, "notes.txt"), "utf8")).toBe(original.replace("line 2\n", "line 2 changed\n"));
    expect(git(repo, "log", "--format=%s", "-1").trim()).toBe("Keep the second region");
    expect(git(repo, "status", "--porcelain").split("\n").filter((l) => l && !l.includes(".modbit"))).toEqual([]);
    await again.app.close();
  } finally {
    server.close();
  }
});
