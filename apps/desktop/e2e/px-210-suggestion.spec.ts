/**
 * REQ-PX-210 (QUAL-PX-210 / PX-E2E-210, DR-PX-2026-11-11-021 MWS-16), with the
 * bridge leg of REQ-PX-218: the next-prompt suggestion against a real Core, its
 * real event log, the preload bridge and the renderer. Only the model is
 * scripted: an OpenAI-compatible server that keeps every request it receives,
 * so a spec counts the calls that really reached the model.
 *
 * After a turn the composer shows the Core's suggestion as placeholder text;
 * Tab puts it in the box and nothing is sent; typing dismisses it; a turn that
 * ends with a message queued gets none; with the person's setting off the
 * model receives no extra request, and the setting survives a reload; with the
 * Core's own switch off (the rest of the suite) the model receives none either.
 */
import { expect, test, type Page } from "@playwright/test";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { accessible, closeApp, launch, makeRepo, setContentSize } from "./support/ui-harness.ts";
import { coreQueue, Gate, openConversation, sequencedModel, startAndOpen, typeAndPress, userMessages, type Part, type RequestBody, type SeqModel } from "./support/composer-harness.ts";
import { sessionOf } from "./support/conversation-harness.ts";

/** How the Core's suggestion request opens: what tells it from a turn's request. */
const MARKER = "[NEXT PROMPT SUGGESTION]";
const ANSWER = "I looked at notes.txt.\nIt holds three lines.";
const ON = { MODBIT_NEXT_PROMPT_SUGGESTIONS: "on" };

const textOf = (b: RequestBody): string => JSON.stringify(b.messages ?? []);
const isSuggestion = (b: RequestBody): boolean => textOf(b).includes(MARKER);
const suggestionRequests = (m: SeqModel): number => m.bodies().filter(isSuggestion).length;
/** Whether a turn's request (never a suggestion's) carrying `needle` has reached the model. */
const turnSaw = (m: SeqModel, needle: string): boolean => m.bodies().some((b) => !isSuggestion(b) && textOf(b).includes(needle));
/** The requests in order, by kind, with runs of the same kind folded into one: "turn" is one or more turn requests in a row. */
const order = (m: SeqModel): string[] => m.bodies().map((b) => (isSuggestion(b) ? "suggestion" : "turn")).filter((k, i, a) => k !== a[i - 1]);

/**
 * The stand-in: a suggestion request is answered with the n-th suggestion (wrapped the way a model wraps one), a turn's
 * request with a text-only answer. A run asks the model a few times before it accepts that an answer with no tool call
 * is the answer, so a turn is one or more requests; a spec counts suggestions exactly and never counts a turn's requests.
 * `turn` may hold a turn's answer back or change it, by what the request carries.
 */
function suggestingModel(suggestions: string[], turn: (body: RequestBody) => Part[] = () => [ANSWER]): Promise<SeqModel> {
  let asked = 0;
  const reply = ({ body }: { body: RequestBody }) => (isSuggestion(body) ? { parts: [`"${suggestions[Math.min(asked++, suggestions.length - 1)]}"\nBecause it is the obvious next step.`] } : { parts: turn(body) });
  return sequencedModel(Array.from({ length: 96 }, () => reply));
}

const input = (page: Page) => page.getByTestId("composer-input");
const hint = (page: Page) => page.getByTestId("composer-suggestion-hint");

/** The composer shows `text` as its suggestion: the placeholder, the marker and the hint that names the key. */
async function expectSuggestion(page: Page, text: string): Promise<void> {
  await expect(input(page)).toHaveAttribute("placeholder", text, { timeout: 60_000 });
  await expect(input(page)).toHaveAttribute("data-suggestion", "true");
  await expect(hint(page)).toContainText("Tab");
}

/** The composer shows no suggestion: its own placeholder, no marker, no hint. */
async function expectNoSuggestion(page: Page, placeholder: RegExp = /Follow up on this task/): Promise<void> {
  await expect(input(page)).toHaveAttribute("placeholder", placeholder, { timeout: 30_000 });
  await expect(input(page)).not.toHaveAttribute("data-suggestion", "true");
  await expect(hint(page)).toHaveCount(0);
}

/** The turn has ended as far as the composer knows: no Stop, and the Core's own answer says no run is alive. */
async function expectTurnEnded(page: Page, taskId: string): Promise<void> {
  await expect.poll(() => page.evaluate((t) => window.modbit.composerQueue(t).then((q) => q.runAlive), taskId), { timeout: 90_000 }).toBe(false);
  await expect(page.getByTestId("composer-region")).toHaveAttribute("data-running", "false", { timeout: 30_000 });
}

test("PX-210: after a turn the Core's suggestion is the composer's placeholder; Tab puts it in the box and nothing is sent; typing dismisses it; a turn that ends with a message queued gets none", async () => {
  test.setTimeout(300_000);
  const repo = makeRepo(mkdtempSync(join(tmpdir(), "modbit-sug-repo-")));
  const gate = new Gate();
  // The turn of "my own follow-up" is held open so a message can be queued behind it.
  const model = await suggestingModel(["Add a test for the empty file case", "Now run the whole suite", "Commit what changed"], (b) => (textOf(b).includes("the queued one") ? ["Handled the queued one."] : textOf(b).includes("my own follow-up") ? ["Working on it, ", { wait: gate }, "and done."] : [ANSWER]));
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-sug-"));
  const { app, page } = await launch(dataDir, { env: { MODBIT_OPENAI_BASE_URL: model.url, ...ON } });
  try {
    await setContentSize(app, page, 1280, 800);
    const taskId = await startAndOpen(page, "Say what notes.txt holds", repo);
    await expectTurnEnded(page, taskId);

    // The suggestion is the placeholder: ghost text, not a message. One request made it, with no tools.
    await expectSuggestion(page, "Add a test for the empty file case");
    await expect(input(page)).toHaveValue("");
    expect(order(model)).toEqual(["turn", "suggestion"]);
    expect(suggestionRequests(model)).toBe(1);
    const asked = model.bodies().find(isSuggestion)!;
    expect((asked as { tools?: unknown[] }).tools ?? []).toEqual([]);
    expect(JSON.stringify(asked.messages)).toContain("Task goal: Say what notes.txt holds");
    // Nothing was sent or queued for the person, and the conversation holds no trace of the suggestion.
    expect(await userMessages(page, taskId)).toEqual(["Say what notes.txt holds"]);
    expect(await coreQueue(page, taskId)).toEqual([]);
    expect(await page.evaluate((t) => window.modbit.transcript(t, { density: "DETAILED" }).then((p) => JSON.stringify(p.rows)), taskId)).not.toContain("empty file case");
    await accessible(page, "the composer showing a suggestion");
    // The call is the task's: one of its model calls, counted as a suggestion call.
    const econ = await page.evaluate((t) => window.modbit.taskEconomics(t), taskId);
    expect(econ.suggestionCalls).toBe(1);
    expect(econ.modelCalls).toBe(model.requests());
    // PX-218 over the same bridge: the task's header carries the last line the assistant wrote.
    const sessionId = await sessionOf(page);
    const header = (await page.evaluate((s) => window.modbit.agentHeaders(s, true), sessionId)).headers.find((h) => h.taskId === taskId)!;
    expect(header.lastLine).toBe("It holds three lines.");

    // Tab puts the suggestion in the box. Nothing is sent: no message, nothing queued, no request.
    const before = model.requests();
    await input(page).focus();
    await page.keyboard.press("Tab");
    await expect(input(page)).toHaveValue("Add a test for the empty file case");
    await expect(input(page)).toBeFocused();
    await expectNoSuggestion(page);
    await expect(page.getByTestId("composer-live")).toContainText("Nothing was sent");
    expect(await userMessages(page, taskId)).toEqual(["Say what notes.txt holds"]);
    expect(await coreQueue(page, taskId)).toEqual([]);
    await expect(page.getByTestId("composer-region")).toHaveAttribute("data-running", "false");
    // Long enough for a message that was going to be sent to have reached the model.
    await page.waitForTimeout(1_500);
    expect(model.requests()).toBe(before);
    await expect(input(page)).toHaveValue("Add a test for the empty file case");

    // The person sends it, in their own words: it is an ordinary message, and the turn it starts ends with a new suggestion.
    await input(page).fill("Add a test for the empty file case, please");
    await input(page).press("Enter");
    await expect.poll(() => turnSaw(model, "empty file case, please"), { timeout: 60_000 }).toBe(true);
    await expectTurnEnded(page, taskId);
    await expectSuggestion(page, "Now run the whole suite");
    expect(await userMessages(page, taskId)).toEqual(["Say what notes.txt holds", "Add a test for the empty file case, please"]);
    expect(suggestionRequests(model)).toBe(2);

    // Typing dismisses it, and it does not come back when the box is emptied again. No request is made for either.
    const quiet = model.requests();
    await input(page).pressSequentially("my");
    await expectNoSuggestion(page);
    await input(page).fill("");
    await expectNoSuggestion(page);
    expect(model.requests()).toBe(quiet);
    expect(order(model)).toEqual(["turn", "suggestion", "turn", "suggestion"]);

    // A turn that ends with a message queued gets no suggestion: the queued message is the next one. The suggestion comes after the last turn.
    await typeAndPress(page, "my own follow-up", "Enter");
    await expect(page.getByTestId("composer-stop")).toBeVisible({ timeout: 60_000 });
    await typeAndPress(page, "the queued one", "Enter");
    await expect(page.getByTestId("queue-header")).toHaveText("1 queued");
    if (await page.getByTestId("education-keep").isVisible()) await page.getByTestId("education-keep").click();
    await expectNoSuggestion(page, /Enter queues it/);
    gate.open();
    await expectTurnEnded(page, taskId);
    await expectSuggestion(page, "Commit what changed");
    // Every request in order: the held turn and the queued message's turn ran back to back, and only then was a suggestion asked for.
    expect(order(model)).toEqual(["turn", "suggestion", "turn", "suggestion", "turn", "suggestion"]);
    expect(suggestionRequests(model)).toBe(3);
    const kinds = model.bodies().map((b) => (isSuggestion(b) ? "suggestion" : "turn"));
    const ownAt = model.bodies().findIndex((b) => !isSuggestion(b) && textOf(b).includes("my own follow-up"));
    const queuedAt = model.bodies().findLastIndex((b) => !isSuggestion(b) && textOf(b).includes("the queued one"));
    expect(ownAt).toBeGreaterThan(0);
    expect(queuedAt).toBeGreaterThan(ownAt);
    expect(kinds.slice(ownAt, queuedAt + 1)).not.toContain("suggestion");
    expect(await userMessages(page, taskId)).toEqual(["Say what notes.txt holds", "Add a test for the empty file case, please", "my own follow-up", "the queued one"]);
    await accessible(page, "the conversation after the queued turn, with a suggestion");
  } finally {
    gate.open();
    await closeApp(app);
    model.server.close();
  }
});

test("PX-210: with the setting off the model receives no extra request, the setting survives a reload, and turning it on again brings the suggestion back", async () => {
  test.setTimeout(300_000);
  const repo = makeRepo(mkdtempSync(join(tmpdir(), "modbit-sug-repo-")));
  const model = await suggestingModel(["Add a test for the empty file case", "Now run the whole suite"]);
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-sug-off-"));
  const { app, page } = await launch(dataDir, { env: { MODBIT_OPENAI_BASE_URL: model.url, ...ON } });
  const toggle = async (want: boolean) => {
    await page.getByTestId("add-context").click();
    const item = page.getByTestId("menu-item-suggest-next");
    await expect(item).toHaveAttribute("aria-checked", String(!want));
    await item.click();
    await page.getByTestId("add-context").click();
    await expect(page.getByTestId("menu-item-suggest-next")).toHaveAttribute("aria-checked", String(want));
    await page.keyboard.press("Escape");
  };
  /** A follow-up turn that runs and ends; returns once the model has had the message and the composer has seen the turn end. */
  const followUp = async (taskId: string, text: string) => {
    await typeAndPress(page, text, "Enter");
    await expect.poll(() => turnSaw(model, text), { timeout: 60_000 }).toBe(true);
    await expectTurnEnded(page, taskId);
  };
  /** After long enough for a request that was going to be made to have been made (the composer asks within a second of a turn's end): the model's requests, by kind. */
  const settledOrder = async () => {
    await page.waitForTimeout(2_000);
    return order(model);
  };
  try {
    await setContentSize(app, page, 1280, 800);
    const taskId = await startAndOpen(page, "Say what notes.txt holds", repo);
    await expectTurnEnded(page, taskId);
    // On by default: the first turn brings a suggestion.
    await expectSuggestion(page, "Add a test for the empty file case");
    expect(suggestionRequests(model)).toBe(1);

    // Off: what is showing is withdrawn at once, and the next turns bring none. The model sees the turns and nothing else.
    await toggle(false);
    await expectNoSuggestion(page);
    await followUp(taskId, "second message");
    await followUp(taskId, "third message");
    await expectNoSuggestion(page);
    expect(await settledOrder()).toEqual(["turn", "suggestion", "turn"]);
    expect(suggestionRequests(model)).toBe(1);
    expect((await page.evaluate((t) => window.modbit.taskEconomics(t), taskId)).suggestionCalls).toBe(1);

    // The setting is the person's and is kept with the composer's preferences: a reload does not turn it back on.
    await page.reload();
    await expect(page.getByTestId("core-status")).toContainText("Core connected", { timeout: 60_000 });
    await openConversation(page, taskId);
    await followUp(taskId, "fourth message");
    await expectNoSuggestion(page);
    expect(await settledOrder()).toEqual(["turn", "suggestion", "turn"]);
    expect(suggestionRequests(model)).toBe(1);

    // On again: the next turn's end brings a suggestion, and it is exactly the second one ever asked for.
    await toggle(true);
    await followUp(taskId, "fifth message");
    await expectSuggestion(page, "Now run the whole suite");
    expect(order(model)).toEqual(["turn", "suggestion", "turn", "suggestion"]);
    expect(suggestionRequests(model)).toBe(2);
    await accessible(page, "the composer's menu setting and a suggestion");
  } finally {
    await closeApp(app);
    model.server.close();
  }
});

test("PX-210: with the Core's switch off (as the rest of this suite runs) a finished turn makes no extra request and the composer shows no suggestion", async () => {
  test.setTimeout(180_000);
  const repo = makeRepo(mkdtempSync(join(tmpdir(), "modbit-sug-repo-")));
  const model = await suggestingModel(["never asked for"]);
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-sug-core-off-"));
  const { app, page } = await launch(dataDir, { env: { MODBIT_OPENAI_BASE_URL: model.url, MODBIT_NEXT_PROMPT_SUGGESTIONS: "off" } });
  try {
    const taskId = await startAndOpen(page, "Say what notes.txt holds", repo);
    await expectTurnEnded(page, taskId);
    // The composer asked (the person's setting is on) and the Core refused with its typed code, calling no model.
    const answer = await page.evaluate(async (t) => window.modbit.composerSuggest((await window.modbit.localState()).sessionId!, t), taskId);
    expect(answer).toMatchObject({ text: "", refusal: "SUGGESTIONS_OFF" });
    await expectNoSuggestion(page);
    await page.waitForTimeout(1_500);
    expect(order(model)).toEqual(["turn"]);
    expect(suggestionRequests(model)).toBe(0);
    expect((await page.evaluate((t) => window.modbit.taskEconomics(t), taskId)).suggestionCalls).toBe(0);
  } finally {
    await closeApp(app);
    model.server.close();
  }
});
