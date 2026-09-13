/**
 * Packaged-app E2E for M6.6 (docs/10 "Home / Fleet", docs/32): the real
 * Electron app against the real modbit-core, a primary that delegates one
 * subtask to a child agent through the real admission transaction. The
 * parent's card shows the phase, the active agent count, the latest
 * evidence and, nested under it, the child; the child is never a top-level
 * card; when the child ends its result is on the parent card; the board
 * needs no log reading. Every fact rendered comes from a Core event.
 */
import { _electron as electron, expect, test, type ElectronApplication, type Page } from "@playwright/test";
import { execFileSync } from "node:child_process";
import { mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import { createServer, type Server } from "node:http";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const appDir = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const coreBin = process.env.MODBIT_CORE_BIN ?? resolve(appDir, "..", "..", "target", "debug", process.platform === "win32" ? "modbit-core.exe" : "modbit-core");

type Reply = { text?: string; calls?: { name: string; args: unknown }[] };

/** Scripted OpenAI-compatible model with one script per needle: a request
 *  whose body contains a needle is answered from that script (the child's
 *  goal tells its requests from the parent's); the rest from `main`. Each
 *  script is indexed by the number of tool results in the request. */
function scriptedModels(main: Reply[], byNeedle: [string, Reply[]][]): Promise<{ server: Server; url: string }> {
  return new Promise((resolveServer) => {
    const server = createServer((req, res) => {
      let body = "";
      req.on("data", (c) => (body += c));
      req.on("end", () => {
        const parsed = JSON.parse(body || "{}") as { messages?: { role: string }[] };
        const results = (parsed.messages ?? []).filter((m) => m.role === "tool").length;
        const script = byNeedle.find(([needle]) => body.includes(needle))?.[1] ?? main;
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

test("fleet: a delegating parent shows its phase, agents and the nested child; the child is never a top-level card", async () => {
  const repo = mkdtempSync(join(tmpdir(), "modbit-fleet-repo-"));
  writeFileSync(join(repo, "README.md"), "# fleet\n");
  mkdirSync(join(repo, "src", "a"), { recursive: true });
  writeFileSync(join(repo, "src", "a", ".keep"), "");
  git(repo, "init", "-q", "-b", "main");
  git(repo, "add", "-A");
  git(repo, "-c", "user.name=t", "-c", "user.email=t@e", "commit", "-q", "-m", "base");
  const parent: Reply[] = [
    { calls: [{ name: "plan.update", args: { outcome: "module a exists", expected_files: ["README.md"], steps: [{ id: "a", title: "module a" }] } }] },
    { calls: [{ name: "agent.spawn", args: { idempotency_key: "child-a", objective: "create src/a/a.txt containing alpha", write_scope: ["src/a/"], work_node: "a", max_turns: 6 } }] },
    { calls: [{ name: "agent.wait", args: { idempotency_key: "child-a", timeout_ms: 60000 } }] },
    { calls: [{ name: "task.complete", args: { summary: "delegated and collected", self_review: { findings: [] } } }] },
  ];
  const child: Reply[] = [
    { calls: [{ name: "plan.update", args: { outcome: "a.txt exists", expected_files: ["src/a/a.txt"] } }] },
    { calls: [{ name: "change.apply", args: { path: "src/a/a.txt", op: "replace", content: "alpha\n" } }] },
    { calls: [{ name: "task.complete", args: { summary: "created src/a/a.txt", self_review: { findings: [] } } }] },
  ];
  const { server, url } = await scriptedModels(parent, [["Task goal: create src/a/a.txt", child]]);
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-fleet-agents-"));
  const env = { MODBIT_OPENAI_BASE_URL: url, MODBIT_CAPACITY: "model=4,provider=8" };
  try {
    const { app, page } = await launch(dataDir, env);
    await page.getByTestId("goal").fill("Delegate module a");
    await page.getByTestId("workspace").fill(repo);
    await page.getByTestId("run").click();
    const card = page.getByTestId("task-card").first();
    await expect(card.getByTestId("task-state")).toHaveText("Queued");
    await expect(card.getByTestId("task-agents")).toHaveText("0/0");
    await card.getByTestId("task-start").click();
    // The parent delegates: phase, agents and the nested child come from
    // AgentNodeCreated / SubagentAdmitted, not from logs.
    await expect(card.getByTestId("task-children").getByTestId("child-card")).toHaveCount(1, { timeout: 60_000 });
    await expect(card.getByTestId("child-goal")).toHaveText("create src/a/a.txt containing alpha");
    // A fast child may already have ended by now (a hosted macOS runner saw
    // it): the evidence line is the delegation or the child's completion.
    await expect(card.getByTestId("task-evidence")).toContainText(/delegated child-a|child completed: created src\/a\/a\.txt/);
    // The child is never a top-level card, in any column.
    await expect(page.getByTestId("task-card")).toHaveCount(1);
    // The child ends: its result is the parent's latest evidence, the
    // agent counts settle, and the parent reaches Ready for Review.
    await expect(page.getByTestId("column-readyForReview").getByTestId("task-card")).toHaveCount(1, { timeout: 90_000 });
    await expect(card.getByTestId("task-evidence")).toContainText("child completed: created src/a/a.txt");
    await expect(card.getByTestId("task-agents")).toHaveText("0/2");
    await expect(card.getByTestId("child-state")).toHaveText("ReadyForReview");
    await expect(page.getByTestId("task-card")).toHaveCount(1);
    // Restart the app: the board is rebuilt from the Core, children included.
    await app.close();
    const again = await launch(dataDir, env);
    await expect(again.page.getByTestId("column-readyForReview").getByTestId("task-card")).toHaveCount(1, { timeout: 30_000 });
    await expect(again.page.getByTestId("task-card")).toHaveCount(1);
    await expect(again.page.getByTestId("task-card").first().getByTestId("child-card")).toHaveCount(1);
    await again.app.close();
  } finally {
    server.close();
  }
});
