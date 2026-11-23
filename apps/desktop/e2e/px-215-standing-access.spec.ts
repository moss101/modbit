/**
 * QUAL-PX-215 / PX-E2E-215 (DR-PX-2026-11-11-021 MWS-19): standing computer access per application, on the real Electron build,
 * a real Core, the repository's real fixture actuator process and a scripted model standing in for the provider only.
 *  - Settings > Permissions shows the person's level per application as the Core holds it. Nothing is set by default (it says so),
 *    the running applications the helper reports are offered, and setting a level through the form, changing it in its row and
 *    clearing it each go through the one typed Core command: the screen reads back what the Core answers, the Core's own log has
 *    one event per change, and a refusal of the Core's (an application no approval lifts) is shown, not swallowed.
 *  - A task whose application is at view only has its click and its typing refused: each action card says the level, what the call
 *    needed, that nothing was done and where to change it, and no approval is asked for either. At never the start is refused the
 *    same way. The wording is the Core's own record (the activity's ACCESS_REFUSED), never inferred from a code alone.
 *  - The page is keyboard-operable and passes the accessibility suite in light, dark and high contrast.
 * Fail conditions, each with a retained mutation (evidence/m10/PX-215/mutation-checks-*.txt) the check is shown to catch: a level
 * that is not enforced (a click goes through at view only), a refusal without its reason on the card, a change that is not on the
 * log. With MODBIT_SHOTS_DIR set the same run writes the evidence screenshots.
 */
import { expect, test, type Page } from "@playwright/test";
import { mkdirSync, mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { accessible, closeApp, launch, makeRepo, setContentSize } from "./support/ui-harness.ts";
import { sessionOf, startAndOpen } from "./support/conversation-harness.ts";
import { approveWithPointer, call, cardOf, computerModel, computerRig, elementOf, plan, start, type Step } from "./support/computer-harness.ts";

const SHOTS = process.env.MODBIT_SHOTS_DIR ?? "";
const FORM = "org.example.fixture.form";
const DOCS = "org.example.fixture.docs";
const newRepo = () => makeRepo(mkdtempSync(join(tmpdir(), "modbit-px215-repo-")));

async function setTheme(page: Page, theme: "light" | "dark"): Promise<void> {
  await page.evaluate((t) => document.documentElement.setAttribute("data-theme", t), theme);
  await page.evaluate(() => new Promise<void>((d) => requestAnimationFrame(() => requestAnimationFrame(() => d()))));
}

/** Screenshots of the running app in both themes. */
async function shots(page: Page, name: string, before?: () => Promise<void>): Promise<void> {
  if (!SHOTS) return;
  mkdirSync(SHOTS, { recursive: true });
  for (const theme of ["light", "dark"] as const) {
    await setTheme(page, theme);
    await before?.();
    await page.waitForTimeout(400);
    await page.screenshot({ path: join(SHOTS, `${name}-${theme}.png`) });
  }
  await setTheme(page, "light");
}

/** The axe suite on the live app in light, dark and high contrast. */
async function accessibleInAllThemes(page: Page, what: string): Promise<void> {
  for (const theme of ["dark", "high-contrast", "light"]) {
    await page.evaluate((t) => (document.documentElement.dataset.theme = t), theme);
    await accessible(page, `${what} in ${theme}`);
  }
  await page.evaluate(() => delete document.documentElement.dataset.theme);
}

async function openPermissions(page: Page): Promise<void> {
  if ((await page.getByTestId("settings-nav").count()) === 0) await page.getByTestId("open-settings").click();
  await expect(page.getByTestId("settings-nav")).toBeVisible();
  await page.getByTestId("settings-nav-permissions").click();
  await expect(page.getByTestId("settings-computer-access")).toBeVisible();
}

/** Sets a level through the form, the way a person does: the application, the level, Set level. */
async function setThroughForm(page: Page, application: string, level: string): Promise<void> {
  await page.getByTestId("settings-access-application").fill(application);
  await page.getByTestId("settings-access-new-level").selectOption(level);
  await page.getByTestId("settings-access-set").click();
}

const rowOf = (page: Page, bundle: string) => page.locator(`[data-testid="settings-access-level"][data-bundle="${bundle}"]`);

/** The Core's own answer, read through the bridge. */
const coreAccess = (page: Page) => page.evaluate(() => window.modbit.computerAccess());

async function backToTasks(page: Page): Promise<void> {
  if ((await page.getByTestId("settings-nav").count()) > 0) await page.getByTestId("nav-back").click();
}

test("PX-215: Settings shows nothing set by default, sets, changes and clears a level through the Core, shows the Core's refusals and is accessible", async () => {
  test.setTimeout(240_000);
  const rig = computerRig();
  const { app, page } = await launch(rig.dataDir, { env: rig.env });
  try {
    await setContentSize(app, page, 1280, 800);
    await openPermissions(page);

    // Nothing is set by default, and the page says what that means.
    await expect(page.getByTestId("settings-access-none")).toContainText("Every application follows each task's own grant");
    await expect(page.getByTestId("settings-computer-access")).toContainText("A level only ever narrows what a task may do, and you still approve every action");
    expect((await coreAccess(page))?.entries).toEqual([]);
    // The helper reports the running applications that could be driven at all; none of the ones no approval lifts.
    const running = (await coreAccess(page))!.running.map((a) => a.bundleId);
    expect(running).toEqual(expect.arrayContaining([FORM, DOCS]));
    expect(running.some((b) => b.startsWith("com.apple.terminal") || b.startsWith("com.1password"))).toBe(false);
    await expect(page.locator("#settings-access-running option")).toHaveCount(running.length, { timeout: 30_000 });
    await shots(page, "settings-access-default");

    // Set a level through the form: the Core answers, the row appears with the level, the person is told when it applies.
    await setThroughForm(page, FORM, "view_only");
    await expect(rowOf(page, FORM)).toHaveValue("view_only", { timeout: 30_000 });
    await expect(page.getByTestId("settings-access-note")).toContainText("is now view only. This applies from the agent's next action.");
    await expect(page.getByTestId("settings-access-none")).toHaveCount(0);
    expect((await coreAccess(page))?.entries.map((e) => [e.bundleId, e.level, e.application])).toEqual([[FORM, "view_only", "Form App"]]);
    await expect(page.getByTestId("settings-access-does").first()).toContainText("can look at the application and cannot click, press or type");

    // Change it in its row, with the keyboard.
    await rowOf(page, FORM).focus();
    await page.keyboard.type("Click");
    await expect(rowOf(page, FORM)).toHaveValue("click", { timeout: 30_000 });
    expect((await coreAccess(page))?.entries[0]?.level).toBe("click");
    await expect(page.getByTestId("settings-access-does").first()).toContainText("press, click and scroll, and cannot type");
    // A second application, never.
    await setThroughForm(page, DOCS, "never");
    await expect(rowOf(page, DOCS)).toHaveValue("never", { timeout: 30_000 });
    expect((await coreAccess(page))?.entries.map((e) => `${e.bundleId}=${e.level}`).sort()).toEqual([`${DOCS}=never`, `${FORM}=click`]);
    await shots(page, "settings-access-levels");
    await accessibleInAllThemes(page, "Settings with standing access set");

    // The Core's refusals are shown: an application no approval lifts, and a malformed identifier (main refuses it before the Core).
    await setThroughForm(page, "com.apple.terminal", "full");
    await expect(page.getByTestId("settings-access-error")).toContainText("never driven", { timeout: 30_000 });
    await expect(page.getByTestId("settings-access-error")).toContainText("no level lifts that");
    expect((await coreAccess(page))?.entries.some((e) => e.bundleId === "com.apple.terminal")).toBe(false);
    await setThroughForm(page, "com example", "click");
    await expect(page.getByTestId("settings-access-error")).toContainText("no spaces and no pattern");
    await setThroughForm(page, "org.example.*", "click");
    await expect(page.getByTestId("settings-access-error")).toContainText("no spaces and no pattern");
    await shots(page, "settings-access-refused");
    await accessible(page, "Settings showing a refused level");
    // Never is allowed for such an application: narrowing is always possible.
    await setThroughForm(page, "com.apple.terminal", "never");
    await expect(rowOf(page, "com.apple.terminal")).toHaveValue("never", { timeout: 30_000 });
    await expect(page.getByTestId("settings-access-error")).toHaveCount(0);

    // Clear one: the grant alone decides again, and the page says so.
    await rowOf(page, DOCS).selectOption("");
    await expect(rowOf(page, DOCS)).toHaveCount(0, { timeout: 30_000 });
    await expect(page.getByTestId("settings-access-note")).toContainText("has no standing access now: each task's own grant decides, as before");
    await rowOf(page, "com.apple.terminal").selectOption("");
    await rowOf(page, FORM).selectOption("");
    await expect(page.getByTestId("settings-access-none")).toBeVisible({ timeout: 30_000 });
    expect((await coreAccess(page))?.entries).toEqual([]);

    // Every accepted change is one event on the Core's log, read back with who made it and what it replaced; the three refused ones
    // appended nothing.
    const changes = (await coreAccess(page))!.changes;
    expect(changes.map((c) => [c.bundleId, c.level, c.previous])).toEqual([
      [FORM, "view_only", ""],
      [FORM, "click", "view_only"],
      [DOCS, "never", ""],
      ["com.apple.terminal", "never", ""],
      [DOCS, "", "never"],
      ["com.apple.terminal", "", "never"],
      [FORM, "", "click"],
    ]);
    expect(changes.every((c) => c.by.startsWith("user:"))).toBe(true);
    await accessible(page, "Settings with nothing set");
  } finally {
    await closeApp(app);
  }
});

test("PX-215: at view only the agent's click and typing are refused on the cards with the level, what they needed and where to change it; at never the start is; the person is asked for neither", async () => {
  test.setTimeout(420_000);
  const repo = newRepo();
  const rig = computerRig();
  // A refusal that tells the agent to abandon ends its run, so each refusal is one short task. The scripted model plays the script of the task being run.
  const scripts: Step[][] = [
    // A: a click-class input at view only.
    [plan(), start(DOCS), () => call("computer.state", {}), (s) => call("computer.press", { element: elementOf(s, "Bold") })],
    // B: typing at view only.
    [plan(), start(DOCS), () => call("computer.state", {}), () => call("computer.type", { text: "Dear Ada" })],
    // C: a start at never.
    [plan(), start(FORM)],
  ];
  let which = 0;
  const model = await computerModel(Array.from({ length: 6 }, (_, i): Step => (s) => scripts[which]![i]?.(s) ?? ["Nothing further."]));
  const { app, page } = await launch(rig.dataDir, { env: { ...rig.env, MODBIT_OPENAI_BASE_URL: model.url } });
  try {
    await setContentSize(app, page, 1280, 800);
    // The person sets the levels where they set everything else: in Settings.
    await openPermissions(page);
    await setThroughForm(page, DOCS, "view_only");
    await expect(rowOf(page, DOCS)).toHaveValue("view_only", { timeout: 30_000 });
    await setThroughForm(page, FORM, "never");
    await expect(rowOf(page, FORM)).toHaveValue("never", { timeout: 30_000 });
    await backToTasks(page);

    // A: the click.
    const taskA = await startAndOpen(page, "bold the letter", repo);
    await approveWithPointer(page, "computer.start");
    const press = cardOf(page, 3);
    await expect(press).toHaveAttribute("data-access", "view_only", { timeout: 120_000 });
    await expect(press).toHaveAttribute("data-code", "ACTION_UNSAFE");
    await expect(press).toHaveAttribute("data-status", "danger");
    await expect(press.getByTestId("computer-card-title")).toHaveText("Press");
    await expect(press.getByTestId("computer-card-status")).toContainText("Refused: your standing access is view only");
    await expect(press.getByTestId("computer-card-detail")).toContainText("Your standing access for “Document App” is view only, and clicking needs click or full.");
    await expect(press.getByTestId("computer-card-detail")).toContainText("Nothing was done. You can change this in Settings, under Computer control.");
    await expect(press.getByTestId("computer-card-approval")).toHaveCount(0);
    // Looking was never refused: the read before it is an ordinary card.
    await expect(cardOf(page, 2)).toHaveAttribute("data-access", "");
    await expect(cardOf(page, 2)).toHaveAttribute("data-status", "ok");
    await shots(page, "action-card-refused-click");
    await accessibleInAllThemes(page, "an action card a standing level refused");

    // B: the typing.
    which = 1;
    const taskB = await startAndOpen(page, "type the letter", repo);
    await approveWithPointer(page, "computer.start");
    const typing = cardOf(page, 3);
    await expect(typing).toHaveAttribute("data-access", "view_only", { timeout: 120_000 });
    await expect(typing.getByTestId("computer-card-title")).toHaveText("Type");
    await expect(typing.getByTestId("computer-card-detail")).toContainText("view only, and typing needs full.");
    await expect(typing.getByTestId("computer-card-approval")).toHaveCount(0);
    await shots(page, "action-card-refused-type");

    // C: the start at never, before any approval is asked.
    which = 2;
    const taskC = await startAndOpen(page, "open the form", repo);
    const refusedStart = cardOf(page, 1);
    await expect(refusedStart).toHaveAttribute("data-access", "never", { timeout: 120_000 });
    await expect(refusedStart.getByTestId("computer-card-title")).toHaveText("Start control");
    await expect(refusedStart.getByTestId("computer-card-status")).toContainText("Refused: your standing access is never");
    await expect(refusedStart.getByTestId("computer-card-detail")).toContainText("is never, so the agent may not open a session in it. Nothing was done.");
    await expect(refusedStart.getByTestId("computer-card-approval")).toHaveCount(0);
    await expect(page.getByTestId("approval-card")).toHaveCount(0);
    await shots(page, "action-card-refused-never");
    await accessibleInAllThemes(page, "the action cards a standing level refused");

    // The Core's record behind the words, one refusal per task, each joined to its call, and no approval for any of them.
    const sid = await sessionOf(page);
    const refused = async (t: string) => (await page.evaluate((a) => window.modbit.computerCards(a.s, a.t), { s: sid, t })).cards.filter((c) => c.standingAccess);
    for (const [t, tool, level, needed] of [
      [taskA, "computer.press", "view_only", "click"],
      [taskB, "computer.type", "view_only", "keyboard"],
      [taskC, "computer.start", "never", "observe"],
    ] as const) {
      const r = await refused(t);
      expect(r.map((c) => [c.tool, c.standingAccess!.level, c.standingAccess!.needed]), t).toEqual([[tool, level, needed]]);
      expect(r[0]!.standingAccess!.reason.length).toBeGreaterThan(20);
      expect(r[0]!.approval).toBeNull();
    }
    // The levels are unchanged by the runs.
    await openPermissions(page);
    await expect(rowOf(page, DOCS)).toHaveValue("view_only");
    await expect(rowOf(page, FORM)).toHaveValue("never");
  } finally {
    await closeApp(app);
    model.server.close();
  }
});
