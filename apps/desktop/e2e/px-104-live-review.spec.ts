/**
 * REQ-PX-104 / QUAL-PX-104, the live per-hunk review, on the real stack: the
 * packaged Electron app, a real Core over its socket, a real Git checkout and
 * the repository's scripted OpenAI-compatible model (the live gateway leg is
 * not run here) that holds the task Running at a gate the test opens.
 *
 * In order, on one app and one task: the Changes tab is open while the task is
 * Running and shows each hunk's originating call as a chip; the chip opens the
 * call in the conversation; one hunk is rejected with a reason by keyboard
 * while the run is live, the file changes by hash and the agent continues and
 * is told the reason; the task waits for the person (the Changes tab is still
 * there); a decision on a file that moved is refused with the Core's own code
 * and a refresh works; a write by something that is not a tool call is shown as
 * unattributed and is accepted only by the person; the renderer's process is
 * killed and the replacement view shows the same decisions from the Core.
 *
 * Every wait is for a condition the Core, the page or the file system reports.
 */
import { expect, test, type ElectronApplication, type Page } from "@playwright/test";
import { createHash } from "node:crypto";
import { mkdtempSync, readFileSync, realpathSync, writeFileSync } from "node:fs";
import type { Server } from "node:http";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { gate, steppedModel, type Step } from "./support/apps-harness.ts";
import { accessible, closeApp, git, launch, makeRepo, MOD, setContentSize } from "./support/ui-harness.ts";

test.describe.configure({ mode: "serial" });

const NOTICE = "Core notice: a person rejected one hunk";
const REASON = "keep row-02 as it was";
const ROWS = Array.from({ length: 45 }, (_, i) => `row-${String(i + 1).padStart(2, "0")}`).join("\n") + "\n";
const AFTER_AGENT = ROWS.replace("row-02\n", "row-02 (first)\n").replace("row-24\n", "row-24 (second)\n");
const AFTER_REJECT = AFTER_AGENT.replace("row-02 (first)\n", "row-02\n");
const sha = (s: string) => createHash("sha256").update(s).digest("hex");

let app: ElectronApplication;
let page: Page;
let server: Server;
let repo: string;
let dataDir: string;
let taskId = "";
const modelGate = gate();
const told: { notice: string | null; requestsWithNotice: number } = { notice: null, requestsWithNotice: 0 };
let atGate!: () => void;
const reachedGate = new Promise<void>((r) => (atGate = r));

const text = (p: string) => readFileSync(join(repo, p), "utf8");
const hunks = () => page.getByTestId("changes-hunk");
const hunk = (i: number) => page.locator(`[data-testid="changes-hunk"][data-index="${i}"]`);
async function openChanges(): Promise<void> {
  if ((await page.getByTestId("app-changes").count()) > 0) return;
  await page.getByTestId("task-card").first().focus();
  await page.keyboard.press(`${MOD}+e`);
  await expect(page.getByTestId("app-changes")).toBeVisible();
  const file = page.locator('[data-testid="changes-file"][data-path="notes.txt"]');
  if ((await file.count()) > 0) await file.click();
}

test.beforeAll(async () => {
  repo = realpathSync(mkdtempSync(join(tmpdir(), "modbit-px104-repo-")));
  makeRepo(repo);
  writeFileSync(join(repo, "notes.txt"), ROWS);
  writeFileSync(join(repo, "other.txt"), "other\n");
  git(repo, "add", "-A");
  git(repo, "-c", "user.name=t", "-c", "user.email=t@e", "commit", "-q", "-m", "rows");
  const plan = { calls: [{ name: "plan.update", args: { outcome: "edit the notes", expected_files: ["notes.txt"] } }] };
  const script: Step[] = [
    plan,
    { calls: [{ name: "fs.read", args: { path: "notes.txt" } }] },
    { calls: [{ name: "change.apply", args: { path: "notes.txt", op: "edit", text_edits: [{ old: "row-02\n", new: "row-02 (first)\n" }] } }] },
    { calls: [{ name: "change.apply", args: { path: "notes.txt", op: "edit", text_edits: [{ old: "row-24\n", new: "row-24 (second)\n" }] } }] },
    // Held here: the task is Running while the person decides.
    async () => {
      atGate();
      await modelGate.wait;
      return { calls: [{ name: "fs.read", args: { path: "other.txt" } }] };
    },
    // The next request is made after the Core's round boundary: it carries the notice.
    ({ body }) => {
      const all = JSON.stringify(body.messages ?? []);
      if (all.includes(NOTICE)) {
        told.requestsWithNotice += 1;
        told.notice = all;
        return { calls: [{ name: "user.ask", args: { question: "I will leave row-02 alone. Anything else?", options: [{ id: "no", label: "no, that is all" }], reason: "change_set" } }] };
      }
      return { calls: [{ name: "fs.read", args: { path: "other.txt" } }] };
    },
  ];
  const m = await steppedModel(script);
  server = m.server;
  dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-px104-"));
  ({ app, page } = await launch(dataDir, { env: { MODBIT_OPENAI_BASE_URL: m.url } }));
  await setContentSize(app, page, 1280, 800);
});

test.afterAll(async () => {
  modelGate.open();
  if (app) await closeApp(app);
  server?.close();
});

test("the Changes tab is open while the task is Running; each hunk's chip names the call the Core attributed it to", async () => {
  await page.getByTestId("goal").fill("Edit the notes");
  await page.getByTestId("workspace").fill(repo);
  await page.getByTestId("run").click();
  const card = page.getByTestId("task-card").first();
  taskId = (await card.getAttribute("data-task-id"))!;
  await card.getByTestId("task-start").click();
  await expect(card.getByTestId("task-state")).toHaveText("Running", { timeout: 60_000 });
  await reachedGate;
  await openChanges();
  await expect(page.getByTestId("app-changes")).toHaveAttribute("data-task-state", "Running");
  await expect(hunks()).toHaveCount(2);
  for (const i of [0, 1]) {
    await expect(hunk(i).getByTestId("changes-hunk-chip")).toHaveText("Written by change.apply");
    await expect(hunk(i)).toHaveAttribute("data-attributed", "true");
    await expect(hunk(i).getByTestId("changes-hunk-unattributed")).toHaveCount(0);
    // Controls the Core would refuse are not enabled: here it would accept, so they are.
    await expect(hunk(i).getByTestId("changes-accept-hunk")).toBeEnabled();
    await expect(hunk(i).getByTestId("changes-reject-hunk")).toBeEnabled();
  }
  await expect(page.getByTestId("changes-note")).toContainText("accept or reject a single hunk now");
  await accessible(page, "the Changes tab on a running task, per-hunk controls shown", '[data-testid="region-apps"]');
});

test("the chip opens the producing call in the conversation", async () => {
  const chip = hunk(0).getByTestId("changes-hunk-chip");
  const rowId = (await chip.getAttribute("data-row-id"))!;
  expect(rowId).toMatch(/^tool:[0-9a-f-]{32,36}$/);
  await chip.click();
  const row = page.locator(`[data-testid="conv-tool"][data-row-id="${rowId}"]`);
  await expect(row).toBeVisible();
  await expect(row).toHaveAttribute("data-highlight", "true");
  await openChanges();
});

test("rejecting one hunk by keyboard while the run is live reverts exactly that hunk, and the agent continues and is told why", async () => {
  expect(text("notes.txt")).toBe(AFTER_AGENT);
  await page.getByTestId("changes-reject-reason").fill(REASON);
  await hunk(0).getByTestId("changes-reject-hunk").focus();
  await page.keyboard.press("Enter");
  const banner = page.getByTestId("changes-decision");
  await expect(banner).toHaveAttribute("data-result", "REJECTED");
  await expect(banner).toHaveAttribute("data-run-live", "true");
  await expect(page.getByTestId("changes-decision-notice")).toContainText(REASON);
  expect(text("notes.txt"), "exactly that hunk went back, by hash").toBe(AFTER_REJECT);
  expect(sha(text("notes.txt"))).toBe(sha(AFTER_REJECT));
  await expect(hunks()).toHaveCount(1);
  await expect(hunk(0).getByTestId("changes-hunk-chip")).toHaveText("Written by change.apply");
  // The same run goes on: released, it asks for the notice at its next boundary and answers it.
  modelGate.open();
  await expect.poll(() => told.requestsWithNotice, { timeout: 90_000, message: "the agent never received the notice" }).toBeGreaterThan(0);
  expect(told.notice).toContain(REASON);
  expect(told.notice!.split(NOTICE).length - 1).toBe(1);
  await expect(page.getByTestId("app-changes")).not.toHaveAttribute("data-task-state", "ReadyForReview");
  expect(text("notes.txt")).toBe(AFTER_REJECT);
});

test("with the task waiting for the person, a decision on a file that moved is refused with the Core's code, and a refresh works", async () => {
  const state = await page.getByTestId("app-changes").getAttribute("data-task-state");
  expect(["Running", "Waiting", "NeedsInput", "NeedsAttention"]).toContain(state);
  // Something that is not a tool call writes the file behind the view.
  const moved = AFTER_REJECT.replace("row-33\n", "row-33 (stranger)\n");
  writeFileSync(join(repo, "notes.txt"), moved);
  await hunk(0).getByTestId("changes-reject-hunk").click();
  const refusal = page.getByTestId("changes-refusal");
  await expect(refusal).toHaveAttribute("data-code", "STALE_REVISION");
  await expect(refusal).toContainText("Nothing was written");
  expect(text("notes.txt"), "the stale decision wrote nothing").toBe(moved);
  await page.getByTestId("changes-refusal-reload").click();
  await expect(refusal).toHaveCount(0);
  await expect(hunks()).toHaveCount(2);
});

test("a write by no tool call is shown unattributed, blocks automatic acceptance, and only the person accepts it", async () => {
  const stranger = page.locator('[data-testid="changes-hunk"][data-attributed="false"]');
  await expect(stranger).toHaveCount(1);
  await expect(stranger.getByTestId("changes-hunk-chip")).toHaveText("Not attributed to a tool call");
  await expect(stranger.getByTestId("changes-hunk-chip")).toBeDisabled();
  await expect(stranger.getByTestId("changes-hunk-unattributed")).toContainText("never accepted automatically");
  await stranger.getByTestId("changes-accept-hunk").click();
  await expect(page.getByTestId("changes-decision")).toHaveAttribute("data-result", "ACCEPTED");
  await expect(stranger).toHaveAttribute("data-decision", "ACCEPTED");
  expect(text("notes.txt")).toContain("row-33 (stranger)");
  await accessible(page, "the Changes tab with an accepted unattributed hunk", '[data-testid="region-apps"]');
});

test("the renderer's process is killed and the replacement view shows the same decisions, read from the Core", async () => {
  const before = await page.getByTestId("changes-hunk").evaluateAll((els) => els.map((e) => [e.getAttribute("data-digest"), e.getAttribute("data-decision"), e.getAttribute("data-attributed")]));
  expect(before.length).toBe(2);
  const old = await app.evaluate(({ BrowserWindow }) => BrowserWindow.getAllWindows()[0]!.webContents.id);
  if (process.platform === "win32") process.kill(await app.evaluate(({ BrowserWindow }) => BrowserWindow.getAllWindows()[0]!.webContents.getOSProcessId()));
  else await app.evaluate(({ BrowserWindow }) => BrowserWindow.getAllWindows()[0]!.webContents.forcefullyCrashRenderer());
  await expect
    .poll(() => app.evaluate(({ BrowserWindow }, id) => BrowserWindow.getAllWindows().filter((w) => !w.isDestroyed() && w.webContents.id !== id).length, old), { timeout: 60_000, message: "main never opened a replacement window" })
    .toBe(1);
  const oldPage = page;
  await expect.poll(() => app.windows().filter((p) => p !== oldPage && !p.isClosed()).length, { timeout: 60_000 }).toBe(1);
  page = app.windows().filter((p) => p !== oldPage && !p.isClosed())[0]!;
  await expect(page.getByTestId("core-status")).toContainText("Core connected", { timeout: 60_000 });
  await setContentSize(app, page, 1280, 800);
  await openChanges();
  await expect(hunks()).toHaveCount(2);
  const after = await page.getByTestId("changes-hunk").evaluateAll((els) => els.map((e) => [e.getAttribute("data-digest"), e.getAttribute("data-decision"), e.getAttribute("data-attributed")]));
  expect(after).toEqual(before);
  expect(git(repo, "status", "--short")).toContain("notes.txt");
});
