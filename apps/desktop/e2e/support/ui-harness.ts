/**
 * Shared helpers for the shell and gallery specs (REQ-PX-044, REQ-PX-045):
 * launching the packaged app, the axe-core pass, window sizing through the
 * main process, and a scripted OpenAI-compatible model for the one flow that
 * needs a task with an artifact. No platform-specific shortcut: `MOD` is the
 * primary modifier (Meta on macOS, Control elsewhere).
 */
import { _electron as electron, expect, type ElectronApplication, type Page } from "@playwright/test";
import { execFileSync } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import { createRequire } from "node:module";
import { createServer, type Server } from "node:http";
import { readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

export const appDir = resolve(dirname(fileURLToPath(import.meta.url)), "..", "..");
export const coreBin = process.env.MODBIT_CORE_BIN ?? resolve(appDir, "..", "..", "target", "debug", process.platform === "win32" ? "modbit-core.exe" : "modbit-core");
export const MOD = process.platform === "darwin" ? "Meta" : "Control";

// axe-core's own bundle, evaluated in the renderer (the page's CSP forbids injected script tags).
const axeSource = readFileSync(createRequire(import.meta.url).resolve("axe-core/axe.min.js"), "utf8");

// One close path for every spec (close-app.ts): a plain kill leaves Windows helpers holding the worker's pipes.
export { closeApp } from "./close-app.ts";

export interface LaunchOptions {
  /** The build directory under apps/desktop (default "dist"). */
  outDir?: string;
  /** Wait for the real Core to connect (default true). */
  waitForCore?: boolean;
  env?: Record<string, string>;
}

export async function launch(dataDir: string, opts: LaunchOptions = {}): Promise<{ app: ElectronApplication; page: Page }> {
  const app = await electron.launch({
    // A window the OS reports as covered gets no animation frames, and xterm.js paints its rows in one: on a Windows runner's desktop the
    // terminal's text reached the DOM only when something woke the page (seconds late). The run must not depend on the runner's window stacking.
    args: ["--disable-features=CalculateNativeWinOcclusion", "--disable-backgrounding-occluded-windows", "--disable-renderer-backgrounding", join(appDir, opts.outDir ?? "dist", "main", "main.cjs")],
    env: { ...process.env, MODBIT_DATA_DIR: dataDir, MODBIT_CORE_BIN: coreBin, OPENAI_API_KEY: "", ANTHROPIC_API_KEY: "", MODBIT_SUPPRESS_OS_NOTIFICATIONS: "1", MODBIT_QUIT_PROMPT: "off", ...opts.env },
  });
  const page = await app.firstWindow();
  app.process().stderr?.on("data", (d: Buffer) => process.stderr.write(`[electron] ${d}`));
  if (opts.waitForCore !== false) await expect(page.getByTestId("core-status")).toContainText("Core connected", { timeout: 60_000 });
  return { app, page };
}

/** The accessibility suite (axe-core, WCAG 2.x A/AA and best practices) on the document or one subtree: no violation of any impact. */
export async function accessible(page: Page, what: string, selector?: string): Promise<void> {
  // A colour mid-transition is not a colour: let any CSS transition, and any animation that ends (the streamed words' fade from dim to
  // full, which axe measured at its dim start on a slow runner), finish before measuring. Animations that never end (a blinking cursor) are left running.
  await page.evaluate(() =>
    Promise.all(
      document
        .getAnimations()
        .filter((a) => a instanceof CSSTransition || (a instanceof CSSAnimation && a.effect?.getComputedTiming().iterations !== Infinity))
        .map((a) => a.finished.catch(() => undefined)),
    ),
  );
  await page.evaluate(axeSource);
  const violations = await page.evaluate(async (sel) => {
    const axe = (window as unknown as { axe: { run: (ctx: Element | Document, opts: unknown) => Promise<{ violations: { id: string; impact: string; help: string; nodes: { target: string[] }[] }[] }> } }).axe;
    const ctx = sel ? document.querySelector(sel)! : document;
    const r = await axe.run(ctx, { runOnly: { type: "tag", values: ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "best-practice"] } });
    return r.violations.map((v) => `${v.id} (${v.impact}): ${v.help} — ${v.nodes.map((n) => n.target.join(" ")).slice(0, 3).join("; ")}`);
  }, selector ?? null);
  expect(violations, `${what}: ${violations.join("\n")}`).toEqual([]);
}

/**
 * Lifts the window's 900 x 600 minimum (REQ-PX-049) from the test process, for the one spec that exercises the
 * renderer's single-pane layout below it. The product keeps the minimum; this only lets that layout be measured.
 */
export async function liftMinimumSize(app: ElectronApplication): Promise<void> {
  // 1 x 1, not 0 x 0: Electron reads 0 x 0 as "no constraint" and on macOS and
  // Windows leaves the window's existing native minimum in place.
  const minimum = await app.evaluate(({ BrowserWindow }) => {
    const w = BrowserWindow.getAllWindows()[0]!;
    w.setMinimumSize(1, 1);
    return w.getMinimumSize();
  });
  expect(minimum, "the window's minimum size was lifted").toEqual([1, 1]);
}

/** Sets the window's content size from the main process (the real window, not an emulation). */
export async function setContentSize(app: ElectronApplication, page: Page, width: number, height: number): Promise<void> {
  await app.evaluate(({ BrowserWindow }, [w, h]) => {
    BrowserWindow.getAllWindows()[0]!.setContentSize(w!, h!);
  }, [width, height]);
  await expect.poll(() => page.evaluate(() => window.innerWidth)).toBe(width);
}

export const box = async (page: Page, testId: string) => {
  const b = await page.getByTestId(testId).boundingBox();
  if (!b) throw new Error(`no box for ${testId}`);
  return b;
};

export const focusedTestId = (page: Page) => page.evaluate(() => document.activeElement?.getAttribute("data-testid") ?? document.activeElement?.tagName ?? "");

export function git(cwd: string, ...args: string[]): string {
  return execFileSync("git", ["-C", cwd, ...args], { encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] });
}

/** A repository with a check, so a task can complete (FIX-03). */
export function makeRepo(repo: string): string {
  writeFileSync(join(repo, "notes.txt"), "line 1\nline 2\nline 3\n");
  git(repo, "init", "-q", "-b", "main");
  mkdirSync(join(repo, ".modbit"), { recursive: true });
  writeFileSync(join(repo, ".modbit", "verification.json"), JSON.stringify({ commands: [{ id: "fixture-noop", argv: ["git", "--version"] }] }));
  git(repo, "add", "-A");
  git(repo, "-c", "user.name=t", "-c", "user.email=t@e", "commit", "-q", "-m", "base");
  return repo;
}

/** Scripted OpenAI-compatible model: the reply is chosen by the number of tool results in the request. */
export function scriptedModel(script: { text?: string; calls?: { name: string; args: unknown }[] }[]): Promise<{ server: Server; url: string }> {
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
    server.listen(0, "127.0.0.1", () => resolveServer({ server, url: `http://127.0.0.1:${(server.address() as { port: number }).port}` }));
  });
}
