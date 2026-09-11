/**
 * Onboarding E2E (PX-E2E-022, docs/39 "Onboarding"): the real Electron app on
 * a fresh profile, driven through the three welcome steps — provider setup
 * with a live test call, explicit scoped repository trust, a starter task on
 * a small real repository — to a first ReadyForReview with real evidence,
 * timed. The model is the wire-faithful scripted provider (docs/decisions
 * DR-M3-003 defers the live-model measurement); the clock is the app's, from
 * launch to the Review opening.
 *
 * Negative half of QUAL-PX-022: an invalid key names its cause and offers a
 * retry; an unreachable endpoint names the network; a profile that skipped
 * provider setup cannot start a task; a missing repository names itself.
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

/** Scripted OpenAI-compatible model that also behaves like a provider about
 *  credentials: `sk-invalid` is refused with 401 before the body is read. */
function scriptedModel(script: { text?: string; calls?: { name: string; args: unknown }[] }[]): Promise<{ server: Server; url: string }> {
  return new Promise((resolveServer) => {
    const server = createServer((req, res) => {
      if ((req.headers.authorization ?? "").includes("sk-invalid")) {
        const body = JSON.stringify({ error: { message: "Incorrect API key provided", type: "invalid_request_error", code: "invalid_api_key" } });
        res.writeHead(401, { "content-type": "application/json", "content-length": String(body.length) });
        res.end(body);
        return;
      }
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

/** A small real repository with a configured check, so the first result has
 *  a diff, a test run and receipts. */
function smallRepo(): string {
  const repo = mkdtempSync(join(tmpdir(), "modbit-onboarding-repo-"));
  writeFileSync(join(repo, "total.py"), "def total(q, unit):\n    return q * unit\n");
  writeFileSync(join(repo, "README.md"), "# Totals\n");
  mkdirSync(join(repo, ".modbit"), { recursive: true });
  writeFileSync(join(repo, ".modbit", "verification.json"), JSON.stringify({ commands: [{ id: "configured:py", argv: [process.platform === "win32" ? "python" : "python3", "-c", "import total; assert total.total(3, 250) == 750"] }] }));
  git(repo, "init", "-q", "-b", "main");
  git(repo, "add", "-A");
  git(repo, "-c", "user.name=t", "-c", "user.email=t@e", "commit", "-q", "-m", "base");
  return repo;
}

/** The starter task's script: read, edit, complete — a first useful change. */
const script = [
  { calls: [{ name: "plan.update", args: { outcome: "document the units", expected_files: ["total.py"] } }] },
  { calls: [{ name: "fs.read", args: { path: "total.py" } }] },
  { calls: [{ name: "change.apply", args: { path: "total.py", op: "edit", text_edits: [{ old: "    return q * unit\n", new: "    return q * unit  # unit is in minor units\n" }] } }] },
  { calls: [{ name: "task.complete", args: { summary: "documented", self_review: { findings: [] } } }] },
];

/** A fresh profile: no provider anywhere in the environment. */
async function launchFresh(dataDir: string): Promise<{ app: ElectronApplication; page: Page }> {
  const app = await electron.launch({
    args: [join(appDir, "dist", "main", "main.cjs")],
    env: { ...process.env, MODBIT_DATA_DIR: dataDir, MODBIT_CORE_BIN: coreBin, OPENAI_API_KEY: "", ANTHROPIC_API_KEY: "", MODBIT_OPENAI_BASE_URL: "", MODBIT_ANTHROPIC_BASE_URL: "" },
  });
  const page = await app.firstWindow();
  app.process().stderr?.on("data", (d: Buffer) => process.stderr.write(`[electron] ${d}`));
  await expect(page.getByTestId("core-status")).toContainText("Core connected", { timeout: 60_000 });
  return { app, page };
}

/** One fresh profile, first launch to the Review opening on the first result. Returns seconds. */
async function onboardOnce(url: string, repo: string): Promise<number> {
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-onboarding-"));
  const started = Date.now();
  const { app, page } = await launchFresh(dataDir);
  try {
    // Step 2 is the active step: no provider yet.
    await expect(page.getByTestId("welcome")).toBeVisible();
    await expect(page.getByTestId("welcome-step-provider")).toHaveAttribute("data-active", "true");
    await page.getByTestId("provider-key").fill("sk-onboarding-test-key");
    await page.getByTestId("provider-url").fill(url);
    await page.getByTestId("provider-test").click();
    await expect(page.getByTestId("provider-result")).toHaveAttribute("data-ok", "true", { timeout: 30_000 });
    await expect(page.getByTestId("provider-ready")).toContainText("openai");
    // Step 3: explicit, scoped trust; the starter tasks follow from the stack.
    await expect(page.getByTestId("welcome-step-repository")).toHaveAttribute("data-active", "true");
    await page.getByTestId("trust-root").fill(repo);
    await page.getByTestId("trust-confirm").click();
    await expect(page.getByTestId("repository-trusted")).toContainText(repo.split(/[\\/]/).pop() ?? repo, { timeout: 15_000 });
    await expect(page.getByTestId("starter-tasks")).toContainText("python");
    // Step 4: a starter task starts immediately on the direct baseline.
    await page.getByTestId("starter-task").first().click();
    await expect(page.getByTestId("task-card").first()).toBeVisible({ timeout: 15_000 });
    // Step 5: the first result opens the Review on the diff with its evidence.
    await expect(page.getByTestId("review-meta")).toContainText("ReadyForReview", { timeout: 120_000 });
    const seconds = (Date.now() - started) / 1000;
    await expect(page.getByTestId("review-hunk")).toHaveCount(1);
    await expect(page.getByTestId("review-verification")).toContainText("COMPLETION");
    return seconds;
  } finally {
    await app.close();
  }
}

test("onboarding: a fresh profile reaches a first reviewed change through provider setup, repository trust and a starter task, within five minutes at the median", async () => {
  test.setTimeout(600_000);
  const { server, url } = await scriptedModel(script);
  try {
    const samples: number[] = [];
    for (let i = 0; i < 3; i += 1) samples.push(await onboardOnce(url, smallRepo()));
    samples.sort((a, b) => a - b);
    const median = samples[Math.floor(samples.length / 2)] ?? Number.POSITIVE_INFINITY;
    const p90 = samples[Math.min(samples.length - 1, Math.floor(samples.length * 0.9))] ?? Number.POSITIVE_INFINITY;
    process.stderr.write(`ONBOARDING samples_s=${JSON.stringify(samples)} median_s=${median.toFixed(1)} p90_s=${p90.toFixed(1)} platform=${process.platform}/${process.arch}\n`);
    expect(median).toBeLessThan(300);
  } finally {
    server.close();
  }
});

test("onboarding: an invalid key, an unreachable endpoint and a missing repository each name their cause; a profile without a provider cannot start a task", async () => {
  test.setTimeout(300_000);
  const { server, url } = await scriptedModel(script);
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-onboarding-neg-"));
  try {
    const { app, page } = await launchFresh(dataDir);
    try {
      // No provider: a task can be created from the composer but does not start.
      await expect(page.getByTestId("composer-no-provider")).toBeVisible();
      const repo = smallRepo();
      await page.getByTestId("goal").fill("summarise");
      await page.getByTestId("workspace").fill(repo);
      await page.getByTestId("run").click();
      const card = page.getByTestId("task-card").first();
      await expect(card.getByTestId("task-state")).toHaveText("Queued", { timeout: 15_000 });
      await card.getByTestId("task-start").click();
      await expect(page.getByTestId("banner-error")).toContainText("NO_PROVIDER", { timeout: 15_000 });
      // An invalid key: the provider refuses it and the step says so.
      await page.getByTestId("provider-key").fill("sk-invalid");
      await page.getByTestId("provider-url").fill(url);
      await page.getByTestId("provider-test").click();
      await expect(page.getByTestId("provider-result")).toHaveAttribute("data-ok", "false", { timeout: 30_000 });
      await expect(page.getByTestId("provider-result")).toContainText("rejected the key");
      await expect(page.getByTestId("provider-result")).toContainText("retry");
      // An unreachable endpoint: the network is named.
      await page.getByTestId("provider-key").fill("sk-onboarding-test-key");
      await page.getByTestId("provider-url").fill("http://127.0.0.1:9");
      await page.getByTestId("provider-test").click();
      await expect(page.getByTestId("provider-result")).toHaveAttribute("data-ok", "false", { timeout: 60_000 });
      await expect(page.getByTestId("provider-result")).toContainText("could not be reached");
      // The working endpoint confirms; with a task already on the fleet the
      // welcome retires (docs/39: it persists only while a step is missing)
      // and the composer no longer warns.
      await page.getByTestId("provider-key").fill("sk-onboarding-test-key");
      await page.getByTestId("provider-url").fill(url);
      await page.getByTestId("provider-test").click();
      await expect(page.getByTestId("welcome")).toHaveCount(0, { timeout: 30_000 });
      await expect(page.getByTestId("composer-no-provider")).toHaveCount(0);
      // A repository that does not exist names itself when the composer
      // tries to trust it.
      await page.getByTestId("goal").fill("summarise");
      await page.getByTestId("workspace").fill(join(tmpdir(), "modbit-no-such-repository-" + Date.now()));
      await page.getByTestId("run").click();
      await expect(page.getByTestId("banner-error")).toContainText("does not exist", { timeout: 15_000 });
      // And the task that could not start before starts now that a provider
      // exists and its repository was trusted by the composer.
      await card.getByTestId("task-start").click();
      await expect(page.getByTestId("column-readyForReview").getByTestId("task-card")).toHaveCount(1, { timeout: 120_000 });
    } finally {
      await app.close();
    }
  } finally {
    server.close();
  }
});
