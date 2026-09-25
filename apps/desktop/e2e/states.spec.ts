/**
 * PX-023 (docs/39 "Screen flows and states", "Notification model"): every
 * screen's empty, loading, populated, error, degraded and recovery states
 * against the real Electron app and the real modbit-core, each forced by a
 * real cause — a provider that refuses connections, a Core that aborts
 * before committing a tool outcome, a pull request the person denies — and
 * each naming its cause, next action and evidence from what the Core said.
 * Notifications: attention, completion and failure only, coalesced per
 * task, deep-linked; OS delivery opt-in and silent in quiet hours, read
 * back from main's delivery log.
 */
import { _electron as electron, expect, test, type ElectronApplication, type Page } from "@playwright/test";
import { execFileSync } from "node:child_process";
import { existsSync, mkdtempSync, realpathSync, writeFileSync } from "node:fs";
import { createServer, type Server } from "node:http";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const appDir = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const coreBin = process.env.MODBIT_CORE_BIN ?? resolve(appDir, "..", "..", "target", "debug", process.platform === "win32" ? "modbit-core.exe" : "modbit-core");
const TOKEN = "ghp_e2e_token_0023";

type Script = { calls?: { name: string; args: unknown }[] }[];

/** A port nothing listens on yet: the provider is "down" until `scriptedModel` takes it. */
function reservePort(): Promise<number> {
  return new Promise((resolvePort) => {
    const s = createServer();
    s.listen(0, "127.0.0.1", () => {
      const port = (s.address() as { port: number }).port;
      s.close(() => resolvePort(port));
    });
  });
}

function scriptedModel(port: number, script: Script): Promise<Server> {
  return new Promise((resolveServer) => {
    const server = createServer((req, res) => {
      let body = "";
      req.on("data", (c) => (body += c));
      req.on("end", () => {
        const parsed = JSON.parse(body || "{}") as { messages?: { role: string; content?: string }[] };
        const results = (parsed.messages ?? []).filter((m) => m.role === "tool").length;
        const reply = script[results] ?? {};
        process.stderr.write(`[scripted-model] tool results so far: ${results}; roles: ${(parsed.messages ?? []).map((m) => m.role).join(",")}; reply: ${(reply.calls ?? []).map((c) => c.name).join(",") || "stop"}; last result: ${String((parsed.messages ?? []).filter((m) => m.role === "tool").at(-1)?.content ?? "").slice(0, 400).replace(/\n/g, " ")}\n`);
        res.writeHead(200, { "content-type": "text/event-stream" });
        const send = (o: unknown) => res.write(`data: ${JSON.stringify(o)}\n\n`);
        (reply.calls ?? []).forEach((c, i) => send({ id: "c", model: "scripted", choices: [{ index: 0, delta: { tool_calls: [{ index: i, id: `call_${results}_${i}`, type: "function", function: { name: c.name, arguments: JSON.stringify(c.args) } }] }, finish_reason: null }] }));
        if (!reply.calls?.length) send({ id: "c", model: "scripted", choices: [{ index: 0, delta: { content: "Nothing further." }, finish_reason: null }] });
        send({ id: "c", model: "scripted", choices: [{ index: 0, delta: {}, finish_reason: reply.calls?.length ? "tool_calls" : "stop" }], usage: { prompt_tokens: 10, completion_tokens: 5 } });
        res.write("data: [DONE]\n\n");
        res.end();
      });
    });
    server.listen(port, "127.0.0.1", () => resolveServer(server));
  });
}

/** A wire-faithful GitHub REST fake for pull requests: POST (422 on a duplicate head), GET by head, bearer checks, a request log. */
function fakeGithub(): Promise<{ server: Server; url: string; requests: { method: string; path: string; authorized: boolean }[]; pulls: { number: number; head: string; sha: string }[] }> {
  const requests: { method: string; path: string; authorized: boolean }[] = [];
  const pulls: { number: number; head: string; sha: string }[] = [];
  return new Promise((resolveServer) => {
    const server = createServer((req, res) => {
      let body = "";
      req.on("data", (c) => (body += c));
      req.on("end", () => {
        const authorized = req.headers.authorization === `Bearer ${TOKEN}`;
        const url = req.url ?? "";
        requests.push({ method: req.method ?? "", path: url, authorized });
        process.stderr.write(`[fake-github] ${req.method} ${url} authorized=${authorized}\n`);
        const reply = (status: number, payload: unknown) => {
          res.writeHead(status, { "content-type": "application/json" });
          res.end(JSON.stringify(payload));
        };
        if (!authorized) return reply(401, { message: "Bad credentials" });
        const [path, query = ""] = url.split("?");
        const view = (p: { number: number; head: string; sha: string }, o: string, r: string) => ({ number: p.number, url: `http://fake/repos/${o}/${r}/pulls/${p.number}`, html_url: `https://github.test/${o}/${r}/pull/${p.number}`, state: "open", title: "t", head: { ref: p.head, sha: p.sha }, base: { ref: "main" } });
        const create = /^\/repos\/([^/]+)\/([^/]+)\/pulls$/.exec(path ?? "");
        if (create && req.method === "POST") {
          const b = JSON.parse(body || "{}") as { head?: string };
          const head = b.head ?? "";
          if (pulls.some((p) => p.head === head)) return reply(422, { message: "Validation Failed", errors: [{ resource: "PullRequest", code: "custom", message: `A pull request already exists for ${create[1]}:${head}.` }] });
          const p = { number: pulls.length + 1, head, sha: "f".repeat(40) };
          pulls.push(p);
          return reply(201, view(p, create[1]!, create[2]!));
        }
        if (create && req.method === "GET") {
          const head = /head=[^:]+:([^&]+)/.exec(query)?.[1] ?? "";
          return reply(200, pulls.filter((p) => p.head === head).map((p) => view(p, create[1]!, create[2]!)));
        }
        return reply(404, { message: "Not Found" });
      });
    });
    server.listen(0, "127.0.0.1", () => resolveServer({ server, url: `http://127.0.0.1:${(server.address() as { port: number }).port}`, requests, pulls }));
  });
}

function git(cwd: string, ...args: string[]): string {
  return execFileSync("git", ["-C", cwd, ...args], { encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] }).trim();
}

/** A repository whose `origin` is the forge URL, rewritten by git itself to a local bare remote. */
function repoWithRemote(): { repo: string; bare: string } {
  const repo = mkdtempSync(join(tmpdir(), "modbit-states-repo-"));
  const bare = mkdtempSync(join(tmpdir(), "modbit-states-bare-"));
  writeFileSync(join(repo, "notes.txt"), "line 1\nline 2\n");
  git(repo, "init", "-q", "-b", "main");
  git(repo, "config", "core.autocrlf", "false");
  git(repo, "add", "-A");
  git(repo, "-c", "user.name=t", "-c", "user.email=t@e", "commit", "-q", "-m", "base");
  execFileSync("git", ["init", "-q", "--bare", "-b", "main", bare]);
  git(repo, "remote", "add", "origin", "https://github.test/o/r.git");
  git(repo, "config", `url.${bare}.insteadOf`, "https://github.test/o/r.git");
  return { repo, bare };
}

async function closeApp(app: ElectronApplication): Promise<void> {
  const proc = app.process();
  await Promise.race([app.close(), new Promise<void>((r) => setTimeout(r, 15_000))]);
  if (proc.exitCode === null) proc.kill("SIGKILL");
}

async function launch(dataDir: string, extraEnv: Record<string, string>): Promise<{ app: ElectronApplication; page: Page }> {
  const app = await electron.launch({ args: [join(appDir, "dist", "main", "main.cjs")], env: { ...process.env, MODBIT_DATA_DIR: dataDir, MODBIT_CORE_BIN: coreBin, OPENAI_API_KEY: "", ANTHROPIC_API_KEY: "", MODBIT_SUPPRESS_OS_NOTIFICATIONS: "1", ...extraEnv } });
  const page = await app.firstWindow();
  app.process().stderr?.on("data", (d: Buffer) => process.stderr.write(`[electron] ${d}`));
  app.process().stdout?.on("data", (d: Buffer) => process.stderr.write(`[electron] ${d}`));
  await expect(page.getByTestId("core-status")).toContainText("Core connected", { timeout: 60_000 });
  return { app, page };
}

test("screen states: provider down, human continuation, review gate and a denied then opened pull request; notifications coalesce, deep-link and honour opt-in and quiet hours", async () => {
  test.setTimeout(240_000);
  const { repo, bare } = repoWithRemote();
  const port = await reservePort();
  const gh = await fakeGithub();
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-states-"));
  const env = { MODBIT_OPENAI_BASE_URL: `http://127.0.0.1:${port}`, MODBIT_GITHUB_API_BASE_URL: gh.url, MODBIT_GITHUB_TOKEN: TOKEN, MODBIT_GITHUB_WEB_HOST: "github.test" };
  let model: Server | null = null;
  try {
    const { app, page } = await launch(dataDir, env);
    // Fleet: empty, from the Core's projection. Settings: the keychain state
    // is whatever main reported, never assumed.
    await expect(page.getByTestId("fleet-state")).toHaveAttribute("data-kind", "empty", { timeout: 30_000 });
    const status = await page.evaluate(() => window.modbit.providerStatus());
    const settings = page.getByTestId("settings-state");
    await expect(settings).toHaveAttribute("data-kind", status.keychainAvailable ? "populated" : "degraded");
    if (!status.keychainAvailable) {
      await expect(settings.getByTestId("settings-state-label")).toHaveText("keychain unavailable");
      await expect(settings.getByTestId("settings-state-evidence")).toContainText("keychainAvailable=false");
    }
    // Notifications: opt in to attention and completion; quiet hours cover now.
    const hour = new Date().getHours();
    await page.getByTestId("notify-attention").check();
    await page.getByTestId("notify-completion").check();
    await page.getByTestId("notify-quiet").check();
    await page.getByTestId("notify-quiet-start").fill(String(hour));
    await page.getByTestId("notify-quiet-end").fill(String((hour + 1) % 24));
    // New Task: the composer names the repository as not yet trusted; running trusts it.
    await page.getByTestId("workspace").fill(repo);
    await expect(page.getByTestId("composer-state-label")).toContainText("not yet trusted");
    await page.getByTestId("goal").fill("append a line to notes");
    await page.getByTestId("run").click();
    const card = page.getByTestId("task-card").first();
    await expect(card).toContainText("append a line to notes", { timeout: 30_000 });
    await expect(page.getByTestId("composer-state-label")).toHaveText("ready");
    await expect(page.getByTestId("fleet-state")).toHaveAttribute("data-kind", "populated");
    await expect(card.getByTestId("task-screen-state")).toHaveAttribute("data-kind", "populated");
    await expect(card.getByTestId("task-screen-state-label")).toHaveText("queued · drafting");
    const taskId = (await card.getAttribute("data-task-id"))!;
    // Provider down: nothing listens on the provider port. The Core's
    // diagnostic (PROVIDER / CONNECT_FAILED) is the cause the task and the
    // fleet name, with the user action the Core recorded and the offset.
    await card.getByTestId("task-start").click();
    const taskState = card.getByTestId("task-screen-state");
    await expect(taskState).toHaveAttribute("data-kind", "degraded", { timeout: 60_000 });
    await expect(taskState.getByTestId("task-screen-state-label")).toHaveText("provider stopped the task");
    await expect(taskState.getByTestId("task-screen-state-cause")).toContainText("CONNECT_FAILED");
    await expect(taskState.getByTestId("task-screen-state-next")).toContainText("check the provider endpoint is reachable");
    await expect(taskState.getByTestId("task-screen-state-evidence")).toContainText(/offset \d+|event:/);
    const fleet = page.getByTestId("fleet-state");
    await expect(fleet).toHaveAttribute("data-kind", "degraded");
    await expect(fleet.getByTestId("fleet-state-label")).toHaveText("provider down");
    await expect(fleet.getByTestId("fleet-state-cause")).toContainText("CONNECT_FAILED");
    await expect(fleet.getByTestId("fleet-state-evidence")).toContainText(taskId.slice(0, 8));
    // One attention notification for the task (the Core's FAILURE item),
    // deep-linked to the task; not delivered to the OS inside quiet hours.
    const notification = page.getByTestId("notification");
    await expect(notification).toHaveCount(1, { timeout: 15_000 });
    await expect(notification).toHaveAttribute("data-kind", "attention");
    await expect(notification).toHaveAttribute("data-id", `task:${taskId}`);
    await expect(notification).toContainText("FAILURE:");
    await expect(notification).toContainText("StartTask to retry");
    await expect(notification).toHaveAttribute("data-delivered", "false");
    expect(await page.evaluate(() => window.modbit.notificationLog())).toEqual([]);
    await notification.getByTestId("notification-open").click();
    await expect(card).toBeFocused();
    // Quiet hours off: what is new from now on reaches the OS (main's log).
    await page.getByTestId("notify-quiet").uncheck();
    // The provider comes back; the person resumes the task; the ordinary
    // loop runs it to review. Completion notifies and deep-links to Review.
    model = await scriptedModel(port, [
      { calls: [{ name: "fs.read", args: { path: "notes.txt" } }] },
      { calls: [{ name: "plan.update", args: { outcome: "append a line to notes", expected_files: ["notes.txt"] } }] },
      { calls: [{ name: "change.apply", args: { path: "notes.txt", op: "replace", content: "line 1\nline 2\nline 3\n" } }] },
      { calls: [{ name: "task.complete", args: { summary: "appended", self_review: { findings: [] } } }] },
    ]);
    await card.getByTestId("task-start").click();
    await expect(page.getByTestId("column-readyForReview").getByTestId("task-card")).toHaveCount(1, { timeout: 90_000 });
    await expect(fleet).toHaveAttribute("data-kind", "populated");
    await expect(page.getByTestId("review")).toBeVisible({ timeout: 15_000 });
    await expect(notification).toHaveAttribute("data-kind", "completion", { timeout: 15_000 });
    await expect(notification).toHaveAttribute("data-delivered", "true", { timeout: 15_000 });
    const log = await page.evaluate(() => window.modbit.notificationLog());
    expect(log.map((l) => l.id)).toEqual([`task:${taskId}`]);
    expect(log[0]!.shown).toBe(false);
    expect(log[0]!.body).toContain("review the result");
    // Review: its state follows the acceptance gate the Core recorded on the run.
    const reviewState = page.getByTestId("review-state");
    await expect(reviewState).toHaveAttribute("data-kind", /populated|degraded/, { timeout: 30_000 });
    const gate = await card.getByTestId("task-evidence").textContent();
    if (gate?.includes("INCONCLUSIVE")) {
      await expect(reviewState.getByTestId("review-state-label")).toHaveText("verification incomplete");
      await expect(reviewState.getByTestId("review-state-evidence")).toContainText("gate ");
    } else if (gate?.includes("REJECT")) {
      await expect(reviewState.getByTestId("review-state-label")).toHaveText("acceptance rejected");
    } else {
      await expect(reviewState).toHaveAttribute("data-kind", "populated");
    }
    // Pull request: the push and the forge write are one protected effect
    // bound to an intent. Denied: the branch stays local, the state names
    // the approval; nothing reached the forge.
    const pr = page.getByTestId("pull-request");
    await page.getByTestId("pr-open").click();
    await expect(pr).toHaveAttribute("data-status", "APPROVAL_PENDING", { timeout: 30_000 });
    const intent = await page.getByTestId("pr-intent").textContent();
    expect(intent).toMatch(/^[0-9a-f]{16}$/);
    await page.getByTestId("pr-deny").click();
    await expect(pr).toHaveAttribute("data-status", "DENIED", { timeout: 30_000 });
    await expect(reviewState).toHaveAttribute("data-kind", "degraded");
    await expect(reviewState.getByTestId("review-state-label")).toHaveText("pull request push denied or failed");
    await expect(reviewState.getByTestId("review-state-cause")).toContainText("the branch stays local");
    await expect(reviewState.getByTestId("review-state-evidence")).toContainText("approval ");
    expect(gh.requests.filter((r) => r.method === "POST")).toEqual([]);
    expect(() => git(bare, "rev-parse", "--verify", `refs/heads/modbit/pr-${taskId.slice(0, 12)}`)).toThrow();
    // Try again: a new attempt, a new approval; approved, the branch is
    // pushed and the pull request opened with the Core's token.
    await page.getByTestId("pr-open").click();
    await expect(pr).toHaveAttribute("data-status", "APPROVAL_PENDING", { timeout: 30_000 });
    await page.getByTestId("pr-approve").click();
    await expect(pr).toHaveAttribute("data-status", "OPENED", { timeout: 60_000 });
    await expect(page.getByTestId("pr-result")).toContainText("opened #1 at https://github.test/o/r/pull/1");
    await expect(page.getByTestId("pr-result")).toContainText("1 receipt(s)");
    expect(gh.requests.filter((r) => r.method === "POST").every((r) => r.authorized)).toBe(true);
    expect(gh.pulls.map((p) => p.head)).toEqual([`modbit/pr-${taskId.slice(0, 12)}`]);
    expect(git(bare, "rev-parse", "--verify", `refs/heads/modbit/pr-${taskId.slice(0, 12)}`)).toMatch(/^[0-9a-f]{40}$/);
    await expect(reviewState).toHaveAttribute("data-kind", /populated|degraded/);
    await expect(reviewState.getByTestId("review-state-label")).not.toHaveText("pull request push denied or failed");
    await closeApp(app);
  } finally {
    model?.close();
    gh.server.close();
  }
});

test("screen states: a Core that dies before committing a tool outcome comes back reconciling — degraded, recovery, then the task's unknown outcome with its evidence; the restart notifies", async () => {
  test.setTimeout(180_000);
  const { repo } = repoWithRemote();
  const port = await reservePort();
  const model = await scriptedModel(port, [
    { calls: [{ name: "plan.update", args: { outcome: "append a line to notes", expected_files: ["notes.txt"] } }] },
    { calls: [{ name: "fs.read", args: { path: "notes.txt" } }] },
    { calls: [{ name: "change.apply", args: { path: "notes.txt", op: "replace", content: "line 1\nline 2\nline 3\n" } }] },
    { calls: [{ name: "task.complete", args: { summary: "appended", self_review: { findings: [] } } }] },
  ]);
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-states2-"));
  try {
    // The Core aborts right before committing the write's outcome (the
    // second successful call; docs/54 kill point): the write happened, its
    // record did not. A read in that position would be cancelled, not
    // unknown.
    const { app, page } = await launch(dataDir, { MODBIT_OPENAI_BASE_URL: `http://127.0.0.1:${port}`, MODBIT_FAULT_KILL_BEFORE_EVENT: "ToolCallSucceeded:2" });
    await page.getByTestId("notify-attention").check();
    await page.getByTestId("workspace").fill(repo);
    await page.getByTestId("goal").fill("append a line to notes");
    await page.getByTestId("run").click();
    const card = page.getByTestId("task-card").first();
    await expect(card).toContainText("append a line to notes", { timeout: 30_000 });
    await card.getByTestId("task-start").click();
    const fleet = page.getByTestId("fleet-state");
    // Degraded while the Core is down: the last persisted state stays, the
    // cause is the exit, nothing is invented. Then recovery: what the Core
    // verified and what is still to reconcile.
    // A fast runner restarts the Core within one poll: the degraded state
    // is on the line's record (`data-seen`) whether or not it is still shown.
    await expect(fleet).toHaveAttribute("data-seen", /degraded:Core restarting/, { timeout: 60_000 });
    // If still shown, the degraded render is read in one snapshot, never by
    // retrying locators: a restart landing between two reads would move the
    // line to recovery (and the card off its cursor) mid-assertion. The fleet
    // line and the card derive from the same Core status, so one read is one
    // render.
    const shown = await page.evaluate(() => {
      const fleetLine = document.querySelector('[data-testid="fleet-state"]');
      const cardLine = document.querySelector('[data-testid="task-card"]')?.querySelector('[data-testid="task-screen-state"]');
      const text = (line: Element | null | undefined, testid: string) => line?.querySelector(`[data-testid="${testid}"]`)?.textContent ?? null;
      return { kind: fleetLine?.getAttribute("data-kind") ?? null, label: text(fleetLine, "fleet-state-label"), cause: text(fleetLine, "fleet-state-cause"), taskLabel: text(cardLine, "task-screen-state-label"), taskEvidence: text(cardLine, "task-screen-state-evidence") };
    });
    process.stderr.write(`[degraded-render] ${JSON.stringify(shown)}\n`);
    if (shown.kind === "degraded") {
      expect(shown.label).toBe("Core restarting");
      expect(shown.cause).toContain("the Core stopped");
      // The task, meanwhile: reconnecting by cursor (the same render).
      expect(shown.taskLabel).toBe("reconnecting by cursor");
      expect(shown.taskEvidence).toMatch(/cursor \d+/);
    }
    await expect(page.getByTestId("core-status")).toContainText("Core connected", { timeout: 60_000 });
    await expect(fleet).toHaveAttribute("data-kind", "recovery", { timeout: 30_000 });
    await expect(fleet.getByTestId("fleet-state-cause")).toContainText(/verified \d+ event\(s\) across \d+ aggregate\(s\) and recovered 1 session\(s\) and 1 task\(s\)/);
    await expect(fleet.getByTestId("fleet-state-evidence")).toContainText("recovery report of boot 2");
    // The task: an unknown outcome under reconciliation, from the Core's
    // RESTART_RECONCILING diagnostic — cause, the user's options, evidence.
    const taskState = card.getByTestId("task-screen-state");
    await expect(taskState).toHaveAttribute("data-kind", "degraded", { timeout: 60_000 });
    await expect(taskState.getByTestId("task-screen-state-label")).toHaveText("unknown tool outcome under reconciliation");
    await expect(taskState.getByTestId("task-screen-state-cause")).toContainText("RESTART_RECONCILING");
    await expect(taskState.getByTestId("task-screen-state-next")).toContainText("ReconcileToolCall");
    await expect(taskState.getByTestId("task-screen-state-evidence")).not.toBeEmpty();
    await expect(card.getByTestId("task-state")).toHaveText("Waiting");
    // The restart is an attention notification (the Core's RESTART item),
    // delivered once opted in and outside quiet hours.
    const notification = page.getByTestId("notification");
    await expect(notification).toHaveCount(1, { timeout: 15_000 });
    await expect(notification).toHaveAttribute("data-kind", "attention");
    await expect(notification).toContainText("RESTART:");
    await expect(notification).toHaveAttribute("data-delivered", "true", { timeout: 15_000 });
    await closeApp(app);
  } finally {
    model.close();
  }
});

test("task workspace: awaiting your answer (the agent's typed question) and awaiting approval with the exact intent, decided on the card; the reasons notify and coalesce", async () => {
  test.setTimeout(180_000);
  const { repo } = repoWithRemote();
  // A worktree the agent will close: a destructive effect the plan declares
  // and the person approves (docs/23), never run before the approval.
  const wt = join(mkdtempSync(join(tmpdir(), "modbit-states-wt-")), "held");
  git(repo, "worktree", "add", "-q", "-b", "task/held", wt);
  const wtReal = realpathSync.native(wt).replace(/^\\\\\?\\/, "");
  const port = await reservePort();
  const model = await scriptedModel(port, [
    { calls: [{ name: "plan.update", args: { outcome: "close the stale worktree", expected_files: [], protected_effects: ["git.worktree.close"] } }] },
    { calls: [{ name: "user.ask", args: { question: "Close the held worktree too?", options: [{ id: "yes", label: "yes, close it" }, { id: "no", label: "no, keep it" }], reason: "protected_effect" } }] },
    { calls: [{ name: "git.worktree.close", args: { path: wtReal } }] },
    { calls: [{ name: "task.complete", args: { summary: "closed", self_review: { findings: [] } } }] },
  ]);
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-states3-"));
  try {
    const { app, page } = await launch(dataDir, { MODBIT_OPENAI_BASE_URL: `http://127.0.0.1:${port}` });
    await page.getByTestId("workspace").fill(repo);
    await page.getByTestId("goal").fill("close the stale worktree");
    await page.getByTestId("run").click();
    const card = page.getByTestId("task-card").first();
    await expect(card).toContainText("close the stale worktree", { timeout: 30_000 });
    await card.getByTestId("task-start").click();
    // The question: the task waits on the person, the state names the
    // question and its id, the notification is the Core's QUESTION item.
    const taskState = card.getByTestId("task-screen-state");
    await expect(taskState).toHaveAttribute("data-kind", "degraded", { timeout: 60_000 });
    await expect(card).toHaveAttribute("data-phase", "awaitingHuman");
    await expect(taskState.getByTestId("task-screen-state-label")).toHaveText("awaiting your answer");
    await expect(taskState.getByTestId("task-screen-state-cause")).toHaveText("Close the held worktree too?");
    await expect(taskState.getByTestId("task-screen-state-next")).toContainText("RespondToQuestion");
    await expect(taskState.getByTestId("task-screen-state-evidence")).toContainText(/question \S+/);
    const question = card.getByTestId("task-question");
    await expect(question.getByTestId("task-question-text")).toHaveText("Close the held worktree too?");
    await expect(question.getByTestId("task-answer-option")).toHaveCount(2);
    const notification = page.getByTestId("notification");
    await expect(notification).toHaveCount(1, { timeout: 15_000 });
    await expect(notification).toHaveAttribute("data-kind", "attention");
    await expect(notification).toContainText("QUESTION: Close the held worktree too?");
    // Answer on the card, then resume: the answer is the tool's result.
    await question.getByTestId("task-answer-option").filter({ hasText: "yes, close it" }).click();
    await expect(card.getByTestId("task-decision")).toContainText("answered question", { timeout: 15_000 });
    await expect(card.getByTestId("task-question")).toHaveCount(0, { timeout: 15_000 });
    await card.getByTestId("task-start").click();
    // The approval: the exact intent the decision binds, from the Core's
    // ApprovalRequested; the effect has not run.
    await expect(taskState.getByTestId("task-screen-state-label")).toHaveText("awaiting approval", { timeout: 60_000 });
    const intent = await card.getByTestId("task-approval-intent").textContent();
    expect(intent).toMatch(/^[0-9a-f]{16}$/);
    await expect(taskState.getByTestId("task-screen-state-next")).toContainText(intent!.slice(0, 12));
    await expect(taskState.getByTestId("task-screen-state-evidence")).toContainText(/approval [0-9a-f-]{32,36}/);
    await expect(taskState.getByTestId("task-screen-state-cause")).toContainText("git.worktree.close asks for a Destructive effect");
    expect(existsSync(wt)).toBe(true);
    await expect(notification).toContainText("APPROVAL: git.worktree.close", { timeout: 15_000 });
    // Approved: the same call runs once, the task reaches review.
    await card.getByTestId("task-approve").click();
    await expect(card.getByTestId("task-decision")).toContainText("approved at offset", { timeout: 15_000 });
    await expect(page.getByTestId("column-readyForReview").getByTestId("task-card")).toHaveCount(1, { timeout: 90_000 });
    expect(existsSync(wt)).toBe(false);
    await expect(notification).toHaveAttribute("data-kind", "completion", { timeout: 15_000 });
    await closeApp(app);
  } finally {
    model.close();
  }
});
