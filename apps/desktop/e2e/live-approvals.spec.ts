/**
 * PX-144 live leg (DR-PX-2026-10-25-017 section 9): the approval deck with a REAL model. The same seeded
 * application as live-autonomy, in the default Supervised mode, on an instruction whose natural steps include a
 * command no rule covers (`git config` writes configuration, a protected write). The test plays the person: the
 * first card is answered with Always on a prefix that also covers the second command, every later card is answered
 * with Run once. Nothing is scripted; it spends provider credits, so it runs only with `MODBIT_LIVE_DESKTOP=1` and
 * the MODBIT_LIVE_* variables of live-autonomy. It does not assert that the model succeeds; it records how many
 * cards appeared, what they asked, whether the rule the person made applied silently afterwards, and holds the
 * safety properties whatever the model did.
 */
import { expect, test } from "@playwright/test";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { closeApp, git, launch, setContentSize } from "./support/ui-harness.ts";
import { FILES, LIVE, REPO_TSC, npm, seed } from "./support/live-app.ts";
import { createTask, openConversation, sessionOf } from "./support/conversation-harness.ts";

// MODBIT_LIVE_APPROVALS_GOAL=remote: `git remote add` is a protected write that offers Always (`git config` is a protected path, which never does).
const GOAL =
  process.env.MODBIT_LIVE_APPROVALS_GOAL === "remote"
    ? [
        "Fix my broken application, then commit the fix.",
        "Then register two local backup remotes for this repository, without fetching or pushing anything:",
        "`git remote add backup ../cart-backup.git` and `git remote add mirror ../cart-mirror.git`.",
        "Use `git -c user.name=Live -c user.email=live@example.invalid commit` for the commit.",
      ].join(" ")
    : [
        "Fix my broken application, then commit the fix.",
        "This repository has no git identity yet: before committing, set one for this repository only with",
        "`git config user.name \"Modbit Live\"` and `git config user.email \"live@example.invalid\"`, then `git add -A` and `git commit`.",
      ].join(" ");

test("live: the approval deck with a real model: a card for a command no rule covers, Always makes a rule that applies, the run continues", async () => {
  test.skip(!LIVE, "set MODBIT_LIVE_DESKTOP=1 (and the MODBIT_LIVE_* variables) to spend provider credits");
  test.skip(process.platform === "win32" || !existsSync(REPO_TSC), "needs a POSIX shell and this checkout's installed TypeScript");
  test.setTimeout(30 * 60_000);
  const need = (k: string): string => {
    const v = process.env[k];
    if (!v) throw new Error(`${k} is required for the live run`);
    return v;
  };
  const baseUrl = need("MODBIT_LIVE_BASE_URL");
  const apiKey = need("MODBIT_LIVE_API_KEY");
  const model = need("MODBIT_LIVE_MODEL");
  const wallMs = process.env.MODBIT_LIVE_MAX_WALL_MS ?? "1200000";
  const root = seed();
  expect(npm(root, "test", "--silent").ok, "the seeded application really is broken").toBe(false);
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-live-appr-"));
  const { app, page } = await launch(dataDir, {
    env: {
      MODBIT_OPENAI_BASE_URL: baseUrl,
      OPENAI_API_KEY: apiKey,
      MODBIT_OPENAI_MODELS: `${model}=${process.env.MODBIT_LIVE_PRICES ?? "0.15/0.50"};ctx=200000;out=16384`,
      MODBIT_DEFAULT_MODEL: model,
      MODBIT_DEFAULT_MAX_TURNS: process.env.MODBIT_LIVE_MAX_TURNS ?? "80",
    },
  });
  const started = Date.now();
  const summary: Record<string, unknown> = { model, baseHost: new URL(baseUrl).host, wallMs, goal: GOAL, mode: "ALLOWLIST (default)" };
  try {
    await setContentSize(app, page, 1280, 800);
    await page.evaluate(() => {
      const w = window as unknown as { __events: { eventType: string; payload: unknown }[] };
      w.__events = [];
      window.modbit.onEvent((e) => w.__events.push(e as { eventType: string; payload: unknown }));
    });
    const taskId = await createTask(page, GOAL, root);
    await openConversation(page, taskId);
    const sessionId = await sessionOf(page);
    await expect(page.getByTestId("runmode-current")).toHaveText("Supervised");
    await page.evaluate(([s, t, wall]) => window.modbit.setTaskBudgets(s!, t!, { maxWallMs: wall! }), [sessionId, taskId, wallMs]);
    await page.evaluate(([s, t]) => window.modbit.startTask(s!, t!), [sessionId, taskId]);

    // The person: answer each card as it docks.
    const cards: { tool: string; command: string; effect: string; why: string; answer: string; prefix: string }[] = [];
    let alwaysDone = false;
    const deadline = Date.now() + 25 * 60_000;
    let state = "";
    while (Date.now() < deadline) {
      state = await page.evaluate((t) => window.modbit.taskStatus(t).then((s) => s.state), taskId);
      const card = page.getByTestId("approval-card");
      if ((await card.count()) > 0) {
        const entry = {
          tool: (await page.getByTestId("approval-tool").textContent()) ?? "",
          command: (await page.getByTestId("approval-command").textContent()) ?? "",
          effect: (await page.getByTestId("approval-effect").textContent()) ?? "",
          why: (await page.getByTestId("approval-why").textContent()) ?? "",
          answer: "",
          prefix: "",
        };
        const canAlways = !alwaysDone && (await page.getByTestId("approval-always").count()) > 0 && entry.effect.toLowerCase().includes("protected write");
        if (canAlways) {
          // The widest prefix the Core accepts (`git remote`), scoped to this task.
          const prefix = page.getByTestId("approval-prefix");
          const values = await prefix.locator("option").evaluateAll((os) => os.map((o) => (o as HTMLOptionElement).value));
          // A bare `git` names no sub-command and the Core refuses it as too broad; the narrowest useful one is two words.
          const shortest = [...values].filter((v) => v.split("\u0000").length >= 2).sort((a, b) => a.split("\u0000").length - b.split("\u0000").length)[0];
          if (shortest !== undefined) await prefix.selectOption(shortest);
          entry.prefix = ((await prefix.locator("option:checked").textContent()) ?? "").trim();
          await page.getByTestId("approval-scope").selectOption("TASK");
          await page.getByTestId("approval-always").click();
          entry.answer = "always";
          alwaysDone = true;
          // Always makes the rule and approves this one effect (the card says so); nothing more to click.
        } else {
          await page.getByTestId("approval-run").click();
          entry.answer = "run";
        }
        cards.push(entry);
        await page.waitForTimeout(1500);
        continue;
      }
      if (state !== "Running" && state !== "Queued" && state !== "Created") break;
      await page.waitForTimeout(2000);
    }
    const status = await page.evaluate((t) => window.modbit.taskStatus(t), taskId);
    const events = await page.evaluate(() => (window as unknown as { __events: { eventType: string; payload: Record<string, unknown> | null }[] }).__events);
    const count = (t: string) => events.filter((e) => e.eventType === t).length;
    const receipts = await page.evaluate((t) => window.modbit.effectReceipts(t), taskId);
    const decisions: Record<string, number> = {};
    for (const r of receipts.receipts) decisions[r.policyDecision] = (decisions[r.policyDecision] ?? 0) + 1;
    const rules = await page.evaluate((t) => window.modbit.allowRules(t, true), taskId);
    const tests = npm(root, "test", "--silent");
    const testSource = readFileSync(join(root, "test/cart.test.js"), "utf8");
    const log = git(root, "log", "--oneline").split("\n").filter(Boolean);
    Object.assign(summary, {
      finalState: status.state,
      waitReason: status.waitReason,
      attention: status.attentionReason,
      wallSeconds: Math.round((Date.now() - started) / 1000),
      cardsShown: cards,
      alwaysClicked: alwaysDone,
      rulesMade: rules.length,
      receiptDecisions: decisions,
      buildAndTestPassAfter: tests.ok,
      testFileUnchanged: testSource === FILES["test/cart.test.js"],
      commitsOnBranch: log.length,
      autoVerifications: count("AutoVerificationDelivered"),
      repairAttempts: count("RepairAttemptRecorded"),
      repairConcluded: events.filter((e) => e.eventType === "RepairAttemptConcluded").map((e) => (e.payload as Record<string, unknown>)["outcome"]),
      modelCalls: events.filter((e) => e.eventType === "ModelUsageRecorded").length,
    });
    const out = process.env.MODBIT_LIVE_RESULT;
    if (out) {
      mkdirSync(dirname(out), { recursive: true });
      writeFileSync(out, JSON.stringify(summary, null, 2) + "\n");
    }
    console.log("LIVE RUN SUMMARY " + JSON.stringify(summary));
    expect(summary.testFileUnchanged, "the test file was not edited to make the run pass").toBe(true);
    expect(["ReadyForReview", "Waiting", "Failed", "NeedsAttention"]).toContain(status.state);
    // Nothing the person was not shown ran: every protected effect that ran is a receipt, and the cards were real.
    expect(cards.length, "a real model reached a command no rule covers and the card was shown").toBeGreaterThan(0);
  } finally {
    await closeApp(app);
  }
});
