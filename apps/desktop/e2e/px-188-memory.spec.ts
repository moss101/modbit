/**
 * REQ-PX-188 / QUAL-PX-188, the Memory view, on the real stack: the packaged
 * Electron app, a real Core over its socket and event log, and the
 * repository's scripted OpenAI-compatible model, whose requests are recorded.
 *
 * In order, on one app: a person writes a private note with personal detail;
 * the view lists it with its scope and source; sharing it into the
 * organization stores a copy with the personal detail removed (the private
 * original stays); once promoted by the Core's own command the next real
 * model request carries only the redacted copy; deleting the original removes
 * it from the list, the store and the next request; the view still lists the
 * same after the app is closed and relaunched on the same data directory.
 *
 * Every wait is for a condition the Core or the page reports.
 */
import { expect, test, type ElectronApplication, type Page } from "@playwright/test";
import { mkdtempSync, realpathSync, writeFileSync } from "node:fs";
import type { Server } from "node:http";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { steppedModel, type Step } from "./support/apps-harness.ts";
import { closeApp, launch, makeRepo, setContentSize } from "./support/ui-harness.ts";

test.describe.configure({ mode: "serial" });

let app: ElectronApplication;
let page: Page;
let server: Server;
let modelUrl = "";
let repo: string;
let dataDir: string;
let sessionId = "";
let taskId = "";
const bodies: string[] = [];
let n = 0;

const PRIVATE = "send release notes to jane.doe@example.com; her checkout is /Users/janedoe/app";
const row = (id: string) => page.locator(`[data-testid="customize-memory-item"][data-id="${id}"]`);
const rowsIn = (scope: string) => page.locator(`[data-testid="customize-memory-item"][data-scope^="${scope}:"]`);

async function runTask(goal: string): Promise<string> {
  const mark = bodies.length;
  if (sessionId === "") {
    await page.getByTestId("goal").fill(goal);
    await page.getByTestId("workspace").fill(repo);
    await page.getByTestId("run").click();
    const card = page.getByTestId("task-card").first();
    taskId = (await card.getAttribute("data-task-id"))!;
    sessionId = (await page.evaluate(() => window.modbit.localState()).then((s) => s.sessionId))!;
    await card.getByTestId("task-start").click();
  } else {
    n += 1;
    const commandId = n.toString(16).padStart(32, "0");
    taskId = await page.evaluate(([s, g, c, w]) => window.modbit.createTask(s!, g!, c!, w).then((t) => t.taskId), [sessionId, goal, commandId, repo]);
    await page.evaluate(([s, t]) => window.modbit.startTask(s!, t!), [sessionId, taskId] as const);
  }
  const t = taskId;
  await expect.poll(() => page.evaluate((x) => window.modbit.taskStatus(x).then((s) => s.loopAlive || s.state === "Queued" || s.state === "Created"), t), { timeout: 60_000 }).toBe(false);
  return bodies.slice(mark).join("\n");
}

async function openMemory(): Promise<void> {
  if ((await page.getByTestId("customize").count()) === 0) await page.getByTestId("open-customize").click();
  await expect(page.getByTestId("customize-memory")).toBeVisible();
  await page.getByTestId("customize-refresh").click();
}

const finish = (): Step => async (ctx) => {
  bodies.push(JSON.stringify(ctx.body.messages ?? []));
  return { calls: [{ name: "task.complete", args: { summary: "done", self_review: { findings: [] } } }] };
};

test.beforeAll(async () => {
  repo = realpathSync(mkdtempSync(join(tmpdir(), "modbit-px188-repo-")));
  writeFileSync(join(repo, "notes.md"), "notes\n");
  makeRepo(repo);
  dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-px188-"));
  const m = await steppedModel([finish(), finish(), finish(), finish()]);
  server = m.server;
  modelUrl = m.url;
  ({ app, page } = await launch(dataDir, { env: { MODBIT_OPENAI_BASE_URL: m.url } }));
  await setContentSize(app, page, 1280, 800);
});

test.afterAll(async () => {
  if (app) await closeApp(app);
  server?.close();
});

test("a private note is listed with its scope and source, and sharing stores the redacted copy", async () => {
  await runTask("Look at the notes");
  await openMemory();
  await page.getByTestId("customize-memory-topic").fill("release contact");
  await page.getByTestId("customize-memory-content").fill(PRIVATE);
  await page.getByTestId("customize-memory-remember").click();
  const mine = rowsIn("user");
  await expect(mine).toHaveCount(1);
  await expect(mine.getByTestId("customize-memory-where")).toContainText("user scope · from user stated");
  await expect(mine).toContainText("jane.doe@example.com");
  const privateId = (await mine.getAttribute("data-id"))!;
  // Share into the organization: a redacted proposal appears in that scope; the original is untouched.
  await page.getByTestId("customize-memory-share-scope").selectOption("organization");
  await row(privateId).getByTestId("customize-memory-share").click();
  const shared = rowsIn("organization");
  await expect(shared).toHaveCount(1);
  await expect(shared).not.toContainText("jane.doe");
  await expect(shared).not.toContainText("janedoe");
  await expect(shared).toContainText("[personal]");
  await expect(row(privateId)).toContainText("jane.doe@example.com");
});

test("only the redacted copy reaches the next real request, and deleting the original removes it everywhere", async () => {
  await openMemory();
  const sharedId = (await rowsIn("organization").getAttribute("data-id"))!;
  const privateId = (await rowsIn("user").getAttribute("data-id"))!;
  // Keep the shared proposal: the Core's governed promotion, from the view.
  await row(sharedId).getByTestId("customize-memory-promote").click();
  await expect(row(sharedId)).toHaveAttribute("data-status", "curated");
  const req = await runTask("Look at the notes again");
  expect(req).toContain("[personal]");
  expect(req).not.toContain("jane.doe@example.com");
  expect(req).not.toContain("/Users/janedoe");
  // Delete the original from the view.
  await openMemory();
  await row(privateId).getByTestId("customize-memory-delete").click();
  await expect(rowsIn("user")).toHaveCount(0);
  const after = await runTask("Look at the notes a third time");
  expect(after).not.toContain("jane.doe@example.com");
  // A relaunch on the same data directory lists the same.
  await closeApp(app);
  ({ app, page } = await launch(dataDir, { env: { MODBIT_OPENAI_BASE_URL: modelUrl } }));
  await setContentSize(app, page, 1280, 800);
  const t = taskId;
  const view = await page.evaluate((x) => window.modbit.customizeMemory(x), t);
  expect(view.items.map((i) => `${i.scope.split(":")[0]}|${i.status}`).sort()).toEqual(["organization|curated"]);
  expect(JSON.stringify(view)).not.toContain("jane.doe");
});
