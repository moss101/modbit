/**
 * REQ-PX-054..056 (QUAL-PX-054, -055, -056): the composer against a real Core,
 * a real event log, the preload bridge and the renderer. Only the model is
 * scripted (an OpenAI-compatible server whose streams a test holds open with a
 * Gate and which keeps every request it receives). Every wait is for a
 * condition the Core or the page reports; no test sleeps for a state.
 */
import { expect, test } from "@playwright/test";
import { existsSync, mkdtempSync, readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { accessible, appDir, closeApp, launch, makeRepo, MOD, setContentSize } from "./support/ui-harness.ts";
import { coreQueue, createTask, Gate, openConversation, sequencedModel, startAndOpen, typeAndPress, userMessages } from "./support/composer-harness.ts";

const FIRST = "The first answer begins here and keeps going, ";
const SECOND = "then it ends.";

test("PX-055: messages sent during a turn queue in the Core, can be edited, reordered and deleted, survive a reload, and drain in order as their own turns", async () => {
  test.setTimeout(240_000);
  const repo = makeRepo(mkdtempSync(join(tmpdir(), "modbit-cmp-repo-")));
  const gate = new Gate();
  // Each turn reads a file (progress), so the run keeps going through the queued messages; the first turn is held open by the gate.
  const read = { call: { name: "fs.read", args: { path: "notes.txt" } } };
  const model = await sequencedModel([{ parts: [FIRST, { wait: gate }, SECOND, read] }, ...[1, 2, 3, 4].map(() => ({ parts: ["Reading.", read] }))]);
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-cmp-queue-"));
  const { app, page } = await launch(dataDir, { env: { MODBIT_OPENAI_BASE_URL: model.url } });
  try {
    await setContentSize(app, page, 1280, 800);
    const taskId = await startAndOpen(page, "Queue some messages", repo);
    await expect(page.getByTestId("conv-assistant")).toHaveAttribute("data-message-state", "streaming", { timeout: 60_000 });
    // A turn is running: the composer offers Stop, and says what Enter does.
    await expect(page.getByTestId("composer-stop")).toBeVisible();
    await expect(page.getByTestId("composer-input")).toHaveAttribute("placeholder", /Enter queues it/);

    // The first send that would queue shows the education tray once, with truthful words.
    await typeAndPress(page, "second message", "Enter");
    await expect(page.getByTestId("queue-header")).toHaveText("1 queued");
    await expect(page.getByTestId("education-tray")).toBeVisible();
    await expect(page.getByTestId("education-tray")).toContainText("may cut the current model call");
    expect(await page.getByTestId("education-tray").innerText()).not.toMatch(/without stopping/i);
    await accessible(page, "the queue tray and the send-behaviour education tray");
    await page.getByTestId("education-keep").click();
    await expect(page.getByTestId("education-tray")).toHaveCount(0);

    await typeAndPress(page, "third message", "Enter");
    await typeAndPress(page, "fourth message", "Enter");
    await expect(page.getByTestId("queue-header")).toHaveText("3 queued");
    // The tray shows the Core's durable queue, in the Core's order.
    expect((await coreQueue(page, taskId)).map((q) => q.text)).toEqual(["second message", "third message", "fourth message"]);
    await expect(page.getByTestId("queue-line")).toHaveText(["second message", "third message", "fourth message"]);
    expect((await coreQueue(page, taskId)).every((q) => q.mode === "FOLLOW_UP")).toBe(true);

    // Edit in place (row 2), reorder (row 3 up), delete (row 1): each is a Core command and the tray follows the Core.
    await page.getByTestId("queue-row").nth(1).getByTestId("queue-edit").click();
    await page.getByTestId("queue-edit-input").fill("third message (edited)");
    await page.getByTestId("queue-edit-save").click();
    await expect(page.getByTestId("queue-line").nth(1)).toHaveText("third message (edited)");
    expect((await coreQueue(page, taskId))[1]).toMatchObject({ text: "third message (edited)", edited: true });
    await page.getByTestId("queue-row").nth(2).getByTestId("queue-up").click();
    await expect(page.getByTestId("queue-line")).toHaveText(["second message", "fourth message", "third message (edited)"]);
    await page.getByTestId("queue-row").nth(0).getByTestId("queue-delete").click();
    await expect(page.getByTestId("queue-header")).toHaveText("2 queued");
    expect((await coreQueue(page, taskId)).map((q) => q.text)).toEqual(["fourth message", "third message (edited)"]);

    // The queue is the Core's: a renderer reload shows the same two, and the acknowledged education does not come back.
    await page.reload();
    await expect(page.getByTestId("core-status")).toContainText("Core connected", { timeout: 60_000 });
    await openConversation(page, taskId);
    await expect(page.getByTestId("queue-line")).toHaveText(["fourth message", "third message (edited)"]);
    await typeAndPress(page, "fifth message", "Enter");
    await expect(page.getByTestId("queue-header")).toHaveText("3 queued");
    await expect(page.getByTestId("education-tray")).toHaveCount(0);

    // Keyboard only: arrows move between rows, Right edits, the primary modifier with Backspace deletes, Escape leaves.
    await page.getByTestId("queue-row-main").first().focus();
    await page.keyboard.press("ArrowDown");
    await expect(page.getByTestId("queue-row-main").nth(1)).toBeFocused();
    await page.keyboard.press("ArrowRight");
    await expect(page.getByTestId("queue-edit-input")).toBeFocused();
    await page.keyboard.press("Escape");
    await page.getByTestId("queue-row-main").nth(1).focus();
    await page.keyboard.press("Escape");
    await expect(page.getByTestId("composer-input")).toBeFocused();

    // The turn ends: the queued messages run in order, each as its own turn, and the deleted one never reaches the model.
    gate.open();
    await expect(page.getByTestId("queue-tray")).toHaveCount(0, { timeout: 90_000 });
    await expect.poll(() => model.requests(), { timeout: 90_000 }).toBeGreaterThanOrEqual(4);
    // Each message reaches the model as its own turn, in the order the Core's queue held them: the first request carrying each is later than the one before.
    const firstSeen = (needle: string) => model.bodies().findIndex((_, i) => model.text(i).includes(needle));
    await expect.poll(() => firstSeen("fifth message"), { timeout: 90_000 }).toBeGreaterThan(0);
    const [first, second, third] = [firstSeen("fourth message"), firstSeen("third message (edited)"), firstSeen("fifth message")];
    expect(first).toBeGreaterThan(0);
    expect(second).toBeGreaterThan(first);
    expect(third).toBeGreaterThan(second);
    for (let i = 0; i < model.requests(); i++) {
      expect(model.text(i)).not.toContain("second message");
      expect(model.text(i)).not.toContain('third message"');
    }
  } finally {
    gate.open();
    await closeApp(app);
    model.server.close();
  }
});
void [existsSync, readFileSync, writeFileSync, mkdirSync, resolve, appDir, MOD, createTask];
