/**
 * DR-PX-2026-11-04-020 sections 5.9 to 5.12: what a person supervising tasks can
 * see and choose without opening anything. The real app, a real Core and a
 * scripted provider: the mode and model chosen on home reach the Core before the
 * first model call, and a refusal leaves the task unstarted; the first-class views
 * hold the Core's classes; a row and the conversation say what is wrong; the
 * shortcut help names no board; the location says what it is.
 */
import { expect, test } from "@playwright/test";
import { mkdtempSync } from "node:fs";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { accessible, closeApp, launch, makeRepo, setContentSize } from "./support/ui-harness.ts";
import { sequencedModel } from "./support/composer-harness.ts";
import { Gate } from "./support/stream-model.ts";
import { agentRow, expectTaskState, openTask, queueTask, toNewTask } from "./support/new-task.ts";

const tmp = (name: string) => mkdtempSync(join(tmpdir(), `modbit-e2e-sup-${name}-`));
const newRepo = () => makeRepo(tmp("repo"));
const opened = (page: import("@playwright/test").Page) => page.getByTestId("conversation").getAttribute("data-task-id") as Promise<string>;

function reservePort(): Promise<number> {
  return new Promise((done) => {
    const s = createServer();
    s.listen(0, "127.0.0.1", () => {
      const port = (s.address() as { port: number }).port;
      s.close(() => done(port));
    });
  });
}

test("home: the mode and the pin reach the Core before the first model call; a blocked model is not held; the next task starts from the defaults", async () => {
  test.setTimeout(180_000);
  const repo = newRepo();
  const model = await sequencedModel([{ parts: ["Planned."] }]);
  const { app, page } = await launch(tmp("choice"), { env: { MODBIT_OPENAI_BASE_URL: model.url, MODBIT_MODEL_POLICY: "block=openai/gpt-5-mini" } });
  try {
    await setContentSize(app, page, 1280, 800);
    await toNewTask(page);
    await expect(page.getByTestId("home-mode")).toHaveAttribute("data-mode", "AGENT");
    await expect(page.getByTestId("model-chip-name")).toHaveText("Auto");
    await expect(page.getByTestId("model-chip-variant")).toHaveText("Balance");
    await page.getByTestId("home-mode-button").click();
    await accessible(page, "the home mode menu");
    await page.getByTestId("menu-item-plan").click();
    await expect(page.getByTestId("home-mode")).toHaveAttribute("data-mode", "PLAN");

    // The catalog needs no task. A row the Core says cannot be pinned is refused with its reason and not held.
    await page.getByTestId("model-chip").click();
    await page.locator('[data-row-id="openai/gpt-5-mini#medium"]').click({ force: true });
    await expect(page.getByTestId("home-model-note")).toContainText("gpt-5-mini is not available");
    await expect(page.getByTestId("model-chip-name")).toHaveText("Auto");

    await page.getByTestId("model-chip").click();
    await page.locator('[data-row-id="openai/gpt-4.1#standard"]').click();
    await expect(page.getByTestId("home-model-note")).toHaveCount(0);
    await expect(page.getByTestId("model-chip-name")).toHaveText("gpt-4.1");

    await page.getByTestId("goal").fill("Plan a small change");
    await page.getByTestId("workspace").fill(repo);
    await page.getByTestId("run").click();
    await expect(page.getByTestId("conversation")).toBeVisible();
    const taskId = await opened(page);

    // The first request the model ever saw was on the pin, so the pin was recorded before the start.
    await expect.poll(() => model.requests(), { timeout: 60_000 }).toBeGreaterThan(0);
    expect(model.bodies()[0]!.model).toBe("gpt-4.1");
    const posture = await page.evaluate((t) => window.modbit.composerPosture(t), taskId);
    expect(posture.mode).toBe("PLAN");
    expect(posture.preference).toMatchObject({ pinEndpoint: "openai", pinModel: "gpt-4.1" });
    await expect(page.getByTestId("mode-label")).toHaveText("Plan");
    await expect(page.getByTestId("model-chip-name")).toHaveText("gpt-4.1");

    // Nothing carries over to the next task.
    await toNewTask(page);
    await expect(page.getByTestId("home-mode")).toHaveAttribute("data-mode", "AGENT");
    await expect(page.getByTestId("model-chip-name")).toHaveText("Auto");

    // An objective with Alt+Enter: recorded, not started. Shift+Tab cycles the mode here as in the composer.
    await page.getByTestId("goal").focus();
    await page.keyboard.press("Shift+Tab");
    await expect(page.getByTestId("home-mode")).toHaveAttribute("data-mode", "PLAN");
    await page.keyboard.press("Shift+Tab");
    await expect(page.getByTestId("home-mode")).toHaveAttribute("data-mode", "DEBUG");
    await page.getByTestId("model-chip").click();
    await page.getByTestId("objective-cost").click();
    await expect(page.getByTestId("model-chip-variant")).toHaveText("Cost");
    const before = model.requests();
    const queued = await queueTask(page, "Queued with a cost objective", repo);
    await expect.poll(() => page.evaluate((t) => window.modbit.composerPosture(t).then((p) => `${p.mode}/${p.preference.objective}`), queued)).toBe("DEBUG/COST");
    await expectTaskState(page, queued, "Queued");
    expect(model.requests()).toBe(before);
  } finally {
    await closeApp(app);
    model.server.closeAllConnections();
    model.server.close();
  }
});

test("home: a mode the Core refuses leaves the task created, unstarted and saying why; no model call is made", async () => {
  test.setTimeout(120_000);
  const repo = newRepo();
  const model = await sequencedModel([{ parts: ["Must not run."] }]);
  const { app, page } = await launch(tmp("refused"), { env: { MODBIT_OPENAI_BASE_URL: model.url } });
  try {
    await setContentSize(app, page, 1280, 800);
    await toNewTask(page);
    // The one injected fault: nothing in the real Core refuses a mode on a new task, so main answers as a refusing Core would.
    await app.evaluate(({ ipcMain }) => {
      ipcMain.removeHandler("composer:setMode");
      ipcMain.handle("composer:setMode", () => {
        throw new Error("PLAN_MODE_FORBIDDEN: refused for this test");
      });
    });
    await page.getByTestId("home-mode-button").click();
    await page.getByTestId("menu-item-plan").click();
    await page.getByTestId("goal").fill("Must stay unstarted");
    await page.getByTestId("workspace").fill(repo);
    await page.getByTestId("run").click();
    await expect(page.getByTestId("banner-error")).toContainText("created and not started");
    await expect(page.getByTestId("banner-error")).toContainText("PLAN_MODE_FORBIDDEN");
    const taskId = await opened(page);
    await expectTaskState(page, taskId, "Queued");
    await expect(page.getByTestId("conv-start")).toBeVisible();
    expect(model.requests()).toBe(0);
  } finally {
    await closeApp(app);
    model.server.closeAllConnections();
    model.server.close();
  }
});

test("home: the location is a selector that names this machine and says why cloud cannot be chosen; the shortcut help names no board", async () => {
  const { app, page } = await launch(tmp("location"));
  try {
    await setContentSize(app, page, 1280, 800);
    await toNewTask(page);
    const machine = process.platform === "darwin" ? "This Mac" : process.platform === "win32" ? "This PC" : "This computer";
    await expect(page.getByTestId("home-location")).toHaveText(machine);
    await page.getByTestId("home-location").click();
    await expect(page.getByTestId("menu-item-location-local")).toHaveAttribute("aria-checked", "true");
    const cloud = page.getByTestId("menu-item-location-cloud");
    await expect(cloud).toHaveAttribute("aria-disabled", "true");
    await expect(cloud).toContainText("not available from this app yet");
    await accessible(page, "the location menu");
    await cloud.click({ force: true });
    await expect(page.getByTestId("home-location")).toHaveText(machine);
    await page.keyboard.press("Escape");

    await page.getByTestId("goal").blur();
    await page.keyboard.press("?");
    const help = page.getByTestId("help");
    await expect(help).toBeVisible();
    expect(await help.innerText()).not.toMatch(/\b(board|columns?|cards?|fleet)\b/i);
    await page.keyboard.press("Escape");

    // The side arrows are no longer swallowed for a column that does not exist.
    const prevented = await page.evaluate(() => {
      const e = new KeyboardEvent("keydown", { key: "ArrowRight", bubbles: true, cancelable: true });
      document.body.dispatchEvent(e);
      return e.defaultPrevented;
    });
    expect(prevented).toBe(false);
  } finally {
    await closeApp(app);
  }
});

test("a stopped task says what is wrong and what to do on its row and in its conversation, with nothing opened", async () => {
  test.setTimeout(120_000);
  const repo = newRepo();
  const port = await reservePort();
  const { app, page } = await launch(tmp("stopped"), { env: { MODBIT_OPENAI_BASE_URL: `http://127.0.0.1:${port}/v1` } });
  try {
    await setContentSize(app, page, 1280, 800);
    await toNewTask(page);
    await page.getByTestId("goal").fill("Provider down");
    await page.getByTestId("workspace").fill(repo);
    await page.getByTestId("run").click();
    await expect(page.getByTestId("conversation")).toBeVisible();
    const taskId = await opened(page);
    await expect(page.getByTestId("task-details")).toHaveCount(0);
    const row = agentRow(page, taskId);
    await expect(row.getByTestId("agent-facts")).not.toHaveText("", { timeout: 60_000 });
    // The failure block says it, in the conversation, with its evidence; the notice does not repeat it.
    await expect(page.getByTestId("conv-failure-cause")).toContainText("connect failed");
    await expect(page.getByTestId("conv-failure-next")).toBeVisible();
    await expect(page.getByTestId("conv-failure-evidence")).not.toBeEmpty();
    await expect(page.getByTestId("task-notice")).toHaveAttribute("data-shown", "false");
    await accessible(page, "a stopped task with its notice");
    // Survives a reload: the facts come back from the Core, not from memory.
    const said = await row.getByTestId("agent-facts").innerText();
    await page.reload();
    await expect(agentRow(page, taskId).getByTestId("agent-facts")).toContainText(said.replace(/^ · /, "").split(" · ")[0]!, { timeout: 30_000 });
    expect(await agentRow(page, taskId).getByTestId("agent-open").getAttribute("title")).toContain("Next: ");
  } finally {
    await closeApp(app);
  }
});

test("a state nothing else on the screen says stands above the composer: a Core that dies while a task is open", async () => {
  test.setTimeout(120_000);
  const repo = newRepo();
  const { app, page } = await launch(tmp("notice"));
  try {
    await setContentSize(app, page, 1280, 800);
    await queueTask(page, "Open while the Core dies", repo);
    await expect(page.getByTestId("task-notice")).toHaveAttribute("data-shown", "false");
    // The state lasts as long as the restart, so it is recorded as it happens rather than caught by a poll.
    await page.evaluate(() => {
      const seen: string[] = [];
      (window as unknown as { __notices: string[] }).__notices = seen;
      new MutationObserver(() => {
        const n = document.querySelector<HTMLElement>('[data-testid="task-notice"]');
        if (n?.dataset.shown !== "true") return;
        const visible = n.getBoundingClientRect().height > 8;
        const label = n.querySelector('[data-testid="task-screen-state-label"]')?.textContent ?? "";
        const next = n.querySelector('[data-testid="task-screen-state-next"]')?.textContent ?? "";
        if (visible && next) seen.push(label);
      }).observe(document.body, { subtree: true, childList: true, attributes: true, characterData: true });
    });
    const info = await page.evaluate(() => window.modbit.debugCoreInfo());
    process.kill(info!.pid, "SIGKILL");
    await expect.poll(() => page.evaluate(() => (window as unknown as { __notices: string[] }).__notices), { timeout: 60_000 }).toContain("reconnecting by cursor");
    await expect(page.getByTestId("core-status")).toHaveAttribute("data-status", "ok", { timeout: 60_000 });
    await expect(page.getByTestId("task-notice")).toHaveAttribute("data-shown", "false");
  } finally {
    await closeApp(app);
  }
});

test("MOD-UX-001: each state a task can be in is a first-class view, holding the Core's class, one click away", async () => {
  test.setTimeout(240_000);
  const repo = newRepo();
  const hold = new Gate();
  const complete = { call: { name: "task.complete", args: { summary: "nothing to change", self_review: { findings: [] } } } };
  const model = await sequencedModel([
    { parts: [{ call: { name: "plan.update", args: { outcome: "nothing to change", expected_files: [] } } }] },
    { parts: [complete] },
    { parts: ["Cleaning.", { call: { name: "shell.exec", args: { argv: ["git", "clean", "-fd"] } } }] },
    async () => {
      await hold.opened;
      return { parts: ["Late."] };
    },
  ]);
  const { app, page } = await launch(tmp("views"), { env: { MODBIT_OPENAI_BASE_URL: model.url } });
  try {
    await setContentSize(app, page, 1280, 800);
    const start = async (goal: string) => {
      await toNewTask(page);
      await page.getByTestId("goal").fill(goal);
      await page.getByTestId("workspace").fill(repo);
      await page.getByTestId("run").click();
      await expect(page.getByTestId("conversation")).toHaveAttribute("data-task-id", /.+/);
      return opened(page);
    };
    // Each one reaches its state before the next asks the model, so the script's replies stay in order.
    const review = await start("Finish for review");
    await expect(agentRow(page, review)).toHaveAttribute("data-class", /READY_FOR_REVIEW/, { timeout: 60_000 });
    const attention = await start("Ask to clean");
    await expect(agentRow(page, attention)).toHaveAttribute("data-class", "NEEDS_ATTENTION", { timeout: 60_000 });
    // A command still waiting on approval has not run, and the conversation does not say it has.
    const pendingStep = page.locator('[data-testid="conv-tool"][data-status="AWAITING_APPROVAL"]');
    await expect(pendingStep).toContainText("Run");
    await expect(pendingStep).toContainText("Waiting for approval");
    await expect(pendingStep).not.toContainText("Ran");
    const running = await start("Keep running");
    await expect(agentRow(page, running)).toHaveAttribute("data-class", "RUNNING", { timeout: 60_000 });

    const states = page.getByTestId("agents-states");
    await expect(states.getByTestId("state-view-review")).toHaveAttribute("data-count", "1");
    await expect(states.getByTestId("state-view-attention")).toHaveAttribute("data-count", "1");
    await expect(states.getByTestId("state-view-running")).toHaveAttribute("data-count", "1");
    // A state with no task takes no room, and Completed is not Ready for review.
    await expect(states.getByTestId("state-view-done")).toHaveCount(0);
    await expect(states.getByTestId("state-view-waiting")).toHaveCount(0);
    await accessible(page, "the state views");

    await expect(states.getByTestId("state-view-review")).toHaveAccessibleName("Ready for review, 1");
    await states.getByTestId("state-view-review").click();
    await expect(states.getByTestId("state-view-review")).toHaveAttribute("aria-pressed", "true");
    await expect(page.getByTestId("agent-row")).toHaveCount(1);
    await expect(agentRow(page, review)).toBeVisible();
    // A view is not the search: choosing one leaves the tools folded.
    await expect(page.getByTestId("agents-controls")).toHaveCount(0);
    await states.getByTestId("state-view-attention").click();
    await expect(page.getByTestId("agent-row")).toHaveCount(1);
    await expect(agentRow(page, attention)).toBeVisible();
    await expect(agentRow(page, attention).getByTestId("agent-facts")).toContainText("asks for a");
    await states.getByTestId("state-view-attention").click();
    await expect(page.getByTestId("agent-row")).toHaveCount(3);
  } finally {
    hold.open();
    await closeApp(app);
    model.server.closeAllConnections();
    model.server.close();
  }
});

test("PX-196/197: Escape with the details open closes them, not the card; a confirmation keeps its keys from a card docking after it; a denied call is never told as run", async () => {
  test.setTimeout(180_000);
  const repo = newRepo();
  const hold = new Gate();
  const model = await sequencedModel([
    async () => {
      await hold.opened;
      return { parts: ["Cleaning.", { call: { name: "shell.exec", args: { argv: ["git", "clean", "-fd"] } } }] };
    },
    { parts: ["Left it."] },
  ]);
  const { app, page } = await launch(tmp("escape"), { env: { MODBIT_OPENAI_BASE_URL: model.url } });
  try {
    await setContentSize(app, page, 1280, 800);
    await toNewTask(page);
    await page.getByTestId("goal").fill("Clean the build output");
    await page.getByTestId("workspace").fill(repo);
    await page.getByTestId("run").click();
    await expect(page.getByTestId("conversation")).toBeVisible();
    // A confirmation first, then a card docks behind it: Escape is the confirmation's "back", never the card's Skip.
    await page.getByTestId("task-details-toggle").click();
    await page.getByTestId("task-cancel").click();
    await expect(page.getByTestId("confirm")).toHaveAttribute("data-confirm-kind", "cancel");
    if (await page.getByTestId("task-details").isVisible()) await page.getByTestId("task-details-toggle").click();
    hold.open();
    await expect(page.getByTestId("approval-card")).toBeVisible({ timeout: 60_000 });
    await page.keyboard.press("Escape");
    await expect(page.getByTestId("confirm")).toHaveCount(0);
    await expect(page.getByTestId("approval-card"), "the card behind the confirmation was not skipped").toBeVisible();
    // With the details open, Escape closes them and the card stays.
    await page.getByTestId("task-details-toggle").click();
    await expect(page.getByTestId("task-details")).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(page.getByTestId("task-details")).toHaveCount(0);
    await expect(page.getByTestId("approval-card"), "Escape stepped back from the details and decided nothing").toBeVisible();
    // Skipped on purpose now: the command never ran, and nothing says it did.
    await page.getByTestId("approval-skip").click();
    await expect(page.getByTestId("approval-card")).toHaveCount(0);
    const tool = page.getByTestId("conv-tool").filter({ hasText: "git clean -fd" });
    await expect(tool).toHaveAttribute("data-status", "DENIED", { timeout: 30_000 });
    await expect(tool).not.toContainText("Ran");
    for (const summary of await page.getByTestId("conv-group-summary").all()) await expect(summary).not.toContainText("Ran git clean");
  } finally {
    hold.open();
    await closeApp(app);
    model.server.closeAllConnections();
    model.server.close();
  }
});

test("PX-196: after a reload the details show the facts the Core folded from the log, not defaults; failed calls are told as failed", async () => {
  test.setTimeout(240_000);
  const repo = newRepo();
  const model = await sequencedModel([
    { parts: [{ call: { name: "plan.update", args: { outcome: "nothing to change", expected_files: [] } } }] },
    { parts: [{ call: { name: "shell.exec", args: { argv: ["sh", "-c", "exit 3"] } } }, { call: { name: "shell.exec", args: { argv: ["sh", "-c", "exit 4"] } } }] },
    { parts: [{ call: { name: "task.complete", args: { summary: "nothing to change", self_review: { findings: [] } } } }] },
    { parts: ["done"] },
  ]);
  const { app, page } = await launch(tmp("reload"), { env: { MODBIT_OPENAI_BASE_URL: model.url } });
  try {
    await setContentSize(app, page, 1280, 800);
    await toNewTask(page);
    await page.getByTestId("goal").fill("Try twice and stop");
    await page.getByTestId("workspace").fill(repo);
    await page.getByTestId("run").click();
    const id = await opened(page);
    await expect(agentRow(page, id)).toHaveAttribute("data-state", "Waiting", { timeout: 90_000 });
    const failedFold = page.getByTestId("conv-group-summary").filter({ hasText: "2 failed" });
    await expect(failedFold).toHaveCount(1);
    await expect(failedFold).not.toContainText("Ran");
    const facts = async () => {
      await page.getByTestId("task-details-toggle").click();
      const out = [await page.getByTestId("task-phase").innerText(), await page.getByTestId("task-agents").innerText(), await page.getByTestId("task-attachments").innerText()];
      await page.keyboard.press("Escape");
      return out;
    };
    const live = await facts();
    expect(live[0]).not.toBe("drafting");
    await page.reload();
    await openTask(page, id);
    await expect.poll(facts, { message: "the details after a reload equal the details live" }).toEqual(live);
    await accessible(page, "the conversation after a reload");
  } finally {
    await closeApp(app);
    model.server.closeAllConnections();
    model.server.close();
  }
});

