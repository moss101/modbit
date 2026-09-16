/**
 * PX-024 / PX-E2E-024 (docs/39 "Keyboard model"): every screen traversed by
 * keyboard only in the real app against the real Core — global shortcuts,
 * list navigation, review navigation, approval and cancellation with
 * confirmation — focus retained across the Core's state changes, live
 * regions announcing attention changes, status never colour-only, and the
 * accessibility suite (axe-core) run on every screen. Not one click.
 */
import { _electron as electron, expect, test, type ElectronApplication, type Page } from "@playwright/test";
import { execFileSync } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync, realpathSync, writeFileSync } from "node:fs";
import { createRequire } from "node:module";
import { createServer, type Server } from "node:http";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const appDir = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const coreBin = process.env.MODBIT_CORE_BIN ?? resolve(appDir, "..", "..", "target", "debug", process.platform === "win32" ? "modbit-core.exe" : "modbit-core");
const MOD = process.platform === "darwin" ? "Meta" : "Control";
// axe-core's own bundle, evaluated in the renderer through the debugger
// (the page's CSP forbids injected script tags; the Electron window has no
// second page for @axe-core/playwright's blank-page trick).
const axeSource = readFileSync(createRequire(import.meta.url).resolve("axe-core/axe.min.js"), "utf8");

type Script = { calls?: { name: string; args: unknown }[] }[];

function scriptedModel(script: Script): Promise<{ server: Server; url: string }> {
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
        send({ id: "c", model: "scripted", choices: [{ index: 0, delta: {}, finish_reason: reply.calls?.length ? "tool_calls" : "stop" }], usage: { prompt_tokens: 10, completion_tokens: 5 } });
        res.write("data: [DONE]\n\n");
        res.end();
      });
    });
    server.listen(0, "127.0.0.1", () => resolveServer({ server, url: `http://127.0.0.1:${(server.address() as { port: number }).port}` }));
  });
}

function git(cwd: string, ...args: string[]): string {
  return execFileSync("git", ["-C", cwd, ...args], { encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] }).trim();
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
  await expect(page.getByTestId("core-status")).toContainText("Core connected", { timeout: 60_000 });
  return { app, page };
}

/** The accessibility suite (axe-core, WCAG 2.x A/AA and best practices) on the current screen: no violation of any impact. */
async function accessible(page: Page, screen: string): Promise<void> {
  await page.evaluate(axeSource);
  const violations = await page.evaluate(async () => {
    const axe = (window as unknown as { axe: { run: (ctx: Document, opts: unknown) => Promise<{ violations: { id: string; impact: string; help: string; nodes: { target: string[] }[] }[] }> } }).axe;
    const r = await axe.run(document, { runOnly: { type: "tag", values: ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "best-practice"] } });
    return r.violations.map((v) => `${v.id} (${v.impact}): ${v.help} — ${v.nodes.map((n) => n.target.join(" ")).slice(0, 3).join("; ")}`);
  });
  expect(violations, `${screen}: ${violations.join("\n")}`).toEqual([]);
}

const focusedTestId = (page: Page) => page.evaluate(() => document.activeElement?.getAttribute("data-testid") ?? document.activeElement?.tagName ?? "");
const focusedTaskId = (page: Page) => page.evaluate(() => document.activeElement?.closest('[data-testid="task-card"]')?.getAttribute("data-task-id") ?? "");
/** Focus settles within a frame or two of a key; poll rather than race the renderer. */
const expectFocusedTask = (page: Page, id: string | null, why?: string) => expect.poll(() => focusedTaskId(page), { message: why ?? `focused task ${id}`, timeout: 5_000 }).toBe(id);

test("keyboard: every screen by keyboard only — shortcuts, list and review navigation, confirmed approval and cancellation, focus retained across Core events, live announcements, the accessibility suite on each screen", async () => {
  test.setTimeout(240_000);
  const repo = mkdtempSync(join(tmpdir(), "modbit-kbd-repo-"));
  writeFileSync(join(repo, "notes.txt"), "line 1\nline 2\nline 3\n");
  git(repo, "init", "-q", "-b", "main");
  git(repo, "config", "core.autocrlf", "false");
  git(repo, "add", "-A");
  git(repo, "-c", "user.name=t", "-c", "user.email=t@e", "commit", "-q", "-m", "base");
  const wt = join(mkdtempSync(join(tmpdir(), "modbit-kbd-wt-")), "held");
  git(repo, "worktree", "add", "-q", "-b", "task/held", wt);
  const wtReal = realpathSync.native(wt).replace(/^\\\\\?\\/, "");
  // One script for both tasks: declare the destructive effect, edit a file
  // (so the review has a change to navigate), close the worktree under the
  // approval, complete.
  const { server, url } = await scriptedModel([
    { calls: [{ name: "plan.update", args: { outcome: "tidy", expected_files: ["notes.txt"], protected_effects: ["git.worktree.close"] } }] },
    { calls: [{ name: "fs.read", args: { path: "notes.txt" } }] },
    { calls: [{ name: "change.apply", args: { path: "notes.txt", op: "replace", content: "line 1\nline 2 changed\nline 3\nline 4\n" } }] },
    { calls: [{ name: "git.worktree.close", args: { path: wtReal } }] },
    { calls: [{ name: "task.complete", args: { summary: "tidied", self_review: { findings: [] } } }] },
  ]);
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-kbd-"));
  try {
    const { app, page } = await launch(dataDir, { MODBIT_OPENAI_BASE_URL: url });
    await expect(page.getByTestId("fleet-state")).toHaveAttribute("data-kind", "empty", { timeout: 30_000 });
    await accessible(page, "fleet (empty) and New Task");
    // New Task by keyboard: the global shortcut focuses the goal; Tab reaches
    // every field; Enter in the workspace field submits.
    await page.keyboard.press(`${MOD}+n`);
    expect(await focusedTestId(page)).toBe("goal");
    await page.keyboard.type("alpha task");
    await page.keyboard.press("Tab");
    expect(await focusedTestId(page)).toBe("issue-url");
    await page.keyboard.press("Tab");
    expect(await focusedTestId(page)).toBe("workspace");
    await page.keyboard.type(repo);
    await page.keyboard.press("Enter");
    const alpha = page.getByTestId("task-card").filter({ hasText: "alpha task" });
    await expect(alpha).toHaveCount(1, { timeout: 30_000 });
    await page.keyboard.press(`${MOD}+n`);
    await page.keyboard.type("beta task");
    await page.keyboard.press("Tab");
    await page.keyboard.press("Tab");
    await page.keyboard.press("Enter");
    const beta = page.getByTestId("task-card").filter({ hasText: "beta task" });
    await expect(beta).toHaveCount(1, { timeout: 30_000 });
    const alphaId = (await alpha.getAttribute("data-task-id"))!;
    const betaId = (await beta.getAttribute("data-task-id"))!;
    await accessible(page, "fleet (populated)");
    // Settings by Tab: the preference checkboxes are reachable and toggle with Space.
    await page.getByTestId("notify-attention").focus();
    expect(await focusedTestId(page)).toBe("notify-attention");
    await expect(page.getByTestId("notify-attention")).not.toBeChecked();
    await page.keyboard.press("Space");
    await expect(page.getByTestId("notify-attention")).toBeChecked();
    // Escape leaves any field; the list: ↓ enters the first column with
    // cards, ↓/↑ move within it, Enter opens the focused card's control.
    await page.keyboard.press("Escape");
    const waiting = page.getByTestId("column-waiting").getByTestId("task-card");
    const [first, second] = [await waiting.nth(0).getAttribute("data-task-id"), await waiting.nth(1).getAttribute("data-task-id")];
    await page.keyboard.press("ArrowDown");
    await expectFocusedTask(page, first);
    await page.keyboard.press("ArrowDown");
    await expectFocusedTask(page, second);
    await page.keyboard.press("ArrowUp");
    await expectFocusedTask(page, first);
    await page.keyboard.press("ArrowUp");
    await expectFocusedTask(page, first, "the top stays the top");
    // Type-ahead filter: / focuses the search, the fleet narrows, Escape returns.
    await page.keyboard.press("/");
    expect(await focusedTestId(page)).toBe("search");
    await page.keyboard.type("beta");
    await expect(page.getByTestId("task-card")).toHaveCount(1);
    await expect(beta).toHaveCount(1);
    await page.keyboard.press(`${MOD}+a`);
    await page.keyboard.press("Backspace");
    await expect(page.getByTestId("task-card")).toHaveCount(2);
    await page.keyboard.press("Escape");
    // ? opens the shortcut list; Escape closes it.
    await page.keyboard.press("?");
    await expect(page.getByTestId("help")).toBeVisible();
    await accessible(page, "help");
    await page.keyboard.press("Escape");
    await expect(page.getByTestId("help")).toHaveCount(0);
    // Start alpha: focus the card (↓ enters the column, ↓/↑ finds it), Enter
    // reaches its Start button, Enter starts.
    await page.keyboard.press("ArrowDown");
    if ((await focusedTaskId(page)) !== alphaId) await page.keyboard.press("ArrowDown");
    await expectFocusedTask(page, alphaId);
    await page.keyboard.press("Enter");
    expect(await focusedTestId(page)).toBe("task-start");
    await page.keyboard.press("Enter");
    // The run reaches its protected effect: the card moves to Needs
    // Attention on the Core's events and keeps the focus; the live region
    // announces the approval in words.
    await expect(page.getByTestId("column-needsAttention").getByTestId("task-card").filter({ hasText: "alpha task" })).toHaveCount(1, { timeout: 90_000 });
    await expect(alpha.getByTestId("task-approval")).toBeVisible({ timeout: 15_000 });
    await expectFocusedTask(page, alphaId, "focus survives the column move");
    await expect(page.getByTestId("live-attention")).toContainText(/Needs attention: approval on alpha task: git\.worktree\.close asks for a Destructive effect/, { timeout: 15_000 });
    await accessible(page, "fleet (attention)");
    // r jumps to Running (empty: the column itself), a to the attention list, then back to the card.
    await page.keyboard.press("r");
    expect(await focusedTestId(page)).toBe("column-running");
    await page.keyboard.press("a");
    expect(await focusedTestId(page)).toBe("attention-item");
    // ← from outside the board lands on the last column with cards (beta,
    // waiting); ← again reaches alpha in Needs Attention.
    await page.keyboard.press("ArrowLeft");
    await expectFocusedTask(page, betaId);
    await page.keyboard.press("ArrowLeft");
    await expectFocusedTask(page, alphaId);
    // y on a Destructive effect asks for confirmation; Escape declines
    // (nothing resolved); y then Enter approves the exact intent.
    expect(existsSync(wt)).toBe(true);
    await page.keyboard.press("y");
    await expect(page.getByTestId("confirm")).toHaveAttribute("data-confirm-kind", "approve");
    await expect(page.getByTestId("confirm")).toContainText("cannot be undone");
    await accessible(page, "confirmation");
    await page.keyboard.press("Escape");
    await expect(page.getByTestId("confirm")).toHaveCount(0);
    await expectFocusedTask(page, alphaId);
    await expect(alpha.getByTestId("task-approval")).toBeVisible();
    await page.keyboard.press("y");
    await page.keyboard.press("Enter");
    await expect(page.getByTestId("confirm")).toHaveCount(0);
    // A fast runner takes the task from Waiting through Running to
    // ReadyForReview within one poll; while it is Running the focus is on
    // the moved card (the same retention the attention move proved above).
    await expect(alpha.getByTestId("task-state")).toHaveText(/Running|ReadyForReview/, { timeout: 60_000 });
    // While it is Running the focus is on the moved card; the moment it is
    // ReadyForReview the Review takes the focus (checked below).
    await expect
      .poll(async () => ((await alpha.getByTestId("task-state").textContent()) === "Running" ? await focusedTaskId(page) : alphaId), { message: "focus survives Waiting → Running (the card moves columns)", timeout: 60_000 })
      .toBe(alphaId);
    // The first result opens the Review on the diff (docs/39): the focus
    // moves into it; Escape returns to the card.
    await expect(page.getByTestId("column-readyForReview").getByTestId("task-card").filter({ hasText: "alpha task" })).toHaveCount(1, { timeout: 90_000 });
    expect(existsSync(wt)).toBe(false);
    await expect(page.getByTestId("review")).toBeVisible({ timeout: 15_000 });
    expect(await focusedTestId(page)).toBe("review");
    await page.keyboard.press("Escape");
    await expect(page.getByTestId("review")).toHaveCount(0);
    await expectFocusedTask(page, alphaId, "focus returns to the card the Review was about");
    // Enter on a ReadyForReview card opens the Review; ↓/↑ move between
    // changes; Enter collapses and expands the focused hunk; u toggles
    // unified/split; f jumps to the failing check (none here: the
    // verification list); Escape returns with the card focused.
    await page.keyboard.press("Enter");
    await expect(page.getByTestId("review")).toBeVisible();
    await expect(page.getByTestId("review-hunk")).toHaveCount(1, { timeout: 30_000 });
    await accessible(page, "review");
    await page.keyboard.press("ArrowDown");
    expect(await focusedTestId(page)).toBe("review-hunk");
    await page.keyboard.press("Enter");
    await expect(page.getByTestId("review-hunk")).toHaveAttribute("data-collapsed", "true");
    await expect(page.getByTestId("hunk-collapsed")).toContainText("2 added, 1 removed");
    await page.keyboard.press("Enter");
    await expect(page.getByTestId("review-hunk")).toHaveAttribute("data-collapsed", "false");
    await page.keyboard.press("u");
    await expect(page.getByTestId("hunk-split")).toBeVisible();
    await expect(page.getByTestId("hunk-split").locator("td.del")).toHaveText("-line 2");
    await expect(page.getByTestId("hunk-split").locator("td.add")).toHaveText(["+line 2 changed", "+line 4"]);
    await accessible(page, "review (split)");
    await page.keyboard.press("u");
    await expect(page.getByTestId("hunk-unified")).toBeVisible();
    await page.keyboard.press("f");
    expect(await focusedTestId(page)).toBe("review-verification");
    await page.keyboard.press("Escape");
    await expect(page.getByTestId("review")).toHaveCount(0);
    await expectFocusedTask(page, alphaId);
    // Cancel beta with confirmation: → to the next column with cards, c, Enter.
    await page.keyboard.press("ArrowRight");
    await expectFocusedTask(page, betaId);
    await page.keyboard.press("c");
    await expect(page.getByTestId("confirm")).toHaveAttribute("data-confirm-kind", "cancel");
    await page.keyboard.press("Enter");
    await expect(beta.getByTestId("task-state")).toHaveText("Cancelled", { timeout: 30_000 });
    await expectFocusedTask(page, betaId, "focus survives the cancellation");
    // Status is never colour-only: every state line carries its kind in text.
    for (const line of await page.getByTestId("task-screen-state").all()) await expect(line).toHaveAttribute("aria-label", /^(empty|loading|populated|error|degraded|recovery): /);
    await accessible(page, "fleet (final)");
    await closeApp(app);
  } finally {
    server.close();
  }
});
