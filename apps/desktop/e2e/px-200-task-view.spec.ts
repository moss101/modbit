/**
 * QUAL-PX-200 / PX-E2E-200 (DR-PX-2026-11-11-021 MWS-05..08): the measured task view on the real Electron build, a real Core
 * and a scripted model. One task plans with steps, reads, edits, asks for a protected write (a command that then runs) and
 * then for an irreversible effect. The plan card is compared with the Core's work graph at every stage; the transcript the
 * Core serves never holds a step's title, so a card fed from transcript text could not show what is asserted. The composer
 * is one line with its controls under it, each opening what it opened before; the queue's tray has Send now and Remove; the
 * approval card has no native select, keeps the intent hash in the accessibility tree behind its one disclosure, decides by
 * keys and by mouse, and an irreversible effect asks twice and offers no Always allow. A reload restores the card and the
 * plan. With MODBIT_SHOTS_DIR set, the same run writes the evidence screenshots.
 */
import { expect, test, type Page } from "@playwright/test";
import { existsSync, mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { Gate, streamingModel } from "./support/stream-model.ts";
import { runTask, setDensity } from "./support/new-task.ts";
import { accessible, box, closeApp, git, launch, makeRepo, setContentSize } from "./support/ui-harness.ts";

const SHOTS = process.env.MODBIT_SHOTS_DIR ?? "";
const URL1 = "https://example.invalid/one.git";
const OUTCOME = "change line 2 of the notes, add the remote and clean the build output";
const STEPS = [
  { id: "read", title: "Find the line to rewrite" },
  { id: "edit", title: "Rewrite the second line", depends_on: ["read"] },
  { id: "remote", title: "Add the origin1 remote", depends_on: ["edit"] },
  { id: "clean", title: "Remove untracked scratch files", depends_on: ["remote"] },
];
const plan = (steps: unknown[], reason?: string) => ({ call: { name: "plan.update", args: { outcome: OUTCOME, expected_files: ["notes.txt"], protected_effects: ["shell.exec"], steps, ...(reason ? { reason } : {}) } } });

async function setTheme(page: Page, theme: "light" | "dark" | "high-contrast"): Promise<void> {
  await page.evaluate((t) => document.documentElement.setAttribute("data-theme", t), theme);
  await page.evaluate(() => new Promise<void>((done) => requestAnimationFrame(() => requestAnimationFrame(() => done()))));
}

/** An evidence screenshot of the window (only when asked for): the running app, never a fixture. */
async function shot(page: Page, name: string, theme: "light" | "dark" = "light", density: "comfortable" | "compact" = "comfortable"): Promise<void> {
  if (!SHOTS) return;
  mkdirSync(SHOTS, { recursive: true });
  await page.mouse.move(700, 300);
  // A theme change fades its colours: the picture is taken once they have arrived.
  await page.waitForTimeout(400);
  await page.screenshot({ path: join(SHOTS, `${name}-${theme}-${density}.png`) });
}

/** A screenshot in light and dark, and in both densities when `both` is set; the theme and density are put back. */
async function shots(page: Page, name: string, both = false): Promise<void> {
  if (!SHOTS) return;
  for (const density of both ? (["comfortable", "compact"] as const) : (["comfortable"] as const)) {
    if (both) await setDensity(page, density);
    for (const theme of ["light", "dark"] as const) {
      await setTheme(page, theme);
      await shot(page, name, theme, density);
    }
  }
  if (both) await setDensity(page, "comfortable");
  await setTheme(page, "light");
}

/** The card's steps as drawn: [id, the Core's status, the title]. */
const cardSteps = (page: Page) => page.getByTestId("plan-step").evaluateAll((els) => els.map((e) => [e.getAttribute("data-step-id"), e.getAttribute("data-status"), e.querySelector(".conv-plan-step-title")?.textContent ?? ""]));
/** The same three facts from the Core's work graph, through the bridge. */
const graphSteps = (page: Page, taskId: string) => page.evaluate((t) => window.modbit.workGraph(t).then((g) => g.steps.map((s) => [s.id, s.status, s.title])), taskId);

async function cardFollowsGraph(page: Page, taskId: string, want: Record<string, string>): Promise<void> {
  await expect.poll(async () => Object.fromEntries((await graphSteps(page, taskId)).map((s) => [s[0], s[1]])), { timeout: 60_000 }).toEqual(want);
  await expect.poll(() => cardSteps(page), { timeout: 30_000 }).toEqual(await graphSteps(page, taskId));
}

test("PX-200: the plan card follows the Core's work graph, the composer is one line with its controls under it, the approval card decides by keys and by mouse with one menu and one disclosure, an irreversible effect asks twice, and a reload restores the card and the plan", async () => {
  test.setTimeout(420_000);
  const repo = makeRepo(mkdtempSync(join(tmpdir(), "modbit-px200-repo-")));
  writeFileSync(join(repo, "junk.txt"), "scratch\n");
  const hold = new Gate();
  const model = await streamingModel([
    { parts: ["I will read the notes, change line 2, add the remote and clean up.", plan([{ ...STEPS[0], status: "ACTIVE" }, STEPS[1], STEPS[2], STEPS[3]])] },
    { parts: [{ call: { name: "fs.read", args: { path: "notes.txt" } } }] },
    { parts: [plan([{ id: "read", status: "DONE", evidence_refs: ["read:notes.txt"] }, { id: "edit", status: "ACTIVE" }], "the notes are read")] },
    { parts: [{ call: { name: "change.apply", args: { path: "notes.txt", op: "replace", content: "line 1\nline 2 changed\nline 3\n" } } }] },
    { parts: ["Line 2 is changed.", plan([{ id: "edit", status: "DONE", evidence_refs: ["change:notes.txt"] }, { id: "remote", status: "ACTIVE" }], "the edit is in")] },
    { parts: [{ wait: hold }, "Adding the remote.", { call: { name: "shell.exec", args: { argv: ["git", "remote", "add", "origin1", URL1] } } }] },
    { parts: [plan([{ id: "remote", status: "DONE", evidence_refs: ["run:git-remote-add"] }, { id: "clean", status: "ACTIVE" }], "the remote is added")] },
    { parts: ["Cleaning the build output.", { call: { name: "shell.exec", args: { argv: ["git", "clean", "-fd"] } } }] },
    { parts: ["Everything in the plan is done.", plan([{ id: "clean", status: "DONE", evidence_refs: ["run:git-clean"] }], "the build output is clean")] },
    { parts: ["All four steps are done."] },
  ]);
  const { app, page } = await launch(mkdtempSync(join(tmpdir(), "modbit-e2e-px200-")), { env: { MODBIT_OPENAI_BASE_URL: model.url } });
  try {
    await setContentSize(app, page, 1440, 900);
    await setTheme(page, "light");
    const taskId = await runTask(page, "Change line 2 of the notes and tidy the repository", repo);
    await expect(page.getByTestId("conversation")).toHaveAttribute("data-loaded", "true");

    // ---- MWS-06: the plan is one card of the work graph's steps; the model is held mid-run with two steps done.
    await cardFollowsGraph(page, taskId, { read: "DONE", edit: "DONE", remote: "ACTIVE", clean: "PENDING" });
    const card = page.getByTestId("plan-card");
    await expect(card).toHaveCount(1);
    await expect(page.getByTestId("plan-progress")).toHaveText("2 of 4 done");
    await expect(page.getByTestId("plan-outcome")).toHaveText(OUTCOME);
    await expect(page.getByTestId("plan-version")).toHaveText("revised 2 times");
    await expect(card.locator('[data-step-id="remote"]')).toHaveAttribute("aria-current", "step");
    // The card replaces the repeated plan lines: three plan versions are on the log and no step or fold says "plan".
    const coreRows = await page.evaluate((t) => window.modbit.transcript(t, { density: "DETAILED" }).then((p) => JSON.stringify(p.rows)), taskId);
    expect((coreRows.match(/"toolName":"plan\.update"/g) ?? []).length, "the Core's transcript holds the three plan versions").toBe(3);
    await expect(page.getByTestId("conversation").getByText("Updated the plan")).toHaveCount(0);
    await expect(page.getByTestId("conv-scroll").getByText(/plan\.update/)).toHaveCount(0);
    // A card fed from transcript text could not show this: the Core's transcript names no step.
    for (const s of STEPS) expect(coreRows.includes(s.title), `the transcript does not hold "${s.title}"`).toBe(false);
    // The steps that are not plans are still told from the Core's facts, one line each.
    await expect(page.getByTestId("conv-scroll")).toContainText(/Explored 1 file|notes\.txt/);

    // ---- MWS-06: the line under the last block says what runs, for how long and the tokens used.
    await expect(page.getByTestId("conv-tail")).toHaveAttribute("data-phase", "RUNNING");
    await expect(page.getByTestId("conv-tail-text")).not.toHaveText("");
    await expect(page.getByTestId("conv-tail-elapsed")).toHaveText(/^for \d+s$|^for \d+m \d\ds$/);
    const economics = await page.evaluate((t) => window.modbit.taskEconomics(t), taskId);
    const counted = Number(economics.inputTokens) + Number(economics.outputTokens);
    expect(counted, "the Core has counted tokens for the task").toBeGreaterThan(0);
    await expect(page.getByTestId("conv-tail-tokens")).toHaveText(new RegExp(`^${counted} tokens$`), { timeout: 30_000 });
    // The clock is outside the live region: a screen reader hears what runs, not every second.
    expect(await page.getByTestId("conv-tail-meta").evaluate((el) => el.closest('[aria-live], [role="status"]') === null)).toBe(true);

    // ---- MWS-07: one box holding add context, the text, then stop; under it at the right the mode, the model, the run mode and the ring.
    const line = await box(page, "composer-line");
    const input = await box(page, "composer-input");
    const stop = await box(page, "composer-stop");
    const add = await box(page, "add-context");
    expect(add.x, "add context is before the text").toBeLessThan(input.x);
    expect(stop.x, "stop is after the text").toBeGreaterThan(input.x + input.width - 1);
    for (const b of [add, stop]) expect(b.y >= line.y - 1 && b.y + b.height <= line.y + line.height + 1, "on the one line").toBe(true);
    await expect(page.getByTestId("composer-send"), "nothing typed while a turn runs: stop stands alone").toHaveCount(0);
    const composer = await box(page, "composer");
    const foot = page.getByTestId("composer-foot");
    for (const id of ["mode-chip", "model-chip", "runmode-button", "context-ring"]) {
      const b = (await foot.getByTestId(id).boundingBox())!;
      expect(b.y, `${id} is under the box`).toBeGreaterThanOrEqual(composer.y + composer.height - 1);
      expect(b.x, `${id} is in the right half`).toBeGreaterThan(composer.x + composer.width / 2);
    }
    // Each control opens the menu it opened before.
    await page.getByTestId("model-chip").click();
    await expect(page.getByTestId("model-picker")).toBeVisible();
    const picker = (await page.getByTestId("model-picker").boundingBox())!;
    expect(picker.y >= 0 && picker.x >= 0 && picker.x + picker.width <= 1440, "the picker opens inside the window").toBe(true);
    await page.keyboard.press("Escape");
    await expect(page.getByTestId("model-picker")).toHaveCount(0);
    await page.getByTestId("mode-chip-button").click();
    await expect(page.getByTestId("menu-item-plan")).toBeVisible();
    await page.keyboard.press("Escape");
    await page.getByTestId("context-ring").click();
    await expect(page.getByTestId("context-tray")).toBeVisible();
    // A click that lands while the ring re-renders can be lost (wave4.spec.ts notes it from PR 115; seen here on the Windows
    // runner, CI run 43198251756): closing is retried only while the tray is still open, so a tray that never closes still fails.
    await expect(async () => {
      if ((await page.getByTestId("context-tray").count()) > 0) await page.getByTestId("context-ring").click();
      await expect(page.getByTestId("context-tray")).toHaveCount(0, { timeout: 2_000 });
    }).toPass({ timeout: 20_000 });
    await page.getByTestId("runmode-button").click();
    await expect(page.getByTestId("runmode-dialog")).toBeVisible();
    await page.getByTestId("runmode-close").click();
    await page.getByTestId("add-context").click();
    await expect(page.getByTestId("menu-item-side")).toBeVisible();
    await page.keyboard.press("Escape");
    // The box grows with its text and goes back to one line.
    const oneLine = (await box(page, "composer")).height;
    await page.getByTestId("composer-input").fill("one\ntwo\nthree");
    expect((await box(page, "composer")).height, "three lines are taller than one").toBeGreaterThan(oneLine + 30);
    await expect(page.getByTestId("composer-send"), "with text there is something to queue").toBeVisible();
    await page.getByTestId("composer-input").fill("");
    expect(Math.abs((await box(page, "composer")).height - oneLine)).toBeLessThanOrEqual(1);
    await accessible(page, "the running task with its plan card", '[data-testid="shell"]');
    await shots(page, "running-plan-card-status-line", true);

    // ---- MWS-07: a queued message sits in a tray above the box with Send now and Remove.
    await page.getByTestId("composer-input").fill("Also check that the notes end with a newline");
    await page.getByTestId("composer-input").press("Enter");
    const queue = page.getByTestId("queue-tray");
    await expect(queue).toBeVisible();
    await expect(page.getByTestId("queue-header")).toHaveText("1 queued");
    await expect(queue.getByTestId("queue-send-now")).toBeVisible();
    await expect(queue.getByRole("button", { name: "Remove queued message 1" })).toBeVisible();
    // The education about queuing shows the first time; it is acknowledged so the tray is the queue alone.
    if (await page.getByTestId("education-keep").isVisible()) await page.getByTestId("education-keep").click();
    const tray = (await queue.boundingBox())!;
    expect(tray.y + tray.height, "the tray is above the box").toBeLessThanOrEqual((await box(page, "composer")).y + 1);
    await accessible(page, "the queue tray", '[data-testid="shell"]');
    await shots(page, "queued-message-tray");
    await queue.getByTestId("queue-delete").click();
    await expect(queue).toHaveCount(0);
    expect(await page.evaluate((t) => window.modbit.composerQueue(t).then((q) => q.items.filter((i) => i.state === "QUEUED").length), taskId), "Remove took it off the Core's queue").toBe(0);

    // ---- MWS-05: the title opens the task's menu; one toggle per pane, each with its state; then More.
    const bar = page.getByTestId("top-bar");
    await expect(bar.getByTestId("top-bar-title")).toContainText("Change line 2 of the notes");
    await bar.getByTestId("task-menu").click();
    await expect(page.getByTestId("menu-item-task-pin")).toHaveText("Pin");
    await expect(page.getByTestId("menu-item-task-archive")).toBeVisible();
    await expect(page.getByRole("menuitem", { name: /split/i }), "split view is a later row").toHaveCount(0);
    await page.getByTestId("pane-toggle-changes").hover();
    await shots(page, "title-menu-and-pane-toggles");
    await page.getByTestId("menu-item-task-pin").click();
    await expect(page.locator(`[data-testid="agent-row"][data-task-id="${taskId}"]`)).toHaveAttribute("data-pinned", "true");
    await bar.getByTestId("task-menu").click();
    await expect(page.getByTestId("menu-item-task-pin")).toHaveText("Unpin");
    await page.getByTestId("menu-item-task-pin").click();
    await expect(page.locator(`[data-testid="agent-row"][data-task-id="${taskId}"]`)).toHaveAttribute("data-pinned", "false");
    const toggles = await bar.locator('[data-testid^="pane-toggle-"]').evaluateAll((els) => els.map((e) => [e.getAttribute("data-testid"), e.getAttribute("aria-pressed")]));
    expect(toggles).toEqual([["pane-toggle-changes", "false"], ["pane-toggle-browser", "false"], ["pane-toggle-files", "false"], ["pane-toggle-evidence", "false"]]);
    const lastToggle = (await bar.getByTestId("pane-toggle-evidence").boundingBox())!;
    const more = (await bar.getByTestId("task-details-toggle").boundingBox())!;
    expect(more.x, "More comes after the toggles").toBeGreaterThanOrEqual(lastToggle.x + lastToggle.width - 1);
    await bar.getByTestId("pane-toggle-files").click();
    await expect(page.getByTestId("tab-files")).toHaveAttribute("aria-selected", "true");
    await expect(bar.getByTestId("pane-toggle-files")).toHaveAttribute("aria-pressed", "true");
    await expect(bar.getByTestId("pane-toggle-changes")).toHaveAttribute("aria-pressed", "false");
    await bar.getByTestId("pane-toggle-changes").click();
    await expect(page.getByTestId("tab-changes")).toHaveAttribute("aria-selected", "true");
    await expect(bar.getByTestId("pane-toggle-files")).toHaveAttribute("aria-pressed", "false");
    await shots(page, "pane-toggle-open-on-changes");
    await bar.getByTestId("pane-toggle-changes").click();
    await expect(page.getByTestId("region-apps")).toHaveCount(0);

    // ---- MWS-08: the protected write asks on one docked card.
    hold.open();
    const appr = page.getByTestId("approval-card");
    await expect(appr).toBeVisible({ timeout: 90_000 });
    await expect(page.getByTestId("approval-title")).toContainText("Run a command?");
    await expect(page.getByTestId("approval-title"), "the tool's own name is not the headline").not.toContainText("shell.exec");
    await expect(page.getByTestId("approval-command")).toHaveText(`git remote add origin1 ${URL1}`);
    await expect(page.getByTestId("approval-why")).toContainText("No allowlist rule covers this command");
    await expect(appr.locator("select"), "no native select in the card").toHaveCount(0);
    await expect(page.getByTestId("conv-approve"), "no decision control in the transcript").toHaveCount(0);
    await expect(page.getByTestId("conv-approval-decision")).toHaveCount(0);
    // The intent hash and the tool sit behind the one disclosure and are in the accessibility tree while it is closed.
    const intent = (await appr.getAttribute("data-intent-hash"))!;
    expect(intent).toMatch(/^[0-9a-f]{64}$/);
    await expect(page.getByTestId("approval-args-toggle")).toHaveAttribute("aria-expanded", "false");
    await expect(appr.locator("[aria-expanded]:not([aria-haspopup])"), "one disclosure").toHaveCount(1);
    const tree = await appr.ariaSnapshot();
    expect(tree, "the intent hash is in the accessibility tree while the disclosure is closed").toContain(intent.slice(0, 16));
    expect(tree).toContain("shell.exec");
    expect(await page.getByTestId("approval-exact").evaluate((el) => { const r = el.getBoundingClientRect(); return r.width <= 1 && r.height <= 1 && getComputedStyle(el).overflow === "hidden"; }), "and clipped from sight until asked for").toBe(true);
    // Its state and the plan: the Core's step is still active, said as waiting for the person, not spinning.
    await expect(card.locator('[data-step-id="remote"]')).toContainText("In progress, waiting for you");
    await expect(page.getByTestId("conv-tail")).toContainText("Waiting for approval");
    await expect(page.getByTestId("task-notice"), "the card says it; no notice stands above the composer").toHaveAttribute("data-shown", "false");
    await accessible(page, "the docked approval", '[data-testid="shell"]');
    await setTheme(page, "dark");
    await accessible(page, "the docked approval in dark", '[data-testid="shell"]');
    await setTheme(page, "high-contrast");
    await accessible(page, "the docked approval in high contrast", '[data-testid="shell"]');
    await setTheme(page, "light");

    // Always allow opens one menu holding the prefix and the scope.
    await page.getByTestId("approval-always").click();
    const menu = page.getByTestId("approval-always-menu");
    await expect(menu).toBeVisible();
    await expect(menu.getByTestId("approval-prefix-option")).toHaveCount(5);
    await expect(menu.locator('[data-testid="approval-prefix-option"]:checked')).toHaveAttribute("data-prefix", `git remote add origin1 ${URL1}`);
    await expect(menu.locator('[data-testid="approval-scope-option"]:checked')).toHaveAttribute("data-scope", "REPO");
    const m = (await menu.boundingBox())!;
    const c = (await appr.boundingBox())!;
    expect(m.y, "the menu is drawn above the card's foot and is not clipped by the card").toBeLessThan(c.y + c.height);
    expect(await menu.evaluate((el) => { const r = el.getBoundingClientRect(); const at = document.elementFromPoint(r.left + r.width / 2, r.top + 12); return !!at && el.contains(at); }), "the menu is on top").toBe(true);
    await accessible(page, "the Always allow menu", '[data-testid="shell"]');
    await shots(page, "approval-protected-write-always-menu");
    if (SHOTS) {
      // Compact, for the evidence: changing the density leaves the conversation and comes back, so the menu is opened again.
      await setDensity(page, "compact");
      await page.getByTestId("approval-always").click();
      await expect(menu).toBeVisible();
      for (const theme of ["light", "dark"] as const) {
        await setTheme(page, theme);
        await shot(page, "approval-protected-write-always-menu", theme, "compact");
      }
      await setTheme(page, "light");
      await setDensity(page, "comfortable");
      await page.getByTestId("approval-always").click();
      await expect(menu).toBeVisible();
    }
    // Escape closes the menu and nothing else: the card is not skipped.
    await menu.locator('[data-testid="approval-prefix-option"][data-prefix="git remote add"]').check();
    await page.keyboard.press("Escape");
    await expect(menu).toHaveCount(0);
    await expect(appr).toBeVisible();
    await expect(page.getByTestId("approval-always")).toBeFocused();
    expect(git(repo, "remote").trim(), "nothing ran").toBe("");

    // The disclosure, by keyboard: open, read, close.
    await page.getByTestId("approval-args-toggle").focus();
    await page.keyboard.press("Enter");
    await expect(page.getByTestId("approval-args-toggle")).toHaveAttribute("aria-expanded", "true");
    await expect(appr, "Enter on the disclosure did not decide").toBeVisible();
    await expect(page.getByTestId("approval-intent-hash")).toBeVisible();
    await expect(page.getByTestId("approval-intent-hash")).toHaveText(intent.slice(0, 16));
    await expect(page.getByTestId("approval-args")).toContainText('"origin1"');
    await expect(page.getByTestId("approval-tool")).toHaveText("shell.exec");
    await accessible(page, "the approval with its disclosure open", '[data-testid="shell"]');
    await shots(page, "approval-disclosure-open");
    await page.keyboard.press("Enter");
    await expect(page.getByTestId("approval-args-toggle")).toHaveAttribute("aria-expanded", "false");

    // A reload mid-approval restores the card and the plan from the Core.
    await page.reload();
    await expect(page.getByTestId("core-status")).toContainText("Core connected", { timeout: 60_000 });
    await page.locator(`[data-testid="agent-row"][data-task-id="${taskId}"]`).getByTestId("agent-open").click();
    await expect(page.getByTestId("approval-card")).toHaveAttribute("data-intent-hash", intent, { timeout: 60_000 });
    await cardFollowsGraph(page, taskId, { read: "DONE", edit: "DONE", remote: "ACTIVE", clean: "PENDING" });
    await expect(page.getByTestId("plan-outcome")).toHaveText(OUTCOME);

    // Decided by keys: Enter from a neutral place allows it once, and the command runs.
    await page.getByTestId("approval-title").click();
    await page.keyboard.press("Enter");
    await expect(page.getByTestId("approval-last")).toContainText("Approved", { timeout: 30_000 });
    const resume = page.getByTestId("conv-start");
    const second = page.locator('[data-testid="approval-card"][data-effect-class="Destructive"]');
    await expect(second.or(resume)).toBeVisible({ timeout: 90_000 });
    if (await resume.isVisible()) await resume.click();

    // ---- The irreversible effect: asks twice, offers no Always allow, and the graph moved on with the command run.
    await expect(second).toBeVisible({ timeout: 90_000 });
    expect(git(repo, "remote").trim(), "the allowed command ran").toBe("origin1");
    await cardFollowsGraph(page, taskId, { read: "DONE", edit: "DONE", remote: "DONE", clean: "ACTIVE" });
    await expect(page.getByTestId("plan-progress")).toHaveText("3 of 4 done");
    await expect(page.getByTestId("approval-command")).toHaveText("git clean -fd");
    await expect(page.getByTestId("approval-always")).toHaveCount(0);
    await expect(page.getByTestId("approval-always-why")).toContainText("always asks");
    await expect(second.locator("select")).toHaveCount(0);
    await accessible(page, "the destructive approval", '[data-testid="shell"]');
    await shots(page, "approval-destructive", true);
    // By mouse: Allow once opens the second question; Escape steps back and skips nothing; the file is still there.
    await page.getByTestId("approval-run").click();
    await expect(page.getByTestId("approval-confirm")).toContainText("cannot be undone");
    expect(existsSync(join(repo, "junk.txt"))).toBe(true);
    await shots(page, "approval-destructive-asks-twice");
    await page.getByTestId("approval-title").click();
    await page.keyboard.press("Escape");
    await expect(page.getByTestId("approval-confirm")).toHaveCount(0);
    await expect(second).toBeVisible();
    expect(existsSync(join(repo, "junk.txt"))).toBe(true);
    await page.getByTestId("approval-run").click();
    await page.getByTestId("approval-confirm-run").click();
    await expect.poll(() => existsSync(join(repo, "junk.txt")), { timeout: 90_000 }).toBe(false);
    if (await resume.isVisible().catch(() => false)) await resume.click();

    // ---- The plan ends with every step done, as the graph says; a reload shows the same card.
    await cardFollowsGraph(page, taskId, { read: "DONE", edit: "DONE", remote: "DONE", clean: "DONE" });
    await expect(page.getByTestId("plan-progress")).toHaveText("4 of 4 done");
    await expect(page.getByTestId("plan-card")).toHaveCount(1);
    await expect(page.getByTestId("conversation").getByText("Updated the plan")).toHaveCount(0);
    await shots(page, "finished-plan-all-done");
  } finally {
    await closeApp(app);
    model.server.close();
  }
});

test("PX-200: a plan that names no steps keeps its plan line and draws no card; a denied command is never told as run", async () => {
  test.setTimeout(180_000);
  const repo = makeRepo(mkdtempSync(join(tmpdir(), "modbit-px200-nosteps-")));
  const model = await streamingModel([
    { parts: ["Planning.", { call: { name: "plan.update", args: { outcome: "add the remote", expected_files: [], protected_effects: ["shell.exec"] } } }] },
    { parts: ["Adding the remote.", { call: { name: "shell.exec", args: { argv: ["git", "remote", "add", "origin1", URL1] } } }] },
    { parts: ["Left the remotes as they were."] },
  ]);
  const { app, page } = await launch(mkdtempSync(join(tmpdir(), "modbit-e2e-px200-nosteps-")), { env: { MODBIT_OPENAI_BASE_URL: model.url } });
  try {
    await setContentSize(app, page, 1280, 800);
    const taskId = await runTask(page, "Add the remote", repo);
    await expect(page.getByTestId("approval-card")).toBeVisible({ timeout: 90_000 });
    // The audit's finding, shown on the real Core: this plan reached the log and the transcript but not the work graph.
    expect(await page.evaluate((t) => window.modbit.workGraph(t).then((g) => g.steps.length), taskId)).toBe(0);
    const recorded = await page.evaluate((t) => window.modbit.plan(t), taskId);
    expect([recorded.currentVersion, recorded.outcome, recorded.stepsInVersion]).toEqual([1, "add the remote", 0]);
    await expect(page.getByTestId("plan-card")).toHaveCount(0);
    await expect(page.getByTestId("conv-scroll")).toContainText(/Updated the plan|add the remote/);
    // Skip by key: the command did not run and no step says it did.
    await page.getByTestId("approval-title").click();
    await page.keyboard.press("Escape");
    await expect(page.getByTestId("approval-last")).toContainText("Skipped", { timeout: 30_000 });
    await expect(page.getByTestId("approval-card")).toHaveCount(0);
    expect(git(repo, "remote").trim()).toBe("");
    await expect(page.getByTestId("conv-scroll").getByText(/Ran git remote add/)).toHaveCount(0);
    await expect(page.getByTestId("conv-scroll")).toContainText(/not run|Denied/);
  } finally {
    await closeApp(app);
    model.server.close();
  }
});
