/**
 * Packaged-app E2E for REQ-EV-0151 / REQ-EV-0275 (docs/10 "Needs Attention",
 * docs/32): the board's attention strip shows what the Core derives from
 * canonical unresolved state — here a start refused for capacity on a
 * one-slot host — and it clears the moment the resolving event lands
 * (the task starts). Nothing is invented in the renderer.
 */
import { _electron as electron, expect, test, type ElectronApplication, type Page } from "@playwright/test";
import { execFileSync } from "node:child_process";
import { mkdtempSync, writeFileSync } from "node:fs";
import { createServer, type Server } from "node:http";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const appDir = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const coreBin = process.env.MODBIT_CORE_BIN ?? resolve(appDir, "..", "..", "target", "debug", process.platform === "win32" ? "modbit-core.exe" : "modbit-core");

type Reply = { text?: string; calls?: { name: string; args: unknown }[] };

/** A scripted OpenAI-compatible model whose second request of every run is
 *  held for `delayMs`, so a second task finds the single model slot taken. */
function slowScriptedModel(script: Reply[], delayMs: number): Promise<{ server: Server; url: string }> {
  return new Promise((resolveServer) => {
    const server = createServer((req, res) => {
      let body = "";
      req.on("data", (c) => (body += c));
      req.on("end", () => {
        const parsed = JSON.parse(body || "{}") as { messages?: { role: string }[] };
        const results = (parsed.messages ?? []).filter((m) => m.role === "tool").length;
        const reply = script[results] ?? { text: "Nothing further." };
        const send = (o: unknown) => res.write(`data: ${JSON.stringify(o)}\n\n`);
        const answer = () => {
          res.writeHead(200, { "content-type": "text/event-stream" });
          if (reply.text) send({ id: "c", model: "scripted", choices: [{ index: 0, delta: { content: reply.text }, finish_reason: null }] });
          (reply.calls ?? []).forEach((c, i) => send({ id: "c", model: "scripted", choices: [{ index: 0, delta: { tool_calls: [{ index: i, id: `call_${results}_${i}`, type: "function", function: { name: c.name, arguments: JSON.stringify(c.args) } }] }, finish_reason: null }] }));
          send({ id: "c", model: "scripted", choices: [{ index: 0, delta: {}, finish_reason: reply.calls?.length ? "tool_calls" : "stop" }], usage: { prompt_tokens: 10, completion_tokens: 5 } });
          res.write("data: [DONE]\n\n");
          res.end();
        };
        if (results === 1) setTimeout(answer, delayMs);
        else answer();
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

async function closeApp(app: ElectronApplication): Promise<void> {
  const proc = app.process();
  await Promise.race([app.close(), new Promise<void>((r) => setTimeout(r, 20_000))]);
  if (proc.exitCode === null && !proc.killed) proc.kill("SIGKILL");
}

async function launch(dataDir: string, extraEnv: Record<string, string>): Promise<{ app: ElectronApplication; page: Page }> {
  const app = await electron.launch({ args: [join(appDir, "dist", "main", "main.cjs")], env: { ...process.env, MODBIT_DATA_DIR: dataDir, MODBIT_CORE_BIN: coreBin, OPENAI_API_KEY: "", ANTHROPIC_API_KEY: "", ...extraEnv } });
  const page = await app.firstWindow();
  app.process().stderr?.on("data", (d: Buffer) => process.stderr.write(`[electron] ${d}`));
  await expect(page.getByTestId("core-status")).toContainText("Core connected", { timeout: 60_000 });
  return { app, page };
}

test("attention: a start refused for capacity is an item from the Core and clears when the task starts", async () => {
  const repo = mkdtempSync(join(tmpdir(), "modbit-attention-repo-"));
  writeFileSync(join(repo, "notes.md"), "hello\n");
  git(repo, "init", "-q", "-b", "main");
  git(repo, "add", "-A");
  git(repo, "-c", "user.name=t", "-c", "user.email=t@e", "commit", "-q", "-m", "base");
  const script: Reply[] = [
    { calls: [{ name: "plan.update", args: { outcome: "read notes", expected_files: [] } }] },
    { calls: [{ name: "fs.read", args: { path: "notes.md" } }] },
    { calls: [{ name: "task.complete", args: { summary: "read", self_review: { findings: [] } } }] },
  ];
  const { server, url } = await slowScriptedModel(script, 4000);
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-attention-"));
  const env = { MODBIT_OPENAI_BASE_URL: url, MODBIT_CAPACITY: "model=1,provider=4" };
  try {
    const { app, page } = await launch(dataDir, env);
    await expect(page.getByTestId("attention")).toHaveCount(0);
    // Two tasks; the first takes the only model slot.
    for (const goal of ["read notes one", "read notes two"]) {
      await page.getByTestId("goal").fill(goal);
      await page.getByTestId("workspace").fill(repo);
      await page.getByTestId("run").click();
      await expect(page.getByTestId("task-card").filter({ hasText: goal })).toHaveCount(1);
    }
    const one = page.getByTestId("task-card").filter({ hasText: "read notes one" });
    const two = page.getByTestId("task-card").filter({ hasText: "read notes two" });
    await one.getByTestId("task-start").click();
    await expect(one.getByTestId("task-state")).toHaveText("Running", { timeout: 30_000 });
    await two.getByTestId("task-start").click();
    // The Core refused the second start: one CAPACITY item, named and actionable.
    const item = page.getByTestId("attention-item");
    await expect(item).toHaveCount(1, { timeout: 30_000 });
    await expect(item).toHaveAttribute("data-kind", "CAPACITY");
    await expect(item).toContainText("model_concurrency");
    await expect(item).toContainText("StartTask");
    const twoId = await two.getAttribute("data-task-id");
    await expect(item).toHaveAttribute("data-task-id", twoId!);
    // The first finishes; starting the second clears the item from its
    // canonical event (TaskStarted), nothing else.
    await expect(page.getByTestId("column-readyForReview").getByTestId("task-card")).toHaveCount(1, { timeout: 60_000 });
    await expect(item).toHaveCount(1);
    await two.getByTestId("task-start").click();
    await expect(page.getByTestId("attention-item")).toHaveCount(0, { timeout: 30_000 });
    await expect(page.getByTestId("column-readyForReview").getByTestId("task-card")).toHaveCount(2, { timeout: 60_000 });
    await closeApp(app);
  } finally {
    server.close();
  }
});
