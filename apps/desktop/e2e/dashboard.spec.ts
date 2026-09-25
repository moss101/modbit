/**
 * M10.1 (docs/34, docs/77 "dashboards over the real usage ledger, SLO
 * events and cost records, never a mock feed"): the desktop's operations
 * dashboard shows exactly what the Core aggregates from its log. A task runs
 * to review in the real app against a real Core and a scripted provider;
 * the dashboard's tokens, calls, tool outcomes, task rows and states are the
 * same numbers `modbit-cli dashboard` reads from the same Core, and a
 * refresh after a second task follows the log.
 */
import { _electron as electron, expect, test, type ElectronApplication, type Page } from "@playwright/test";
import { execFile, execFileSync } from "node:child_process";
import { mkdtempSync, writeFileSync } from "node:fs";
import { createServer, type Server } from "node:http";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const appDir = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const coreBin = process.env.MODBIT_CORE_BIN ?? resolve(appDir, "..", "..", "target", "debug", process.platform === "win32" ? "modbit-core.exe" : "modbit-core");
const cliBin = process.env.MODBIT_CLI_BIN ?? resolve(dirname(coreBin), process.platform === "win32" ? "modbit-cli.exe" : "modbit-cli");

function scriptedModel(script: { calls?: { name: string; args: unknown }[] }[]): Promise<{ server: Server; url: string }> {
  return new Promise((resolveServer) => {
    const server = createServer((req, res) => {
      let body = "";
      req.on("data", (c) => (body += c));
      req.on("end", () => {
        const parsed = JSON.parse(body || "{}") as { messages?: { role: string }[] };
        const results = (parsed.messages ?? []).filter((m) => m.role === "tool").length;
        const reply = script[results] ?? {};
        res.writeHead(200, { "content-type": "text/event-stream" });
        const send = (o: unknown) => res.write(`data: ${JSON.stringify(o)}\n\n`);
        (reply.calls ?? []).forEach((c, i) => send({ id: "c", model: "scripted", choices: [{ index: 0, delta: { tool_calls: [{ index: i, id: `call_${results}_${i}`, type: "function", function: { name: c.name, arguments: JSON.stringify(c.args) } }] }, finish_reason: null }] }));
        if (!reply.calls?.length) send({ id: "c", model: "scripted", choices: [{ index: 0, delta: { content: "Nothing further." }, finish_reason: null }] });
        send({ id: "c", model: "scripted", choices: [{ index: 0, delta: {}, finish_reason: reply.calls?.length ? "tool_calls" : "stop" }], usage: { prompt_tokens: 40 + results, completion_tokens: 7 } });
        res.write("data: [DONE]\n\n");
        res.end();
      });
    });
    server.listen(0, "127.0.0.1", () => resolveServer({ server, url: `http://127.0.0.1:${(server.address() as { port: number }).port}` }));
  });
}

function git(cwd: string, ...args: string[]): string {
  return execFileSync("git", ["-C", cwd, ...args], { encoding: "utf8" });
}

function cli(args: string[], env: NodeJS.ProcessEnv): Promise<{ status: number; stdout: string; stderr: string }> {
  return new Promise((resolveRun) => {
    execFile(cliBin, args, { encoding: "utf8", env }, (error, stdout, stderr) => {
      const status = error && typeof (error as { code?: unknown }).code === "number" ? ((error as { code: number }).code ?? 1) : error ? 1 : 0;
      resolveRun({ status, stdout, stderr });
    });
  });
}

async function launch(dataDir: string, extraEnv: Record<string, string>): Promise<{ app: ElectronApplication; page: Page }> {
  const app = await electron.launch({ args: [join(appDir, "dist", "main", "main.cjs")], env: { ...process.env, MODBIT_DATA_DIR: dataDir, MODBIT_CORE_BIN: coreBin, OPENAI_API_KEY: "", ANTHROPIC_API_KEY: "", ...extraEnv } });
  const page = await app.firstWindow();
  app.process().stderr?.on("data", (d: Buffer) => process.stderr.write(`[electron] ${d}`));
  await expect(page.getByTestId("core-status")).toContainText("Core connected", { timeout: 60_000 });
  return { app, page };
}

async function closeApp(app: ElectronApplication): Promise<void> {
  const proc = app.process();
  await Promise.race([app.close(), new Promise<void>((r) => setTimeout(r, 15_000))]);
  if (proc.exitCode === null) proc.kill("SIGKILL");
}

/** `key=value` from the CLI line starting with `prefix`. */
function field(stdout: string, prefix: string, key: string): string {
  const line = stdout.split("\n").find((l) => l.startsWith(prefix)) ?? "";
  return (new RegExp(`(?:^| )${key}=(\\S+)`).exec(line) ?? [])[1] ?? "";
}

test("the operations dashboard shows the Core's own aggregation of its log, the same numbers the CLI reads", async () => {
  const repo = mkdtempSync(join(tmpdir(), "modbit-dash-repo-"));
  writeFileSync(join(repo, "notes.txt"), "line 1\nline 2\n");
  git(repo, "init", "-q", "-b", "main");
  git(repo, "config", "core.autocrlf", "false");
  git(repo, "add", "-A");
  git(repo, "-c", "user.name=t", "-c", "user.email=t@e", "commit", "-q", "-m", "base");
  const { server, url } = await scriptedModel([
    { calls: [{ name: "plan.update", args: { outcome: "read the notes", expected_files: [] } }] },
    { calls: [{ name: "fs.read", args: { path: "notes.txt" } }] },
    { calls: [{ name: "task.complete", args: { summary: "read", self_review: { findings: [] } } }] },
  ]);
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-dash-"));
  const env = { MODBIT_OPENAI_BASE_URL: url };
  try {
    const { app, page } = await launch(dataDir, env);
    await page.getByTestId("workspace").fill(repo);
    await page.getByTestId("goal").fill("Read the notes");
    await page.getByTestId("run").click();
    const card = page.getByTestId("task-card").first();
    await expect(card).toBeVisible({ timeout: 30_000 });
    await card.getByTestId("task-start").click();
    await expect(page.getByTestId("column-readyForReview").getByTestId("task-card")).toHaveCount(1, { timeout: 90_000 });

    await page.getByTestId("open-dashboard").click();
    const dash = page.getByTestId("dashboard");
    await expect(dash.getByTestId("dashboard-task-row")).toHaveCount(1, { timeout: 15_000 });
    const sessionId = await page.evaluate(async () => (await window.modbit.localState()).sessionId ?? "");
    const read = await cli(["--data-dir", dataDir, "dashboard", "--session", sessionId], { ...process.env, MODBIT_CORE_BIN: coreBin, OPENAI_API_KEY: "", ANTHROPIC_API_KEY: "", ...env });
    expect(read.status, read.stderr).toBe(0);
    process.stderr.write(`[cli dashboard]\n${read.stdout}\n`);

    // Tokens and cost: the ledger's totals, as the CLI reads them.
    await expect(dash.getByTestId("dashboard-tokens")).toHaveAttribute("data-input", field(read.stdout, "tokens ", "input"));
    await expect(dash.getByTestId("dashboard-tokens")).toHaveAttribute("data-output", field(read.stdout, "tokens ", "output"));
    expect(Number(field(read.stdout, "tokens ", "input"))).toBeGreaterThan(0);
    await expect(dash.getByTestId("dashboard-cost")).toHaveAttribute("data-cost-minor", field(read.stdout, "cost ", "cost_minor"));
    await expect(dash.getByTestId("dashboard-cost")).toContainText("unpriced", { ignoreCase: true });
    // The task row: its calls, tool calls and cost.
    const row = dash.getByTestId("dashboard-task-row").first();
    const taskId = (await row.getAttribute("data-task-id")) ?? "";
    const line = read.stdout.split("\n").find((l) => l.startsWith(`task ${taskId} `)) ?? "";
    expect(line, read.stdout).not.toBe("");
    await expect(row).toHaveAttribute("data-model-calls", field(line, "task ", "model_calls"));
    await expect(row).toHaveAttribute("data-tool-calls", field(line, "task ", "tool_calls"));
    expect(Number(field(line, "task ", "model_calls"))).toBe(3);
    await expect(row).toContainText("ReadyForReview");
    // Tool outcomes and the per-model row.
    await expect(dash.getByTestId("dashboard-tools")).toHaveAttribute("data-succeeded", field(read.stdout, "tools ", "succeeded"));
    await expect(dash.getByTestId("dashboard-model-row")).toHaveCount(1);
    await expect(dash.getByTestId("dashboard-model-row").first()).toHaveAttribute("data-calls", "3");
    // No cloud task: no SLO starts; no failure.
    await expect(dash.getByTestId("dashboard-slo-cold")).toHaveAttribute("data-starts", "0");
    await expect(dash.getByTestId("dashboard-no-failures")).toBeVisible();

    // A second task, and a refresh follows the log.
    await page.getByTestId("dashboard-close").click();
    // The review the finished task opened hides the composer; go back to it.
    if (await page.getByTestId("review-close").isVisible()) await page.getByTestId("review-close").click();
    await page.getByTestId("goal").fill("A second task, not started");
    await page.getByTestId("run").click();
    await expect(page.getByTestId("task-card")).toHaveCount(2, { timeout: 30_000 });
    await page.getByTestId("open-dashboard").click();
    await expect(page.getByTestId("dashboard").getByTestId("dashboard-task-row")).toHaveCount(2, { timeout: 15_000 });
    await expect(page.getByTestId("dashboard").getByTestId("dashboard-states")).toContainText("ReadyForReview 1");
    await closeApp(app);
  } finally {
    server.close();
  }
});
