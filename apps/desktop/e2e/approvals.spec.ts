/**
 * REQ-PX-058 / REQ-PX-057 (QUAL-PX-058, QUAL-PX-057 desktop half): the docked
 * approval stack and the run-mode chooser against a real Core and a scripted
 * OpenAI-compatible model. The Core, its event log, the real shell effector
 * (`git remote add` writes the repository's configuration; `git clean -fd`
 * deletes a file), the preload bridge and the renderer are real; only the
 * provider is scripted. No test waits on a clock for a state: a Gate holds the
 * model until the test has set what it needs.
 */
import { expect, test, type Page } from "@playwright/test";
import { existsSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { accessible, closeApp, git, launch, makeRepo, setContentSize } from "./support/ui-harness.ts";
import { Gate, streamingModel } from "./support/stream-model.ts";
import { openConversation, routedModel, sessionOf, startAndOpen } from "./support/conversation-harness.ts";

const URL1 = "https://example.invalid/one.git";
const URL2 = "https://example.invalid/two.git";
const remoteAdd = (name: string, url: string) => ({ call: { name: "shell.exec", args: { argv: ["git", "remote", "add", name, url] } } });
const remotes = (repo: string): string[] => git(repo, "remote").split("\n").filter(Boolean);
const newRepo = () => makeRepo(mkdtempSync(join(tmpdir(), "modbit-appr-repo-")));

/** Creates and starts a task from the Fleet board without opening its conversation. */
async function createAndStart(page: Page, goal: string, repo: string): Promise<string> {
  await page.getByTestId("goal").fill(goal);
  await page.getByTestId("workspace").fill(repo);
  await page.getByTestId("run").click();
  const card = page.getByTestId("task-card").filter({ hasText: goal }).first();
  await expect(card).toBeVisible();
  const id = (await card.getAttribute("data-task-id"))!;
  await card.getByTestId("task-start").click();
  return id;
}

async function chooseMode(page: Page, mode: string, widest = false): Promise<void> {
  await page.getByTestId("runmode-button").click();
  await expect(page.getByTestId("runmode-dialog")).toBeVisible();
  await page.getByTestId(`runmode-radio-${mode}`).click();
  await expect(page.getByTestId("runmode-warning")).toBeVisible();
  if (widest) await expect(page.getByTestId("runmode-warning-widest")).toContainText("this session only");
  // The Core's acknowledgement gate: the button waits for the person to say they understand.
  await expect(page.getByTestId("runmode-warning-confirm")).toBeDisabled();
  await accessible(page, `the warning for ${mode}`);
  await page.getByTestId("runmode-understood").check();
  await page.getByTestId("runmode-warning-confirm").click();
  await expect(page.getByTestId("runmode-warning")).toHaveCount(0);
  await expect(page.getByTestId("runmode-current")).toHaveText(mode === "ALLOWLIST" ? "Allowlist" : "Run everything (this session)");
  await page.getByTestId("runmode-close").click();
  await expect(page.getByTestId("runmode-dialog")).toHaveCount(0);
}

test("PX-058: a pending effect docks a card with the exact intent and the Core's reason; a stale hash is refused; it survives a reload and a Core kill; Run executes it once", async () => {
  test.setTimeout(240_000);
  const repo = newRepo();
  const model = await streamingModel([{ parts: ["Adding a remote.", remoteAdd("origin1", URL1)] }, { parts: ["done"] }]);
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-appr-"));
  const { app, page } = await launch(dataDir, { env: { MODBIT_OPENAI_BASE_URL: model.url } });
  try {
    await setContentSize(app, page, 1280, 800);
    const taskId = await startAndOpen(page, "add the origin remote", repo);
    const card = page.getByTestId("approval-card");
    await expect(card).toBeVisible({ timeout: 90_000 });
    // The exact intent: the tool, the command line, the effect class and why the Core asked.
    await expect(page.getByTestId("approval-tool")).toHaveText("shell.exec");
    await expect(page.getByTestId("approval-command")).toHaveText(`git remote add origin1 ${URL1}`);
    await expect(page.getByTestId("approval-effect")).toContainText("protected write");
    await expect(page.getByTestId("approval-why")).toContainText("Every protected effect asks in the Ask mode");
    await expect(card).toHaveAttribute("data-ask-reason", "MODE_ASK");
    await expect(page.getByTestId("approval-count")).toContainText("1 pending");
    const hash = (await card.getAttribute("data-intent-hash"))!;
    expect(hash).toMatch(/^[0-9a-f]{64}$/);
    expect(await page.getByTestId("approval-intent-hash").textContent()).toBe(hash.slice(0, 16));
    expect(remotes(repo), "nothing ran before the decision").toEqual([]);
    await accessible(page, "the docked approval card");

    // An approval can never be resolved for a different intent than the one shown: the Core refuses the stale hash and the card stays.
    const sid = await sessionOf(page);
    const approvalId = (await card.getAttribute("data-approval-id"))!;
    const refused = await page.evaluate(([s, a]) => window.modbit.resolveApproval(s!, a!, true, "stale", "0".repeat(64)).then(() => "accepted", (e: Error) => e.message), [sid, approvalId]);
    expect(refused).toContain("INTENT_MISMATCH");
    expect(remotes(repo)).toEqual([]);
    await expect(card).toBeVisible();

    // The card persists across a renderer reload and a real kill of the Core.
    await page.reload();
    await expect(page.getByTestId("core-status")).toContainText("Core connected", { timeout: 60_000 });
    await openConversation(page, taskId);
    await expect(page.getByTestId("approval-card")).toHaveAttribute("data-intent-hash", hash, { timeout: 60_000 });
    const info = await page.evaluate(() => window.modbit.debugCoreInfo());
    process.kill(info!.pid, "SIGKILL");
    await expect(page.getByTestId("core-status")).not.toContainText(`pid ${info!.pid}`, { timeout: 90_000 });
    await expect(page.getByTestId("core-status")).toContainText("Core connected", { timeout: 90_000 });
    await expect(page.getByTestId("approval-card")).toHaveAttribute("data-intent-hash", hash, { timeout: 60_000 });
    expect(remotes(repo)).toEqual([]);

    // Run (the button) executes exactly this effect once. The restart suspended the run: resume it, and the approved effect runs.
    await page.getByTestId("approval-run").click();
    await expect(page.getByTestId("approval-last")).toContainText("Approved", { timeout: 30_000 });
    await expect(page.getByTestId("approval-card")).toHaveCount(0, { timeout: 30_000 });
    await page.getByTestId("conv-start").click();
    await expect.poll(() => remotes(repo), { timeout: 90_000 }).toEqual(["origin1"]);
    expect(git(repo, "remote", "get-url", "origin1").trim()).toBe(URL1);
    // The decision left its receipt: authorised by this approval.
    const receipts = await page.evaluate((t) => window.modbit.effectReceipts(t), taskId);
    const authorised = receipts.receipts.filter((r) => r.policyDecision === `approval:${approvalId.replace(/^(.{8})(.{4})(.{4})(.{4})(.{12})$/, "$1-$2-$3-$4-$5")}`);
    expect(authorised.length, "one protected effect, one authorising receipt").toBeGreaterThan(0);
    expect(authorised.every((r) => r.intentHash === hash), "the receipt names the intent hash the card showed").toBe(true);
  } finally {
    await closeApp(app);
    model.server.close();
  }
});

test("PX-058: three pending approvals form a deck, oldest first, each decided on its own; a bare key never decides another agent's card; Skip is a typed user decision", async () => {
  test.setTimeout(300_000);
  const repos = [newRepo(), newRepo(), newRepo()];
  const goals = ["deck one", "deck two", "deck three"];
  const model = await routedModel((goal, results) => {
    const n = goals.indexOf(goal);
    return n >= 0 && results === 0 ? [`Adding remote ${n + 1}.`, remoteAdd(`r${n + 1}`, `https://example.invalid/r${n + 1}.git`)] : ["done"];
  });
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-deck-"));
  const { app, page } = await launch(dataDir, { env: { MODBIT_OPENAI_BASE_URL: model.url } });
  try {
    await setContentSize(app, page, 1280, 800);
    const ids: string[] = [];
    for (let i = 0; i < 3; i++) ids.push(await createAndStart(page, goals[i]!, repos[i]!));
    const sid = await sessionOf(page);
    // Wait on what the Core lists, not on a clock.
    await expect.poll(() => page.evaluate((s) => window.modbit.dockApprovals(s).then((l) => l.length), sid), { timeout: 120_000 }).toBe(3);
    await openConversation(page, ids[0]!);
    await expect(page.getByTestId("approval-count")).toContainText("3 pending");
    // The open task's approval is surfaced first; the others follow, oldest first.
    await expect(page.getByTestId("approval-card")).toHaveAttribute("data-approval-id", /.+/);
    await expect(page.getByTestId("approval-foreign")).toHaveCount(0);
    await expect(page.getByTestId("approval-command")).toContainText("remote add r1");
    await page.getByTestId("approval-next").click();
    await expect(page.getByTestId("approval-count")).toContainText("showing 2 of 3");
    await expect(page.getByTestId("approval-foreign")).toContainText("deck");
    const second = (await page.getByTestId("approval-card").getAttribute("data-approval-id"))!;
    await accessible(page, "a deck of three approvals, another agent's card showing");
    // Another agent's card is not decided by a bare key: Enter does nothing, Escape does nothing.
    await page.locator("body").click({ position: { x: 5, y: 5 } });
    await page.keyboard.press("Enter");
    await page.keyboard.press("Escape");
    expect(await page.evaluate((s) => window.modbit.dockApprovals(s).then((l) => l.length), sid)).toBe(3);
    // Each decision applies to exactly one effect: Skip the second (denied by the user), the others are untouched.
    await page.getByTestId("approval-skip").click();
    await expect(page.getByTestId("approval-count")).toContainText("2 pending", { timeout: 30_000 });
    const left = await page.evaluate((s) => window.modbit.dockApprovals(s).then((l) => l.map((a) => a.approvalId)), sid);
    expect(left).not.toContain(second);
    // Approve the open task's own card with the keyboard (focus is neutral, the card is this task's).
    await page.getByTestId("approval-card").focus();
    await page.keyboard.press("Enter");
    await expect.poll(() => remotes(repos[0]!), { timeout: 90_000 }).toEqual(["r1"]);
    await expect(page.getByTestId("approval-count")).toContainText("1 pending", { timeout: 30_000 });
    // The skipped one did not run; the remaining one is still waiting; the model was told it was skipped (the task goes on).
    expect(remotes(repos[1]!)).toEqual([]);
    expect(remotes(repos[2]!)).toEqual([]);
    await expect(page.getByTestId("approval-foreign")).toBeVisible();
    await page.getByTestId("approval-run").click();
    await expect.poll(() => remotes(repos[2]!), { timeout: 90_000 }).toEqual(["r3"]);
    expect(remotes(repos[1]!)).toEqual([]);
    // Skip is a typed denial, not a tool failure, and the agent went on: the skipped call is DENIED and the task took another turn after it.
    await expect
      .poll(
        () =>
          page.evaluate(async (t) => {
            const rows = (await window.modbit.transcript(t, { density: "DETAILED" })).rows;
            const denied = rows.filter((r) => r.kind === "TOOL_CARD" && r.hints.status === "DENIED").length;
            const footers = rows.filter((r) => r.kind === "TURN_FOOTER").length;
            return { denied, moreTurns: footers >= 2 };
          }, ids[1]!),
        { timeout: 90_000 },
      )
      .toEqual({ denied: 1, moreTurns: true });
  } finally {
    await closeApp(app);
    model.server.close();
  }
});

test("PX-058: an effect that cannot be undone asks twice, and a key typed into a text field never decides; Always is not offered for it; the file is only deleted after the second confirmation", async () => {
  test.setTimeout(240_000);
  const repo = newRepo();
  writeFileSync(join(repo, "junk.txt"), "scratch\n");
  const model = await streamingModel([{ parts: ["Cleaning.", { call: { name: "shell.exec", args: { argv: ["git", "clean", "-fd"] } } }] }, { parts: ["done"] }]);
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-destructive-"));
  const { app, page } = await launch(dataDir, { env: { MODBIT_OPENAI_BASE_URL: model.url } });
  try {
    await setContentSize(app, page, 1280, 800);
    await startAndOpen(page, "clean the scratch files", repo);
    const card = page.getByTestId("approval-card");
    await expect(card).toBeVisible({ timeout: 90_000 });
    await expect(card).toHaveAttribute("data-effect-class", "Destructive");
    await expect(card).toHaveAttribute("data-ask-reason", "ALWAYS_ASK:DELETION");
    await expect(page.getByTestId("approval-classes")).toContainText("Deletions");
    await expect(page.getByTestId("approval-why")).toContainText("Deletions always ask");
    await expect(page.getByTestId("approval-always")).toHaveCount(0);
    await expect(page.getByTestId("approval-always-why")).toContainText("always asks");
    // Typing a message and pressing Enter decides nothing: the steer box keeps the key.
    await page.getByTestId("conv-steer").fill("hold on");
    await page.getByTestId("conv-steer").press("Enter");
    await expect(card).toBeVisible();
    expect(existsSync(join(repo, "junk.txt"))).toBe(true);
    // Enter from a neutral place opens the second step; nothing is deleted yet; Escape steps back; the effect is still pending.
    await page.locator("body").click({ position: { x: 5, y: 5 } });
    await page.keyboard.press("Enter");
    await expect(page.getByTestId("approval-confirm")).toBeVisible();
    await expect(page.getByTestId("approval-confirm")).toContainText("cannot be undone");
    await accessible(page, "the second confirmation of an irreversible effect");
    expect(existsSync(join(repo, "junk.txt"))).toBe(true);
    await page.keyboard.press("Escape");
    await expect(page.getByTestId("approval-confirm")).toHaveCount(0);
    expect(existsSync(join(repo, "junk.txt"))).toBe(true);
    // Enter, Enter: the second one runs it.
    await page.keyboard.press("Enter");
    await expect(page.getByTestId("approval-confirm")).toBeVisible();
    await page.keyboard.press("Enter");
    await expect.poll(() => existsSync(join(repo, "junk.txt")), { timeout: 90_000 }).toBe(false);
  } finally {
    await closeApp(app);
    model.server.close();
  }
});

test("PX-057/058: Allowlist plus Always on a card makes a durable rule through the Core; the same kind of command then runs silently and its receipt names the rule; the rule can be revoked", async () => {
  test.setTimeout(300_000);
  const repo = newRepo();
  const gate = new Gate();
  const model = await streamingModel([
    { parts: [{ wait: gate }, "First remote.", remoteAdd("one", URL1)] },
    { parts: ["Second remote.", remoteAdd("two", URL2)] },
    { parts: ["done"] },
  ]);
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-allowlist-"));
  const { app, page } = await launch(dataDir, { env: { MODBIT_OPENAI_BASE_URL: model.url } });
  try {
    await setContentSize(app, page, 1280, 800);
    const taskId = await startAndOpen(page, "add two remotes", repo);
    await expect(page.getByTestId("runmode-current")).toHaveText("Ask");
    // The mode is chosen behind the warning dialog, with the always-ask classes explained.
    await page.getByTestId("runmode-button").click();
    await expect(page.getByTestId("runmode-always-ask")).toContainText("Deletions");
    await expect(page.getByTestId("runmode-always-ask")).toContainText("Network");
    await accessible(page, "the run-mode dialog");
    await page.getByTestId("runmode-close").click();
    await chooseMode(page, "ALLOWLIST");
    gate.open();
    const card = page.getByTestId("approval-card");
    await expect(card).toBeVisible({ timeout: 90_000 });
    await expect(card).toHaveAttribute("data-ask-reason", "NOT_IN_ALLOWLIST");
    await expect(page.getByTestId("approval-why")).toContainText("No allowlist rule covers this command");
    expect(remotes(repo)).toEqual([]);
    // Always: the prefix is chosen from the command's own words; the Core makes the rule and this one effect is approved.
    await page.getByTestId("approval-prefix").selectOption({ label: "git remote add" });
    // Shift+Enter allows the prefix, from the card (focus moves off the select, which keeps its own Enter).
    await page.getByTestId("approval-title").click();
    await page.keyboard.press("Shift+Enter");
    await expect(page.getByTestId("approval-last")).toContainText("Allowed “git remote add”", { timeout: 30_000 });
    // The second command is covered by the rule: it runs with no card, and nobody was asked.
    await expect.poll(() => remotes(repo).sort(), { timeout: 120_000 }).toEqual(["one", "two"]);
    await expect(page.getByTestId("approval-card")).toHaveCount(0);
    const rules = await page.evaluate((t) => window.modbit.allowRules(t), taskId);
    expect(rules).toHaveLength(1);
    expect(rules[0]!.pattern).toEqual(["git", "remote", "add"]);
    expect(rules[0]!.state).toBe("ACTIVE");
    expect(rules[0]!.scope).toBe("REPO");
    const receipts = await page.evaluate((t) => window.modbit.effectReceipts(t), taskId);
    expect(receipts.receipts.some((r) => r.policyDecision === `allowlist:${rules[0]!.ruleId}`), "the silent run's receipt names the rule").toBe(true);
    expect(receipts.receipts.some((r) => r.policyDecision.startsWith("approval:")), "the first one was approved by the person").toBe(true);
    // The rule is listed in the manager and revocable.
    await page.getByTestId("runmode-button").click();
    await expect(page.getByTestId("rule")).toHaveCount(1);
    await expect(page.getByTestId("rule-pattern")).toHaveText("git remote add");
    await accessible(page, "the run-mode dialog with a rule");
    await page.getByTestId("rule-revoke").click();
    await expect(page.getByTestId("rules-empty")).toBeVisible({ timeout: 30_000 });
    await expect.poll(() => page.evaluate((t) => window.modbit.allowRules(t, true).then((r) => r[0]?.state), taskId)).toBe("REVOKED");
    // Making a rule by hand: the Core refuses a pattern that is a wrapper, and says why.
    await page.getByTestId("rule-pattern-input").fill("sh -c");
    await page.getByTestId("rule-add").click();
    await expect(page.getByTestId("runmode-error")).toContainText("The Core did not make the rule");
    await page.getByTestId("rule-pattern-input").fill("git status");
    await page.getByTestId("rule-add").click();
    await expect(page.getByTestId("rule")).toHaveCount(1, { timeout: 30_000 });
    await page.getByTestId("runmode-close").click();
  } finally {
    gate.open();
    await closeApp(app);
    model.server.close();
  }
});

test("PX-057: the widest preset needs its warning and still asks for the always-ask classes; everything else in the envelope runs without asking", async () => {
  test.setTimeout(300_000);
  const repo = newRepo();
  writeFileSync(join(repo, "junk.txt"), "scratch\n");
  const gate = new Gate();
  const model = await streamingModel([
    { parts: [{ wait: gate }, "Adding a remote.", remoteAdd("quiet", URL1)] },
    { parts: ["Now cleaning.", { call: { name: "shell.exec", args: { argv: ["git", "clean", "-fd"] } } }] },
    { parts: ["done"] },
  ]);
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-widest-"));
  const { app, page } = await launch(dataDir, { env: { MODBIT_OPENAI_BASE_URL: model.url } });
  try {
    await setContentSize(app, page, 1280, 800);
    const taskId = await startAndOpen(page, "add a remote then clean", repo);
    // Cancel keeps the mode; the warning names the risk; only the acknowledged choice changes it.
    await page.getByTestId("runmode-button").click();
    await page.getByTestId("runmode-radio-RUN_EVERYTHING").click();
    await expect(page.getByTestId("runmode-warning-text")).toContainText("prompt injection");
    await page.getByTestId("runmode-warning-cancel").click();
    await expect(page.getByTestId("runmode-warning")).toHaveCount(0);
    await expect(page.getByTestId("runmode-current")).toHaveText("Ask");
    await page.getByTestId("runmode-close").click();
    await chooseMode(page, "RUN_EVERYTHING", true);
    gate.open();
    // The in-envelope effect ran without a card; the deletion still asks, with the Core's reason.
    await expect.poll(() => remotes(repo), { timeout: 120_000 }).toEqual(["quiet"]);
    const card = page.getByTestId("approval-card");
    await expect(card).toBeVisible({ timeout: 120_000 });
    await expect(card).toHaveAttribute("data-ask-reason", "ALWAYS_ASK:DELETION");
    await expect(page.getByTestId("approval-mode")).toContainText("run everything");
    expect(existsSync(join(repo, "junk.txt"))).toBe(true);
    // Skip with Escape: the file stays.
    await page.locator("body").click({ position: { x: 5, y: 5 } });
    await page.keyboard.press("Escape");
    await expect(page.getByTestId("approval-last")).toContainText("Skipped", { timeout: 30_000 });
    expect(existsSync(join(repo, "junk.txt"))).toBe(true);
    const receipts = await page.evaluate((t) => window.modbit.effectReceipts(t), taskId);
    expect(receipts.receipts.some((r) => r.policyDecision === "run_mode:RUN_EVERYTHING")).toBe(true);
    // The widest preset is session-only and says so.
    await page.getByTestId("runmode-button").click();
    await expect(page.getByTestId("runmode-session-only")).toBeVisible();
    await page.getByTestId("runmode-close").click();
  } finally {
    gate.open();
    await closeApp(app);
    model.server.close();
  }
});

test("PX-057/058: the main process rejects invalid arguments before the Core sees them", async () => {
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-badargs-"));
  const { app, page } = await launch(dataDir);
  try {
    const sid = await page.evaluate(() => window.modbit.createSession());
    const tid = "0123456789abcdef0123456789abcdef";
    const H = "a".repeat(64);
    const reject = (fn: string, ...args: unknown[]) =>
      page.evaluate(([f, a]) => ((window.modbit as unknown as Record<string, (...x: unknown[]) => Promise<unknown>>)[f as string]!(...(a as unknown[]))).then(() => "accepted", (e: Error) => e.message), [fn, args] as const);
    expect(await reject("setRunMode", sid, tid, "AUTO_REVIEW", true)).toContain("BAD_ARGUMENT");
    expect(await reject("setRunMode", sid, "nope", "ASK", false)).toContain("BAD_ARGUMENT");
    expect(await reject("setRunMode", sid, tid, "ASK", "yes")).toContain("BAD_ARGUMENT");
    expect(await reject("addAllowRule", sid, tid, { pattern: [], scope: "REPO", expiresAtMs: 0, coversAlwaysAsk: false })).toContain("BAD_ARGUMENT");
    expect(await reject("addAllowRule", sid, tid, { pattern: ["git", "status"], scope: "EVERYWHERE", expiresAtMs: 0, coversAlwaysAsk: false })).toContain("BAD_ARGUMENT");
    expect(await reject("addAllowRule", sid, tid, { pattern: ["git", "status"], scope: "REPO", expiresAtMs: 5, coversAlwaysAsk: false })).toContain("BAD_ARGUMENT");
    expect(await reject("revokeAllowRule", sid, tid, "")).toContain("BAD_ARGUMENT");
    expect(await reject("allowAlways", sid, "not-an-approval", H, { pattern: ["git", "status"], scope: "REPO", expiresAtMs: 0, coversAlwaysAsk: false })).toContain("BAD_ARGUMENT");
    expect(await reject("allowAlways", sid, tid, "short", { pattern: ["git", "status"], scope: "REPO", expiresAtMs: 0, coversAlwaysAsk: false })).toContain("BAD_ARGUMENT");
    expect(await reject("restoreCheckpoint", sid, tid, { checkpointId: "x" }, {})).toContain("BAD_ARGUMENT");
    expect(await reject("restoreCheckpoint", sid, tid, {}, { keepPaths: ["../outside"] })).toContain("BAD_ARGUMENT");
    expect(await reject("restoreCheckpoint", sid, tid, {}, { expected: [{ path: "a", contentHash: "nope" }] })).toContain("BAD_ARGUMENT");
    expect(await reject("restoreCheckpoint", sid, tid, { turnOrdinal: 1, name: "x" }, {})).toContain("BAD_ARGUMENT");
    expect(await reject("forkFromTurn", sid, tid, { name: "x" })).toContain("BAD_ARGUMENT");
    expect(await reject("previewRestore", "nope", {})).toContain("BAD_ARGUMENT");
    expect(await reject("setTaskBudgets", sid, tid, { maxWallMs: "-5" })).toContain("BAD_ARGUMENT");
    expect(await reject("contextAccounting", "nope")).toContain("BAD_ARGUMENT");
    // Nothing above reached the Core: the log has no run-mode or rule record.
  } finally {
    await closeApp(app);
  }
});
