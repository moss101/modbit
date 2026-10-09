/**
 * Shared pieces of the browser runtime specs (PX-073, PX-120..PX-123): a
 * scripted OpenAI-compatible model (the repo's standard stand-in for the
 * model; everything the Core and the host do with it is real), a git
 * fixture with the mandatory check, the real app launched against a real
 * Core, and small readers for what the model was shown.
 */
import { _electron as electron, expect, type ElectronApplication, type Page } from "@playwright/test";
import { execFileSync } from "node:child_process";
import { mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import { createServer, type Server } from "node:http";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

export const appDir = resolve(dirname(fileURLToPath(import.meta.url)), "..", "..");
export const coreBin = process.env.MODBIT_CORE_BIN ?? resolve(appDir, "..", "..", "target", "debug", process.platform === "win32" ? "modbit-core.exe" : "modbit-core");

export type Calls = { name: string; args: unknown }[];
/** A step is fixed, or computed from the tool texts so far (a ref read from a snapshot). */
export type Script = ({ calls?: Calls } | ((toolTexts: string[]) => { calls?: Calls } | Promise<{ calls?: Calls }>))[];

export function scriptedModel(script: Script): Promise<{ server: Server; url: string; toolTexts: string[] }> {
  const toolTexts: string[] = [];
  return new Promise((resolveServer) => {
    const server = createServer((req, res) => {
      let body = "";
      req.on("data", (c) => (body += c));
      req.on("end", async () => {
        const parsed = JSON.parse(body || "{}") as { messages?: { role: string; content?: string }[] };
        const tools = (parsed.messages ?? []).filter((m) => m.role === "tool");
        const results = tools.length;
        const last = tools.at(-1)?.content;
        if (typeof last === "string" && toolTexts.length < results) toolTexts.push(last);
        const step = script[results] ?? {};
        const reply = typeof step === "function" ? await step(toolTexts) : step;
        res.writeHead(200, { "content-type": "text/event-stream" });
        const send = (o: unknown) => res.write(`data: ${JSON.stringify(o)}\n\n`);
        (reply.calls ?? []).forEach((c, i) => send({ id: "c", model: "scripted", choices: [{ index: 0, delta: { tool_calls: [{ index: i, id: `call_${results}_${i}`, type: "function", function: { name: c.name, arguments: JSON.stringify(c.args) } }] }, finish_reason: null }] }));
        if (!reply.calls?.length) send({ id: "c", model: "scripted", choices: [{ index: 0, delta: { content: "Nothing further." }, finish_reason: null }] });
        send({ id: "c", model: "scripted", choices: [{ index: 0, delta: {}, finish_reason: reply.calls?.length ? "tool_calls" : "stop" }], usage: { prompt_tokens: 10, completion_tokens: 5 } });
        res.write("data: [DONE]\n\n");
        res.end();
      });
    });
    server.listen(0, "127.0.0.1", () => resolveServer({ server, url: `http://127.0.0.1:${(server.address() as { port: number }).port}`, toolTexts }));
  });
}

export function git(cwd: string, ...args: string[]): string {
  return execFileSync("git", ["-C", cwd, ...args], { encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] }).trim();
}

/** A repository a task can complete in: it declares a no-op check (FIX-03). */
export function fixtureRepo(): string {
  const repo = mkdtempSync(join(tmpdir(), "modbit-browser-repo-"));
  writeFileSync(join(repo, "notes.txt"), "line 1\n");
  git(repo, "init", "-q", "-b", "main");
  mkdirSync(join(repo, ".modbit"), { recursive: true });
  writeFileSync(join(repo, ".modbit", "verification.json"), JSON.stringify({ commands: [{ id: "fixture-noop", argv: ["git", "--version"] }] }));
  git(repo, "add", "-A");
  git(repo, "-c", "user.name=t", "-c", "user.email=t@e", "commit", "-q", "-m", "base");
  return repo;
}

export async function closeApp(app: ElectronApplication): Promise<void> {
  const proc = app.process();
  await Promise.race([app.close(), new Promise<void>((r) => setTimeout(r, 15_000))]);
  if (proc.exitCode === null) proc.kill("SIGKILL");
}

/** The real app against a real Core. By default the policy names nothing local: a spec that serves its fixtures on loopback names them. */
export async function launch(dataDir: string, extraEnv: Record<string, string>): Promise<{ app: ElectronApplication; page: Page }> {
  const app = await electron.launch({
    args: [join(appDir, "dist", "main", "main.cjs")],
    env: { ...process.env, MODBIT_DATA_DIR: dataDir, MODBIT_CORE_BIN: coreBin, OPENAI_API_KEY: "", ANTHROPIC_API_KEY: "", MODBIT_SUPPRESS_OS_NOTIFICATIONS: "1", MODBIT_BROWSER_ALLOW_TARGETS: "", MODBIT_BROWSER_ALLOW_ORIGINS: "", MODBIT_BROWSER_HOSTS: "", ...extraEnv },
  });
  const page = await app.firstWindow();
  app.process().stderr?.on("data", (d: Buffer) => process.stderr.write(`[electron] ${d}`));
  await expect(page.getByTestId("core-status")).toContainText("Core connected", { timeout: 60_000 });
  return { app, page };
}

/** The task reaches review - or the failure names what the card and the model saw (a hosted runner's only witness). */
export async function readyForReview(page: Page, model: { toolTexts: string[] }, timeoutMs = 90_000): Promise<void> {
  const ready = page.getByTestId("column-readyForReview").getByTestId("task-card");
  const deadline = Date.now() + timeoutMs;
  while ((await ready.count()) === 0) {
    if (Date.now() > deadline) {
      const card = page.getByTestId("task-card").first();
      const state = await card.getAttribute("data-state").catch(() => null);
      const line = await card.getByTestId("task-screen-state").textContent().catch(() => null);
      const tail = model.toolTexts.map((t, i) => `[${i}] ${t.slice(0, 300).replace(/\n/g, " | ")}`).join("\n");
      throw new Error(`the task never reached review: state=${state} line=${line}\ntool texts:\n${tail}`);
    }
    await new Promise((r) => setTimeout(r, 500));
  }
}

/** Create the task, open its browser session and start the run, with the panel closed (the host serves the agent). */
export async function startBrowserTask(page: Page, repo: string, goal: string): Promise<{ taskId: string; bsid: string }> {
  await page.getByTestId("workspace").fill(repo);
  await page.getByTestId("goal").fill(goal);
  await page.getByTestId("run").click();
  const card = page.getByTestId("task-card").first();
  await expect(card).toContainText(goal, { timeout: 30_000 });
  const taskId = (await card.getAttribute("data-task-id"))!;
  await card.getByTestId("task-browser").click();
  const panel = page.getByTestId("browser");
  await expect(panel).toHaveAttribute("data-browser-session-id", /^[0-9a-f]{32}$/, { timeout: 30_000 });
  const bsid = (await panel.getAttribute("data-browser-session-id"))!;
  await page.getByTestId("browser-close").click();
  await expect(panel).toHaveCount(0);
  // The view has left the window before anything else is clicked (a click on a view still there would be the person's).
  await expect.poll(async () => (await page.evaluate((id) => window.modbit.describeBrowser(id), bsid))?.shown, { timeout: 15_000 }).toBe(false);
  await card.getByTestId("task-start").click();
  return { taskId, bsid };
}

/** The JSON of the last tool text that contains `marker`. */
export function lastJson<T = Record<string, any>>(texts: string[], marker: string): T {
  const t = [...texts].reverse().find((x) => x.includes(marker));
  if (!t) throw new Error(`no tool text contains ${marker}: ${texts.map((x) => x.slice(0, 80)).join(" | ")}`);
  return JSON.parse(t.slice(t.indexOf("{"), t.lastIndexOf("}") + 1)) as T;
}

/** The ref of the last snapshot's entity by role and name (optionally inside a form). */
export function refOf(texts: string[], role: string, name: string, opts: { inForm?: boolean } = {}): string {
  const j = lastJson<{ entities: { ref: string; role: string; name: string; path: string[] }[] }>(texts, '"entities":');
  const e = j.entities.find((x) => x.role === role && x.name === name && (opts.inForm === undefined || x.path.some((p) => p.startsWith("form:")) === opts.inForm));
  if (!e) throw new Error(`no ${role} “${name}” in ${JSON.stringify(j.entities.map((x) => `${x.role}:${x.name}`))}`);
  return e.ref;
}

/** Collect every Core event the renderer is sent, in `window.__events` (type and payload). */
export async function collectEvents(page: Page): Promise<void> {
  await page.evaluate(() => {
    const w = window as unknown as { __events?: { eventType: string; payload: any }[]; modbit: { onEvent(cb: (e: unknown) => void): () => void } };
    w.__events = [];
    w.modbit.onEvent((e) => w.__events!.push(e as { eventType: string; payload: any }));
  });
}

export async function eventsOf(page: Page, type: string): Promise<any[]> {
  return page.evaluate((t) => ((window as unknown as { __events?: { eventType: string; payload: any }[] }).__events ?? []).filter((e) => e.eventType === t).map((e) => e.payload), type);
}

export function sleep(ms: number): Promise<void> {
  return new Promise((r) => setTimeout(r, ms));
}

/** A gate the test opens and a model step waits on. */
export function gate(): { opened: Promise<void>; open: () => void } {
  let open!: () => void;
  const opened = new Promise<void>((r) => (open = r));
  return { opened, open };
}

/** Wait until the model has been shown `n` tool results; on a timeout, the failure carries what it was shown (a hosted runner's only witness). */
export async function toolResults(model: { toolTexts: string[] }, n: number, timeoutMs = 90_000): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  while (model.toolTexts.length < n) {
    if (Date.now() > deadline) {
      const tail = model.toolTexts.map((t, i) => `[${i}] ${t.slice(0, 420).replace(/\n/g, " | ")}`).join("\n");
      throw new Error(`the model was shown ${model.toolTexts.length} of ${n} tool results:\n${tail}`);
    }
    await new Promise((r) => setTimeout(r, 200));
  }
}
