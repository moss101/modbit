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
import { accessible, appDir, box, closeApp, launch, makeRepo, MOD, setContentSize } from "./support/ui-harness.ts";
import { coreQueue, createTask, Gate, openConversation, sequencedModel, startAndOpen, typeAndPress, userMessages } from "./support/composer-harness.ts";
import { ECHO_LOOP, nodeTerminal } from "./support/apps-harness.ts";

/** Lets the frame that follows a state change render, so a transition it starts is one `accessible` waits for (a colour mid-transition is not a colour). */
const settled = (page: import("@playwright/test").Page) => page.evaluate(() => new Promise<void>((r) => requestAnimationFrame(() => requestAnimationFrame(() => r()))));

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
    await expect(page.getByTestId("queue-edit-mode")).toHaveValue("FOLLOW_UP");
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

/** The abort source the Core projected for each assistant message of the task, oldest first. */
const abortSources = (page: import("@playwright/test").Page, taskId: string) =>
  page.evaluate(
    (t) =>
      window.modbit.transcript(t, { density: "DETAILED" }).then((p) => {
        const out: { phase: string; abortSource: string; text: string }[] = [];
        const walk = (rows: typeof p.rows) => {
          for (const r of rows) {
            if (r.kind === "ASSISTANT_MESSAGE" && r.facts.type === "stream") out.push({ phase: r.facts.phase, abortSource: r.facts.abortSource, text: r.text });
            walk(r.children);
          }
        };
        walk(p.rows);
        return out;
      }),
    taskId,
  );

test("PX-055: Send now cuts the live stream as a user interrupt, keeps the partial text marked partial, and the new turn starts from the sent message", async () => {
  test.setTimeout(240_000);
  const repo = makeRepo(mkdtempSync(join(tmpdir(), "modbit-cmp-repo-")));
  const gate = new Gate();
  const model = await sequencedModel([{ parts: [FIRST, { wait: gate }, SECOND] }, { parts: ["Handled the urgent message."] }]);
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-cmp-sendnow-"));
  const { app, page } = await launch(dataDir, { env: { MODBIT_OPENAI_BASE_URL: model.url } });
  try {
    await setContentSize(app, page, 1280, 800);
    const taskId = await startAndOpen(page, "Send now test", repo);
    await expect(page.getByTestId("conv-assistant")).toHaveAttribute("data-message-state", "streaming", { timeout: 60_000 });
    await typeAndPress(page, "urgent: use tabs", "Enter");
    await expect(page.getByTestId("queue-header")).toHaveText("1 queued");
    // The label states the consequence; it never claims to leave the agent running.
    const sendNow = page.getByTestId("queue-send-now");
    await expect(sendNow).toContainText("interrupts");
    await sendNow.click();
    // The live stream ends as aborted by the user's interrupt, its partial text kept; the new turn starts from the message.
    await expect.poll(async () => (await abortSources(page, taskId))[0]?.abortSource, { timeout: 60_000 }).toBe("USER_INTERRUPT");
    const first = (await abortSources(page, taskId))[0]!;
    expect(first.phase).toBe("ABORTED");
    await expect(page.getByTestId("conv-assistant").first()).toHaveAttribute("data-message-state", "aborted");
    await expect(page.getByTestId("conv-aborted").first()).toContainText("not a finished answer");
    await expect(page.getByTestId("conv-aborted").first()).not.toContainText("restarted");
    await expect(page.getByTestId("conv-text").first()).toContainText("The first answer begins here");
    await expect.poll(() => model.requests(), { timeout: 60_000 }).toBeGreaterThanOrEqual(2);
    expect(model.text(1)).toContain("urgent: use tabs");
    await expect(page.getByTestId("queue-tray")).toHaveCount(0, { timeout: 60_000 });
    await expect(page.getByTestId("conv-text").filter({ hasText: "Handled the urgent message." })).toBeVisible({ timeout: 60_000 });
  } finally {
    gate.open();
    await closeApp(app);
    model.server.close();
  }
});

test("PX-055: Stop ends the turn as a user interrupt, leaves the message editable and re-sends nothing; Continue resumes only when the person says so", async () => {
  test.setTimeout(240_000);
  const repo = makeRepo(mkdtempSync(join(tmpdir(), "modbit-cmp-repo-")));
  const gate = new Gate();
  const model = await sequencedModel([{ parts: [FIRST, { wait: gate }, SECOND] }, { parts: ["Continued after the stop."] }]);
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-cmp-stop-"));
  const { app, page } = await launch(dataDir, { env: { MODBIT_OPENAI_BASE_URL: model.url } });
  try {
    await setContentSize(app, page, 1280, 800);
    const taskId = await startAndOpen(page, "Stop test goal", repo);
    await expect(page.getByTestId("conv-assistant")).toHaveAttribute("data-message-state", "streaming", { timeout: 60_000 });
    await page.getByTestId("composer-stop").click();
    await expect.poll(async () => (await abortSources(page, taskId))[0]?.abortSource, { timeout: 30_000 }).toBe("USER_INTERRUPT");
    // The stopped state: the interrupted message is offered back, nothing was sent again, and there is a way to continue.
    const tray = page.getByTestId("stopped-tray");
    await expect(tray).toBeVisible();
    await expect(page.getByTestId("stopped-text")).toContainText("Stop test goal");
    await expect(page.getByTestId("composer-stop")).toHaveCount(0, { timeout: 30_000 });
    await accessible(page, "the stopped state");
    expect(model.requests()).toBe(1);
    await page.getByTestId("stopped-edit").click();
    await expect(page.getByTestId("composer-input")).toHaveValue("Stop test goal");
    expect(model.requests()).toBe(1);
    expect(await userMessages(page, taskId)).toEqual(["Stop test goal"]);
    // Nothing is re-sent until the person acts; Continue resumes the run.
    await page.getByTestId("composer-input").fill("");
    await page.getByTestId("conv-start").click();
    await expect.poll(() => model.requests(), { timeout: 60_000 }).toBeGreaterThanOrEqual(2);
  } finally {
    gate.open();
    await closeApp(app);
    model.server.close();
  }
});

test("PX-054: Shift+Tab walks the modes with the right chip and placeholder, the Core's mode field is authoritative, and a write in Plan is refused at the kernel", async () => {
  test.setTimeout(240_000);
  const repo = makeRepo(mkdtempSync(join(tmpdir(), "modbit-cmp-repo-")));
  const plan = { call: { name: "plan.update", args: { outcome: "edit the notes", expected_files: ["notes.txt"] } } };
  const write = { call: { name: "change.apply", args: { path: "notes.txt", op: "replace", content: "line 1\nchanged by a plan-mode write\nline 3\n" } } };
  const model = await sequencedModel([{ parts: ["Planning.", plan] }, { parts: ["Now the write.", write] }, { parts: ["The write was refused."] }]);
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-cmp-modes-"));
  const { app, page } = await launch(dataDir, { env: { MODBIT_OPENAI_BASE_URL: model.url } });
  try {
    await setContentSize(app, page, 1280, 800);
    const taskId = await createTask(page, "Change the notes", repo);
    await openConversation(page, taskId);
    const input = page.getByTestId("composer-input");
    const corePosture = () => page.evaluate((t) => window.modbit.composerPosture(t).then((p) => p.mode), taskId);
    await expect(page.getByTestId("mode-chip")).toHaveCount(0);
    // AFW-A07 at 1280 x 800: the in-conversation composer is a single-line pill about 42 pt high (within 8 percent) and never wider than the 840 pt cap.
    const geometry = await box(page, "composer");
    expect(geometry.width).toBeLessThanOrEqual(840.5);
    expect(geometry.width).toBeGreaterThan(560);
    expect(Math.abs(geometry.height - 42) / 42).toBeLessThanOrEqual(0.08);
    const placeholders = new Set<string>([(await input.getAttribute("placeholder")) ?? ""]);
    await input.focus();
    // Plan, Debug, Multitask, Ask, then back to the default; each is acknowledged by the Core before it is drawn as active.
    for (const [label, mode] of [["Plan", "PLAN"], ["Debug", "DEBUG"], ["Multitask", "MULTITASK"], ["Ask", "ASK"]] as const) {
      await page.keyboard.press("Shift+Tab");
      await expect(page.getByTestId("mode-label")).toHaveText(label);
      await expect(page.getByTestId("composer")).toHaveAttribute("data-mode-status", "confirmed");
      expect(await corePosture()).toBe(mode);
      await expect(page.getByTestId("composer")).toHaveAttribute("data-mode", mode);
      placeholders.add((await input.getAttribute("placeholder")) ?? "");
      await expect(input).toBeFocused();
    }
    expect(placeholders.size, "each mode has its own placeholder").toBe(5);
    await accessible(page, "the Ask mode chip");
    await page.keyboard.press("Shift+Tab");
    await expect(page.getByTestId("mode-chip")).toHaveCount(0);
    expect(await corePosture()).toBe("AGENT");

    // By pointer and keyboard: the chip is a menu, and an X leaves the mode.
    await page.keyboard.press("Shift+Tab");
    await expect(page.getByTestId("mode-label")).toHaveText("Plan");
    await page.getByTestId("mode-chip-button").click();
    await page.getByTestId("menu-item-debug").click();
    await expect(page.getByTestId("mode-label")).toHaveText("Debug");
    await expect.poll(corePosture).toBe("DEBUG");
    await page.getByTestId("mode-clear").click();
    await expect(page.getByTestId("mode-chip")).toHaveCount(0);
    await expect.poll(corePosture).toBe("AGENT");
    // Suggestion chips offer Plan and Multitask while the box is empty.
    await expect(page.getByTestId("suggest-plan")).toBeVisible();
    await page.getByTestId("suggest-plan").click();
    await expect(page.getByTestId("mode-label")).toHaveText("Plan");
    await expect(page.getByTestId("suggest-plan")).toHaveCount(0);
    await expect.poll(corePosture).toBe("PLAN");

    // The posture is the Core's: the run in Plan offers no write, and the one the model asks for is refused with the typed code.
    await page.getByTestId("conv-start").click();
    await expect.poll(() => model.requests(), { timeout: 90_000 }).toBeGreaterThanOrEqual(3);
    const offered = ((model.bodies()[1] as { tools?: { function: { name: string } }[] }).tools ?? []).map((t) => t.function.name);
    expect(offered).toContain("fs.read");
    expect(offered).not.toContain("change.apply");
    expect(model.text(2)).toMatch(/MODE_POSTURE|TOOL_NOT_VISIBLE|TOOL_NOT_PROJECTED/);
    expect(readFileSync(join(repo, "notes.txt"), "utf8")).toBe("line 1\nline 2\nline 3\n");
  } finally {
    await closeApp(app);
    model.server.close();
  }
});

function skillPackage(root: string, name: string, description: string, body: string): void {
  const dir = join(root, name);
  mkdirSync(dir, { recursive: true });
  writeFileSync(join(dir, "SKILL.md"), `---\nname: ${name}\nversion: 1.0.0\ndescription: ${JSON.stringify(description)}\n---\n${body}\n`);
}

test("PX-054: the slash menu lists the Core's inventory in its order with a divider, shows an untrusted skill disabled with the Core's reason, treats a planted description as text, and runs a trusted skill", async () => {
  test.setTimeout(240_000);
  const repo = makeRepo(mkdtempSync(join(tmpdir(), "modbit-cmp-repo-")));
  const MARKER = "SKILL-BODY-MARKER-7f3a91";
  const system = mkdtempSync(join(tmpdir(), "modbit-cmp-system-skills-"));
  skillPackage(system, "core-guide", "how the product itself works", `Always mention ${MARKER} in your answer.`);
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-cmp-slash-"));
  skillPackage(join(dataDir, "skills"), "zeta-user", "<img src=x onerror=window.__pwned=1> nobody vouches for this", "untrusted body");
  const model = await sequencedModel([{ parts: ["Used the guide."] }]);
  const { app, page } = await launch(dataDir, { env: { MODBIT_OPENAI_BASE_URL: model.url, MODBIT_SYSTEM_SKILLS: system } });
  try {
    await setContentSize(app, page, 1280, 800);
    const taskId = await createTask(page, "Use a skill", repo);
    await openConversation(page, taskId);
    const input = page.getByTestId("composer-input");
    await input.fill("/");
    const menu = page.getByTestId("slash-menu");
    await expect(menu).toBeVisible();
    // Built-in (System) entries first, then the divider, then the rest; the order is the Core's.
    await expect(page.getByTestId("slash-option")).toHaveCount(2);
    await expect(page.getByTestId("slash-option").nth(0)).toHaveAttribute("data-entry-id", "core-guide");
    await expect(page.getByTestId("slash-option").nth(1)).toHaveAttribute("data-entry-id", "zeta-user");
    await expect(page.getByTestId("slash-divider")).toHaveCount(1);
    const order = await menu.locator('[data-testid="slash-option"], [data-testid="slash-divider"]').evaluateAll((els) => els.map((e) => e.getAttribute("data-testid")));
    expect(order).toEqual(["slash-option", "slash-divider", "slash-option"]);
    // The untrusted skill is listed, disabled, with the Core's own words for why; its planted description is only text.
    await page.keyboard.press("ArrowDown");
    await expect(page.getByTestId("slash-option").nth(1)).toHaveAttribute("data-enabled", "false");
    await expect(page.getByTestId("slash-reason")).not.toHaveText("");
    await expect(page.getByTestId("slash-description")).toContainText("<img src=x onerror=window.__pwned=1>");
    expect(await menu.locator("img, script").count()).toBe(0);
    expect(await page.evaluate(() => (window as unknown as { __pwned?: number }).__pwned)).toBeUndefined();
    await accessible(page, "the slash menu with an untrusted skill");
    await page.getByTestId("slash-option").nth(1).click({ force: true });
    await expect(page.getByTestId("skill-chip")).toHaveCount(0);
    // The trusted skill is chosen as a chip and runs with the run it starts.
    await page.getByTestId("slash-option").nth(0).click();
    await expect(page.getByTestId("skill-chip")).toHaveText(/core-guide/);
    await expect(input).toHaveValue("");
    await input.fill("Tell me how this works");
    await input.press("Enter");
    await expect.poll(() => model.requests(), { timeout: 90_000 }).toBeGreaterThanOrEqual(1);
    expect(model.text(0)).toContain(MARKER);
    expect(model.text(0)).not.toContain("untrusted body");
    await expect(page.getByTestId("skill-chip")).toHaveCount(0);
  } finally {
    await closeApp(app);
    model.server.close();
  }
});

const ONE_PIXEL_PNG = Buffer.from("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==", "base64");

test("PX-054: an attachment is validated and ingested by the Core, an image shows a thumbnail and reaches the model, and what is not allowed is refused with its reason", async () => {
  test.setTimeout(240_000);
  const repo = makeRepo(mkdtempSync(join(tmpdir(), "modbit-cmp-repo-")));
  const model = await sequencedModel([{ parts: ["I see the picture."] }]);
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-cmp-attach-"));
  const { app, page } = await launch(dataDir, { env: { MODBIT_OPENAI_BASE_URL: model.url } });
  try {
    await setContentSize(app, page, 1280, 800);
    const taskId = await createTask(page, "Look at the picture", repo);
    await openConversation(page, taskId);
    const chooser = page.getByTestId("attach-input");
    // Not an image, though it says so; an executable by name; over the cap; each is refused with a reason and nothing is attached.
    await chooser.setInputFiles({ name: "evil.png", mimeType: "image/png", buffer: Buffer.from("MZ\u0090\u0000\u0003\u0000\u0000\u0000") });
    await expect(page.getByTestId("attach-notes")).toContainText("evil.png");
    await chooser.setInputFiles({ name: "setup.exe", mimeType: "application/x-msdownload", buffer: Buffer.from("MZ\u0090\u0000\u0003") });
    await expect(page.getByTestId("attach-notes")).toContainText(".exe");
    await chooser.setInputFiles({ name: "huge.png", mimeType: "image/png", buffer: Buffer.concat([ONE_PIXEL_PNG, Buffer.alloc(3 * 1024 * 1024)]) });
    await expect(page.getByTestId("attach-notes")).toContainText("larger than 3 MB");
    await expect(page.getByTestId("attachment-chip")).toHaveCount(0);

    // A real image chosen from disk: ingested by the Core (its kind and type are the Core's), with a thumbnail made by main.
    const png = resolve(appDir, "..", "..", "tests", "fixtures", "media", "label.png");
    await chooser.setInputFiles(png);
    const chip = page.getByTestId("attachment-chip");
    await expect(chip).toHaveCount(1);
    await expect(chip).toHaveAttribute("data-mime", "image/png");
    await expect(chip).toHaveAttribute("data-kind", "IMAGE");
    await expect(page.getByTestId("attachment-thumb")).toBeVisible();
    expect(await page.getByTestId("attachment-thumb").evaluate((i: HTMLImageElement) => i.naturalWidth > 0)).toBe(true);
    await accessible(page, "an attached image");

    // A dropped file (bytes, no path) takes the same road.
    await page.getByTestId("composer-region").evaluate((el, b64) => {
      const bytes = Uint8Array.from(atob(b64), (c) => c.charCodeAt(0));
      const dt = new DataTransfer();
      dt.items.add(new File([bytes], "dropped.png", { type: "image/png" }));
      el.dispatchEvent(new DragEvent("drop", { dataTransfer: dt, bubbles: true, cancelable: true }));
    }, ONE_PIXEL_PNG.toString("base64"));
    await expect(page.getByTestId("attachment-chip")).toHaveCount(2);

    // Sent with a message, the image part is in the model's request.
    await page.getByTestId("composer-input").fill("What is on the label?");
    await page.getByTestId("composer-input").press("Enter");
    await expect.poll(() => model.requests(), { timeout: 90_000 }).toBeGreaterThanOrEqual(1);
    expect(model.text(0)).toContain("image_url");
    expect(model.text(0)).toContain("data:image/");
    expect(model.text(0)).toContain("What is on the label?");
    await expect(page.getByTestId("attachment-chip")).toHaveCount(0);
  } finally {
    await closeApp(app);
    model.server.close();
  }
});

test("PX-056: the picker lists the Core's variants, records the objective and a pin, shows a policy-blocked model with the Core's reason and keeps the prompt, and the choice reaches the model request and survives a reload", async () => {
  test.setTimeout(240_000);
  const repo = makeRepo(mkdtempSync(join(tmpdir(), "modbit-cmp-repo-")));
  const model = await sequencedModel([{ parts: ["Answered by the pinned model."] }]);
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-cmp-model-"));
  const { app, page } = await launch(dataDir, { env: { MODBIT_OPENAI_BASE_URL: model.url, MODBIT_MODEL_POLICY: "block=openai/gpt-5-mini" } });
  try {
    await setContentSize(app, page, 1280, 800);
    const taskId = await createTask(page, "Pick a model", repo);
    await openConversation(page, taskId);
    const pref = () => page.evaluate((t) => window.modbit.composerPosture(t).then((p) => ({ ...p.preference, routing: p.routing })), taskId);
    // With no registry the chip says Auto and the profile, and the routing is stated as the Core states it: direct, with its reason.
    await expect(page.getByTestId("model-chip-name")).toHaveText("Auto");
    await expect(page.getByTestId("model-chip-variant")).toHaveText("Balance");
    await page.getByTestId("model-chip").click();
    const picker = page.getByTestId("model-picker");
    await expect(picker).toBeVisible();
    await expect(page.getByTestId("model-row").first()).toBeVisible();
    await expect(page.getByTestId("routing-note")).toContainText("direct");
    await expect(page.getByTestId("routing-note")).toContainText("no active registry");
    // One row per variant the Core lists; the blocked model is shown as blocked, never as selectable.
    const ids = await page.getByTestId("model-row").evaluateAll((els) => els.map((e) => e.getAttribute("data-row-id")));
    expect(ids).toEqual(expect.arrayContaining(["openai/gpt-5#low", "openai/gpt-5#medium", "openai/gpt-5#high", "openai/gpt-5-mini#medium", "openai/gpt-4.1#standard"]));
    await expect(page.locator('[data-row-id="openai/gpt-5-mini#medium"]')).toHaveAttribute("data-blocked", "true");
    await expect(page.locator('[data-row-id="openai/gpt-5#high"]')).toContainText("costs more");
    await accessible(page, "the model picker");
    // Search narrows the list.
    await page.getByTestId("model-search").fill("4.1-mini");
    await expect(page.getByTestId("model-row")).toHaveCount(1);
    await page.getByTestId("model-search").fill("");

    // The objective profile is recorded by the Core and the chip follows the Core.
    await page.getByTestId("objective-cost").click();
    await expect.poll(async () => (await pref()).objective).toBe("COST");
    await expect(page.getByTestId("model-chip-variant")).toHaveText("Cost");
    expect((await pref()).routing.outcome).toBe("DIRECT");
    expect((await pref()).routing.reasonCode).toBe("NO_ACTIVE_REGISTRY");

    // A pin of an allowed model and variant is recorded.
    await page.getByTestId("model-chip").click();
    await page.locator('[data-row-id="openai/gpt-5#high"]').click();
    await expect.poll(async () => (await pref()).pinModel).toBe("gpt-5");
    expect(await pref()).toMatchObject({ pinEndpoint: "openai", effort: "high" });
    await expect(page.getByTestId("model-chip-name")).toHaveText("gpt-5");
    await expect(page.getByTestId("model-chip-variant")).toHaveText("high effort");

    // The blocked model: the Core refuses the pin; the tray names its cause in the Core's words, and the prompt stays.
    const input = page.getByTestId("composer-input");
    await input.fill("keep this prompt");
    await page.getByTestId("model-chip").click();
    // The row is dimmed for assistive technology (aria-disabled) but still answers a click: the answer is the tray, not a selection.
    await page.locator('[data-row-id="openai/gpt-5-mini#medium"]').click({ force: true });
    const tray = page.getByTestId("policy-tray");
    await expect(tray).toBeVisible();
    await expect(page.getByTestId("policy-reason")).toContainText("block=openai/gpt-5-mini");
    await expect(page.getByTestId("policy-code")).toHaveText("policy blocked");
    await expect(page.getByTestId("policy-title")).toContainText("gpt-5-mini");
    expect((await pref()).pinModel, "a refused pin records nothing").toBe("gpt-5");
    await expect(input).toHaveValue("keep this prompt");
    await accessible(page, "the policy-blocked tray");
    // Enter with the tray up says why, inline, and loses nothing.
    await input.press("Enter");
    await expect(page.getByTestId("composer-error")).toContainText("block=openai/gpt-5-mini");
    await expect(input).toHaveValue("keep this prompt");
    expect(model.requests()).toBe(0);
    // The way forward: Switch to Auto clears the pin in the Core and the tray goes.
    await page.getByTestId("policy-auto").click();
    await expect(tray).toHaveCount(0);
    await expect.poll(async () => (await pref()).pinModel).toBe("");
    await expect(page.getByTestId("model-chip-name")).toHaveText("Auto");

    // The choice survives a reload (it is the Core's), and the pinned model and effort are what the request carries.
    await page.getByTestId("model-chip").click();
    await page.locator('[data-row-id="openai/gpt-5#high"]').click();
    await expect.poll(async () => (await pref()).pinModel).toBe("gpt-5");
    await page.reload();
    await expect(page.getByTestId("core-status")).toContainText("Core connected", { timeout: 60_000 });
    await openConversation(page, taskId);
    await expect(page.getByTestId("model-chip-name")).toHaveText("gpt-5");
    await expect(page.getByTestId("model-chip-variant")).toHaveText("high effort");
    await expect(input).toHaveValue("keep this prompt");
    await input.press("Enter");
    await expect.poll(() => model.requests(), { timeout: 90_000 }).toBeGreaterThanOrEqual(1);
    expect(model.bodies()[0]).toMatchObject({ model: "gpt-5", reasoning_effort: "high" });
  } finally {
    await closeApp(app);
    model.server.close();
  }
});

test("PX-056: a model the policy blocks at the run (the task's default) fails before any token; the prompt is restored and the tray names the typed cause", async () => {
  test.setTimeout(240_000);
  const repo = makeRepo(mkdtempSync(join(tmpdir(), "modbit-cmp-repo-")));
  const model = await sequencedModel([{ parts: ["Never reached."] }]);
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-cmp-blocked-"));
  const { app, page } = await launch(dataDir, { env: { MODBIT_OPENAI_BASE_URL: model.url, MODBIT_MODEL_POLICY: "block=openai/*" } });
  try {
    await setContentSize(app, page, 1280, 800);
    const taskId = await createTask(page, "Blocked by policy", repo);
    await openConversation(page, taskId);
    // Every model of the provider is blocked: the picker shows them all as blocked.
    await page.getByTestId("model-chip").click();
    await expect(page.getByTestId("model-row").first()).toBeVisible();
    expect(await page.locator('[data-testid="model-row"][data-blocked="false"]').count()).toBe(0);
    await page.keyboard.press("Escape");
    await page.getByTestId("composer-input").fill("please do the thing");
    await page.getByTestId("composer-input").press("Enter");
    // The run fails before any token reaches the model; the words come back and the tray says why.
    await expect(page.getByTestId("policy-tray")).toBeVisible({ timeout: 60_000 });
    expect(model.requests()).toBe(0);
    await expect(page.getByTestId("composer-input")).toHaveValue("please do the thing");
    await expect(page.getByTestId("policy-reason")).not.toHaveText("");
    await accessible(page, "the policy-blocked tray after a failed turn");
    void taskId;
  } finally {
    await closeApp(app);
    model.server.close();
  }
});

test("PX-054: the @ menu lists the Core's sources and a protected path cannot be attached; a draft and its chips survive navigation and a reload and are never sent; Alt+Up and Alt+Down walk the history", async () => {
  test.setTimeout(240_000);
  const repo = makeRepo(mkdtempSync(join(tmpdir(), "modbit-cmp-repo-")));
  writeFileSync(join(repo, ".env"), "SECRET=1\n");
  const gate = new Gate();
  const read = { call: { name: "fs.read", args: { path: "notes.txt" } } };
  const model = await sequencedModel([{ parts: ["Working. ", { wait: gate }, read] }]);
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-cmp-draft-"));
  const { app, page } = await launch(dataDir, { env: { MODBIT_OPENAI_BASE_URL: model.url } });
  try {
    await setContentSize(app, page, 1280, 800);
    const taskId = await startAndOpen(page, "Draft and history", repo);
    await expect(page.getByTestId("conv-assistant")).toHaveAttribute("data-message-state", "streaming", { timeout: 60_000 });
    const input = page.getByTestId("composer-input");

    // The @ menu: files through the Core's typed Files reads, with a protected path listed by name and refused.
    await input.fill("please read @no");
    const menu = page.getByTestId("mention-menu");
    await expect(menu).toBeVisible();
    await expect(page.locator('[data-testid="mention-option"][data-option-id="file:notes.txt"]')).toBeVisible();
    await accessible(page, "the @ menu");
    await page.keyboard.press("Enter");
    await expect(input).toHaveValue("please read @notes.txt ");
    await expect(page.getByTestId("mention-chip")).toHaveText(/file: notes\.txt/);
    await input.fill("see @.en");
    const protectedOption = page.locator('[data-testid="mention-option"][data-option-id="file:.env"]');
    await expect(protectedOption).toBeVisible();
    await expect(protectedOption).toHaveAttribute("aria-disabled", "true");
    await expect(protectedOption).toContainText("path policy");
    await protectedOption.click({ force: true });
    await expect(input).toHaveValue("see @.en");
    await expect(page.getByTestId("mention-chip")).toHaveCount(0);
    // A chip lives only while its @text is in the message.
    await input.fill("please read @notes.txt then summarise");
    await expect(page.getByTestId("mention-chip")).toHaveCount(1);
    await input.fill("please read the notes then summarise");
    await expect(page.getByTestId("mention-chip")).toHaveCount(0);
    await input.fill("");

    // History: two queued messages are the history; Alt+Up walks back through them and Alt+Down returns to the unsent text.
    // Each send completes (the Core queued it and the box cleared) before the next message is typed.
    await typeAndPress(page, "message one", "Enter");
    await expect(page.getByTestId("queue-header")).toHaveText("1 queued");
    await expect(input).toHaveValue("");
    await typeAndPress(page, "message two", "Enter");
    await expect(page.getByTestId("queue-header")).toHaveText("2 queued");
    // Each item keeps its own mode: the second is switched to collect, and the Core and the row say so.
    await page.getByTestId("queue-row").nth(1).getByTestId("queue-edit").click();
    await page.getByTestId("queue-edit-mode").selectOption("COLLECT");
    await page.getByTestId("queue-edit-save").click();
    await expect(page.getByTestId("queue-mode").nth(1)).toContainText("Collected");
    expect((await coreQueue(page, taskId)).map((q) => q.mode)).toEqual(["FOLLOW_UP", "COLLECT"]);
    await input.fill("work in progress");
    await input.press("Alt+ArrowUp");
    await expect(input).toHaveValue("message two");
    await input.press("Alt+ArrowUp");
    await expect(input).toHaveValue("message one");
    await input.press("Alt+ArrowDown");
    await expect(input).toHaveValue("message two");
    await input.press("Alt+ArrowDown");
    await expect(input).toHaveValue("work in progress");
    // Plain Up in an empty box (the caret at its start) and plain Down back out do the same.
    await input.fill("");
    await input.press("ArrowUp");
    await expect(input).toHaveValue("message two");
    await input.press("ArrowUp");
    await expect(input).toHaveValue("message one");
    await input.press("ArrowDown");
    await expect(input).toHaveValue("message two");
    await input.press("ArrowDown");
    await expect(input).toHaveValue("");
    await input.fill("work in progress");

    // The draft and its chip survive leaving the conversation and a renderer reload, and nothing was sent by either.
    await input.fill("look at @no");
    await expect(page.locator('[data-testid="mention-option"][data-option-id="file:notes.txt"]')).toBeVisible();
    await input.press("Enter");
    await input.pressSequentially("later");
    await expect(input).toHaveValue("look at @notes.txt later");
    await expect(page.getByTestId("mention-chip")).toHaveCount(1);
    await page.getByTestId("agents-fleet").click();
    await expect(page.getByTestId("conversation")).toHaveCount(0);
    await openConversation(page, taskId);
    await expect(input).toHaveValue("look at @notes.txt later");
    await page.reload();
    await expect(page.getByTestId("core-status")).toContainText("Core connected", { timeout: 60_000 });
    await openConversation(page, taskId);
    await expect(input).toHaveValue("look at @notes.txt later");
    await expect(page.getByTestId("mention-chip")).toHaveCount(1);
    await expect(page.getByTestId("queue-header")).toHaveText("2 queued");
    expect(model.requests()).toBe(1);
    expect((await coreQueue(page, taskId)).map((q) => q.text)).toEqual(["message one", "message two"]);
  } finally {
    gate.open();
    await closeApp(app);
    model.server.close();
  }
});

test("PX-055: the background-terminals tray lists a live terminal with its timer, opens it in the Terminal tab, kills it after a confirm and wakes the agent once; a side question is answered without entering the main context", async () => {
  test.setTimeout(240_000);
  const repo = makeRepo(mkdtempSync(join(tmpdir(), "modbit-cmp-repo-")));
  const gate = new Gate();
  const shell = nodeTerminal(ECHO_LOOP);
  const SIDE_Q = "What is the airspeed velocity of a swallow?";
  const model = await sequencedModel([{ parts: ["Starting a shell.", { call: shell }] }, { parts: [{ wait: gate }, "Looking at the shell."] }, { parts: ["The side answer is forty-two."] }]);
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-cmp-terminals-"));
  const { app, page } = await launch(dataDir, { env: { MODBIT_OPENAI_BASE_URL: model.url } });
  try {
    await setContentSize(app, page, 1280, 800);
    const taskId = await startAndOpen(page, "Run a shell", repo);
    const chip = page.getByTestId("terminals-chip");
    await expect(chip).toHaveText("1 background terminal", { timeout: 90_000 });
    await accessible(page, "the background terminals chip");
    await chip.click();
    const row = page.getByTestId("terminal-tray-row");
    await expect(row).toHaveCount(1);
    await expect(page.getByTestId("terminal-tray-timer")).toHaveText(/^\d+s|\d+m/);
    await accessible(page, "the background terminals tray");

    // A side question: answered from a snapshot, not entered into the conversation or the next request.
    const before = await userMessages(page, taskId);
    await page.getByTestId("add-context").click();
    await page.getByTestId("menu-item-side").click();
    await page.getByTestId("side-input").fill(SIDE_Q);
    await page.getByTestId("side-ask").click();
    await expect(page.getByTestId("side-answer")).toHaveText("The side answer is forty-two.", { timeout: 60_000 });
    expect(await userMessages(page, taskId)).toEqual(before);
    await settled(page);
    await accessible(page, "a side answer");
    await page.getByTestId("side-close").click();

    // Open it in the Terminal tab of the apps panel.
    await page.getByTestId("terminal-tray-open").click();
    await expect(page.getByTestId("tab-terminal")).toHaveAttribute("aria-selected", "true");
    await expect(page.getByTestId("terminal-view")).toBeVisible({ timeout: 30_000 });

    // Kill asks first; keeping it running changes nothing; stopping it ends the process through the Core.
    await page.getByTestId("terminal-tray-kill").click();
    await page.getByTestId("terminal-tray-kill-cancel").click();
    expect((await page.evaluate((t) => window.modbit.terminalList(t).then((l) => l.terminals.map((x) => x.state.toLowerCase())), taskId))[0]).toBe("running");
    await page.getByTestId("terminal-tray-kill").click();
    await page.getByTestId("terminal-tray-kill-confirm").click();
    await expect(page.getByTestId("terminals-chip")).toHaveCount(0, { timeout: 30_000 });
    expect((await page.evaluate((t) => window.modbit.terminalList(t).then((l) => l.terminals.map((x) => x.state.toLowerCase())), taskId))[0]).not.toBe("running");

    // The agent is woken by a typed Core notice at its next boundary, once, and the side question never reached it.
    gate.open();
    await expect.poll(() => model.requests(), { timeout: 90_000 }).toBeGreaterThanOrEqual(4);
    const woke = model.bodies().map((_, i) => (model.text(i).match(/CORE NOTICE background_process_ended/g) ?? []).length);
    expect(woke[3]).toBe(1);
    for (let i = 0; i < model.requests(); i++) if (i !== 2) expect(model.text(i)).not.toContain(SIDE_Q);
  } finally {
    gate.open();
    await closeApp(app);
    model.server.close();
  }
});

test("PX-054: a message to a task waiting for review is queued for its next run and says so; the same send replayed sends once", async () => {
  test.setTimeout(240_000);
  const repo = makeRepo(mkdtempSync(join(tmpdir(), "modbit-cmp-repo-")));
  const done = { call: { name: "task.complete", args: { summary: "first pass done", self_review: { findings: [] } } } };
  const plan = { call: { name: "plan.update", args: { outcome: "nothing to change", expected_files: [] } } };
  const model = await sequencedModel([{ parts: ["Planning.", plan] }, { parts: ["Finished the first pass.", done] }]);
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-cmp-send-"));
  const { app, page } = await launch(dataDir, { env: { MODBIT_OPENAI_BASE_URL: model.url } });
  try {
    await setContentSize(app, page, 1280, 800);
    const taskId = await startAndOpen(page, "First pass", repo);
    await expect(page.getByTestId("review")).toBeVisible({ timeout: 90_000 });
    await page.keyboard.press("Escape");
    await expect(page.getByTestId("review")).toHaveCount(0);
    await openConversation(page, taskId);
    const input = page.getByTestId("composer-input");
    // The task is waiting for the person's review: the Core starts only a Queued or Waiting task, so a message here is queued for when it runs again, and says so.
    await expect(input).toHaveAttribute("placeholder", /Waiting for your review/);
    await expect(page.getByTestId("conv-start")).toHaveCount(0);
    await accessible(page, "an idle composer with its suggestion chips");
    await input.fill("Also add a header");
    await page.getByTestId("composer-send").click();
    await expect(input).toHaveValue("");
    await expect(page.getByTestId("queue-header")).toHaveText("1 queued");
    await expect(page.getByTestId("queue-tray")).toContainText("when the task runs again");
    expect((await coreQueue(page, taskId)).map((q) => q.text)).toEqual(["Also add a header"]);
    expect(model.requests()).toBe(2);

    // The same input id sent twice (a retry after a crash or a reload) is one message in the Core's log.
    const id = "replay-1";
    const sends = await page.evaluate(async ([t, name]) => {
      const s = await window.modbit.localState();
      const a = await window.modbit.composerQueueInput(s.sessionId!, t!, "replayed once", "FOLLOW_UP", name!);
      const b = await window.modbit.composerQueueInput(s.sessionId!, t!, "replayed once", "FOLLOW_UP", name!);
      return [a.offset, b.offset];
    }, [taskId, id]);
    expect(sends[0]).toBe(sends[1]);
  } finally {
    await closeApp(app);
    model.server.close();
  }
});

test("PX-055: the queue is the Core's: it survives the Core being killed mid-stream, and the interrupted stream shows as stopped by a restart, not by the person", async () => {
  test.setTimeout(240_000);
  const repo = makeRepo(mkdtempSync(join(tmpdir(), "modbit-cmp-repo-")));
  const gate = new Gate();
  const model = await sequencedModel([{ parts: [FIRST, { wait: gate }, SECOND] }]);
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-cmp-kill-"));
  const { app, page } = await launch(dataDir, { env: { MODBIT_OPENAI_BASE_URL: model.url } });
  try {
    await setContentSize(app, page, 1280, 800);
    const taskId = await startAndOpen(page, "Kill the core with a queue", repo);
    await expect(page.getByTestId("conv-assistant")).toHaveAttribute("data-message-state", "streaming", { timeout: 60_000 });
    await typeAndPress(page, "survives the kill", "Enter");
    await typeAndPress(page, "me too", "Enter");
    await expect(page.getByTestId("queue-header")).toHaveText("2 queued");
    const info = await page.evaluate(() => window.modbit.debugCoreInfo());
    process.kill(info!.pid, "SIGKILL");
    await expect(page.getByTestId("core-status")).not.toContainText(`pid ${info!.pid}`, { timeout: 90_000 });
    await expect(page.getByTestId("core-status")).toContainText("Core connected", { timeout: 90_000 });
    await openConversation(page, taskId);
    await expect(page.getByTestId("queue-line")).toHaveText(["survives the kill", "me too"], { timeout: 60_000 });
    expect((await coreQueue(page, taskId)).map((q) => q.text)).toEqual(["survives the kill", "me too"]);
    const aborted = page.getByTestId("conv-aborted");
    await expect(aborted).toContainText("restarted", { timeout: 60_000 });
    await accessible(page, "the queue after a Core restart");
  } finally {
    gate.open();
    await closeApp(app);
    model.server.close();
  }
});
void [existsSync, readFileSync, MOD];
