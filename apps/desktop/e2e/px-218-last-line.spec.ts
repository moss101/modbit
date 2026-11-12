/**
 * PX-E2E-218 (DR-PX-2026-11-11-021 MWS-20), the desktop's half: on the real Electron build with a real Core and a scripted
 * model whose answer ends with a known line and holds a credential-shaped secret, a finished turn puts the assistant's last
 * line on the task's header and on the row's preview card, redacted as the transcript is; a task that has not answered has
 * none; and after the app and its Core are stopped and started again on the same data, the header and the card say the same.
 * The card makes no read of its own: it shows the header the list already holds. With MODBIT_SHOTS_DIR set the run writes
 * the evidence screenshots.
 */
import { expect, test, type Page } from "@playwright/test";
import { mkdirSync, mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { Gate, streamingModel } from "./support/stream-model.ts";
import { agentRow, queueTask, runTask, toNewTask } from "./support/new-task.ts";
import { accessible, closeApp, launch, makeRepo, setContentSize } from "./support/ui-harness.ts";

const SHOTS = process.env.MODBIT_SHOTS_DIR ?? "";
const SECRET = "ghp_abcdefghijklmnopqrstuvwxyz0123";
const ANSWER = `I read notes.txt.\n\nIt has three lines.\nThe token ${SECRET} is in the last one.\n\n   \n`;
const LINE = "The token [redacted] is in the last one.";

const headerOf = (page: Page, taskId: string) => page.evaluate(async (t) => {
  const s = await window.modbit.localState();
  return (await window.modbit.agentHeaders(s.sessionId!, false)).headers.find((h) => h.taskId === t) ?? null;
}, taskId);

/**
 * Opens a row's preview card from the keyboard (home is open, so no conversation reads anything) and returns it. The card
 * closes when focus leaves it, and the list moves focus to a row once after a task is created or the app starts; the card is
 * opened again if that happened under it, so what is read is a card that stays.
 */
async function previewOf(page: Page, taskId: string) {
  await toNewTask(page);
  const card = page.getByTestId("agent-preview");
  await expect(async () => {
    if (!(await card.isVisible())) {
      await agentRow(page, taskId).getByTestId("agent-open").focus();
      await page.keyboard.press("Space");
    }
    await expect(card).toHaveAttribute("data-task-id", taskId, { timeout: 2_000 });
    await page.waitForTimeout(700);
    await expect(card).toHaveAttribute("data-task-id", taskId, { timeout: 1_000 });
  }).toPass({ timeout: 30_000 });
  return card;
}

async function shot(page: Page, name: string): Promise<void> {
  if (!SHOTS) return;
  mkdirSync(SHOTS, { recursive: true });
  for (const theme of ["light", "dark"] as const) {
    await page.evaluate((t) => document.documentElement.setAttribute("data-theme", t), theme);
    await page.waitForTimeout(400);
    await page.screenshot({ path: join(SHOTS, `${name}-${theme}.png`) });
  }
  await page.evaluate(() => document.documentElement.setAttribute("data-theme", "light"));
}

test("PX-218: after a finished turn the header and the preview card carry the assistant's last line, redacted; a task that has not answered carries none; both say the same after a restart", async () => {
  test.setTimeout(240_000);
  const repo = makeRepo(mkdtempSync(join(tmpdir(), "modbit-px218-repo-")));
  const hold = new Gate();
  const model = await streamingModel([{ parts: ["I read notes.txt.\n\n", { wait: hold }, ANSWER.slice("I read notes.txt.\n\n".length)] }]);
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-px218-"));
  let { app, page } = await launch(dataDir, { env: { MODBIT_OPENAI_BASE_URL: model.url } });
  try {
    await setContentSize(app, page, 1440, 900);
    const taskId = await runTask(page, "Say what notes.txt holds", repo);
    // While the message is unfinished (the model is held mid-answer) the header has no line: nothing of a half-said message is shown.
    await expect(page.getByTestId("conv-assistant").first()).toContainText("I read notes.txt.", { timeout: 60_000 });
    expect((await headerOf(page, taskId))!.lastLine ?? "", "no line from an unfinished message").toBe("");
    hold.open();
    await expect(page.locator('[data-testid="conv-footer"][data-outcome="COMPLETED"]').first()).toBeAttached({ timeout: 90_000 });

    // The header's line is the end of the assistant's message, and the secret is redacted there as in the transcript.
    await expect.poll(async () => (await headerOf(page, taskId))!.lastLine, { timeout: 30_000 }).toBe(LINE);
    const said = await page.evaluate((t) => window.modbit.transcript(t, { density: "DETAILED" }).then((p) => p.rows.filter((r) => r.kind === "ASSISTANT_MESSAGE").map((r) => r.text).join("\n")), taskId);
    expect(said.trimEnd().split("\n").pop(), "the header's line is the transcript's last line").toBe(LINE);
    expect(said.includes(SECRET), "the transcript does not hold the secret").toBe(false);
    await expect(page.getByTestId("conversation")).not.toContainText(SECRET);

    // A second task that never ran: no line, and its card draws none.
    const idle = await queueTask(page, "A task that has not answered", repo);
    expect((await headerOf(page, idle))!.lastLine ?? "").toBe("");

    // The card: the last line under the state, exactly the header's, and nothing of the secret anywhere in the window.
    let card = await previewOf(page, taskId);
    await expect(card.getByTestId("agent-preview-last")).toHaveText(`The assistant's last line: ${LINE}`);
    const state = (await card.getByTestId("agent-preview-state").boundingBox())!;
    const last = (await card.getByTestId("agent-preview-last").boundingBox())!;
    expect(last.y, "the line is under the state").toBeGreaterThan(state.y + state.height - 1);
    expect(await page.evaluate((s) => document.body.innerText.includes(s) || document.body.innerHTML.includes(s), SECRET), "the secret is nowhere in the page").toBe(false);
    await accessible(page, "the preview card with the last line", '[data-testid="agent-preview"]');
    await shot(page, "preview-card-last-line");
    await page.keyboard.press("Escape");
    card = await previewOf(page, idle);
    await expect(card.getByTestId("agent-preview-last"), "a task that has not answered shows no line").toHaveCount(0);
    await page.keyboard.press("Escape");
    const before = await headerOf(page, taskId);

    // Restart: the app and its Core stop and start again on the same data. The header and the card read the same.
    await closeApp(app);
    ({ app, page } = await launch(dataDir, { env: { MODBIT_OPENAI_BASE_URL: model.url } }));
    await setContentSize(app, page, 1440, 900);
    await expect(agentRow(page, taskId)).toBeVisible({ timeout: 60_000 });
    const after = await headerOf(page, taskId);
    expect(after!.lastLine, "the same line after the restart").toBe(LINE);
    expect(after!.lastLine).toBe(before!.lastLine);
    expect((await headerOf(page, idle))!.lastLine ?? "").toBe("");
    card = await previewOf(page, taskId);
    await expect(card.getByTestId("agent-preview-last")).toHaveText(`The assistant's last line: ${LINE}`);
    expect(await page.evaluate((s) => document.body.innerHTML.includes(s), SECRET)).toBe(false);
    await shot(page, "preview-card-last-line-after-restart");
    await page.keyboard.press("Escape");
    card = await previewOf(page, idle);
    await expect(card.getByTestId("agent-preview-last")).toHaveCount(0);
  } finally {
    await closeApp(app);
    model.server.close();
  }
});
