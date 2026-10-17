/**
 * REQ-PX-074 (QUAL-PX-074): native computer control's desktop surface against a
 * real Core, the repository's real fixture actuator process and the real Electron
 * app. The Core, its event log, the actuator process and its authenticated RPC,
 * the preload bridge and the renderer are real; only the model is scripted. A
 * real operating-system permission flow cannot be driven here: the fixture
 * actuator reports permissions read from a file the test changes, the way the
 * macOS helper reports a grant made in System Settings.
 */
import { expect, test, type Page } from "@playwright/test";
import { existsSync, mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { accessible, closeApp, launch, makeRepo, setContentSize, MOD } from "./support/ui-harness.ts";
import { sessionOf, startAndOpen } from "./support/conversation-harness.ts";
import { Gate } from "./support/stream-model.ts";
import { FORM, LOGIN, approveWithKeyboard, approveWithPointer, call, cardOf, computerModel, computerRig, elementOf, finish, plan, say, setPermissions, start } from "./support/computer-harness.ts";

const newRepo = () => makeRepo(mkdtempSync(join(tmpdir(), "modbit-cc-repo-")));

/** The axe suite on the live app in light, dark and high contrast (the theme is the document's, as the Settings menu sets it). */
async function accessibleInAllThemes(page: Page, what: string): Promise<void> {
  for (const theme of ["dark", "high-contrast", "light"]) {
    await page.evaluate(async (t) => {
      document.documentElement.dataset.theme = t;
      // A colour transition off screen does not advance until it is on screen: bring each control into view so the theme has applied.
      for (const el of document.querySelectorAll("button")) {
        el.scrollIntoView({ block: "nearest" });
        await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));
      }
    }, theme);
    await accessible(page, `${what} in ${theme}`);
  }
  await page.evaluate(() => delete document.documentElement.dataset.theme);
}

async function openPermissions(page: Page): Promise<void> {
  await page.keyboard.press(`${MOD}+k`);
  await page.getByTestId("palette-input").fill("Computer control permissions");
  await page.keyboard.press("Enter");
  await expect(page.getByTestId("computer-permissions")).toBeVisible();
}

test("PX-074: the permission window shows checking, allow and ready from the Core's actuator; the deep link only opens a pane; a grant is noticed; it is keyboard-operable and accessible", async () => {
  test.setTimeout(180_000);
  const rig = computerRig({ permissions: { accessibility: false, screen_recording: false } });
  const { app, page } = await launch(rig.dataDir, { env: rig.env });
  try {
    await setContentSize(app, page, 1280, 800);
    await openPermissions(page);
    // Both capabilities are asked for in plain language, and both are off: the person has a step to take.
    await expect(page.getByTestId("perm-accessibility")).toContainText("Access to application interfaces");
    await expect(page.getByTestId("perm-screen_recording")).toContainText("Use of screenshots as context");
    await expect(page.getByTestId("perm-accessibility")).toHaveAttribute("data-status", "allow");
    await expect(page.getByTestId("perm-screen_recording")).toHaveAttribute("data-status", "allow");
    await expect(page.getByTestId("perm-accessibility-steps")).toContainText("Privacy & Security");
    await accessibleInAllThemes(page, "the permission window with both permissions off");

    // The deep link opens the right pane and changes nothing: the Core still reports the permission off.
    await page.getByTestId("perm-open-accessibility").focus();
    await page.keyboard.press("Enter");
    await expect(page.getByTestId("perm-accessibility-note")).toBeVisible();
    const opened = await page.evaluate(() => window.modbit.debugOpenedSettings());
    expect(opened.map((o) => o.pane)).toEqual(["accessibility"]);
    expect(opened[0]!.url).toBe("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility");
    expect((await page.evaluate(() => window.modbit.computerPresence()))?.accessibility).toBe("DENIED");
    await page.getByTestId("perm-open-screen_recording").click();
    expect((await page.evaluate(() => window.modbit.debugOpenedSettings())).map((o) => o.url)).toEqual([
      "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility",
      "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture",
    ]);

    // A grant made in the operating system reaches the Core as PermissionChanged and the row turns ready.
    setPermissions(rig, { accessibility: true, screen_recording: false });
    await expect(page.getByTestId("perm-accessibility")).toHaveAttribute("data-status", "ready", { timeout: 30_000 });
    await expect(page.getByTestId("perm-screen_recording")).toHaveAttribute("data-status", "allow");
    await expect(page.getByTestId("perm-allready")).toHaveCount(0);
    setPermissions(rig, { accessibility: true, screen_recording: true });
    await expect(page.getByTestId("perm-screen_recording")).toHaveAttribute("data-status", "ready", { timeout: 30_000 });
    await expect(page.getByTestId("perm-allready")).toBeVisible();
    await accessible(page, "the permission window with both permissions ready");
    // Keyboard only: Check again, then Done, with the keys.
    await page.getByTestId("perm-check").focus();
    await page.keyboard.press("Enter");
    await page.getByTestId("perm-done").focus();
    await page.keyboard.press("Enter");
    await expect(page.getByTestId("computer-permissions")).toHaveCount(0);
    // A permission that goes off again is shown as it is, not as it was.
    setPermissions(rig, { accessibility: false, screen_recording: true });
    await expect.poll(() => page.evaluate(() => window.modbit.computerPresence().then((p) => p?.accessibility)), { timeout: 30_000 }).toBe("DENIED");
  } finally {
    await closeApp(app);
  }
});

test("PX-074: Stop halts an in-flight input, is sticky for the turn, and presence and the cards say so", async () => {
  test.setTimeout(300_000);
  const repo = newRepo();
  const rig = computerRig({ permissions: { accessibility: true, screen_recording: true } });
  // The model holds its next turn until the person has pressed Stop and the test has looked at what the surface says.
  const afterStop = new Gate();
  const model = await computerModel([
    plan(),
    start(FORM),
    () => call("computer.state", {}),
    (s) => say("Pressing the button that hangs.", ...call("computer.press", { element: elementOf(s, "Hang after delivery") })),
    () => [{ wait: afterStop }, ...call("computer.state", {})],
  ]);
  const { app, page } = await launch(rig.dataDir, { env: { ...rig.env, MODBIT_OPENAI_BASE_URL: model.url } });
  try {
    await setContentSize(app, page, 1280, 800);
    const taskId = await startAndOpen(page, "operate the form app", repo);
    const sid = await sessionOf(page);

    // The observation grant: one application, by identity, never allowlistable.
    await approveWithKeyboard(page, "computer.start");
    // The press is its own approval: the exact identity, window, element, action and arguments.
    await expect(page.getByTestId("approval-tool")).toHaveText("computer.press", { timeout: 90_000 });
    const card = page.getByTestId("approval-card");
    await expect(page.getByTestId("computer-approval-bundle")).toHaveText(FORM);
    await expect(page.getByTestId("computer-approval-app")).toContainText("Form App");
    await expect(page.getByTestId("computer-approval-window")).toContainText("Order form");
    await expect(page.getByTestId("computer-approval-target")).toContainText("Hang after delivery");
    await expect(page.getByTestId("computer-approval-action")).toContainText("press");
    await expect(page.getByTestId("computer-approval-scope")).toContainText("This application only");
    await expect(page.getByTestId("computer-approval-expiry")).toContainText("then it can no longer be approved");
    await expect(card).toHaveAttribute("data-intent-hash", /^[0-9a-f]{64}$/);
    // No Always, no allowlist, no prefix, no run-mode path on this class of card.
    for (const id of ["approval-always", "approval-prefix", "approval-scope", "approval-change-mode", "approval-always-why"]) await expect(page.getByTestId(id), id).toHaveCount(0);
    await expect(page.getByTestId("computer-approval-rule")).toContainText("No “always allow”");
    await accessibleInAllThemes(page, "the approval card of a native input");
    await approveWithPointer(page, "computer.press");

    // The input is in flight inside the actuator (the actuator says so): presence is on screen with the application and the task.
    await expect.poll(() => existsSync(rig.hangMarker), { timeout: 60_000 }).toBe(true);
    const bar = page.getByTestId("computer-presence");
    await expect(bar).toBeVisible();
    await expect(page.getByTestId("computer-presence-entry")).toHaveAttribute("data-state", "CONTROLLING");
    await expect(page.getByTestId("computer-presence-entry")).toHaveAttribute("data-task-id", taskId);
    await expect(page.getByTestId("computer-presence-text")).toContainText("Form App");
    await expect(page.getByTestId("computer-presence-text")).toContainText("operate the form app");
    await expect(page.getByTestId("computer-live")).toContainText("Computer control acquired");
    await accessibleInAllThemes(page, "presence while the agent holds the machine");

    // Stop, from the presence control: the Core stops the actuator at once.
    const t0 = Date.now();
    await page.getByTestId("computer-stop").click();
    await expect(page.getByTestId("computer-presence-entry")).toHaveAttribute("data-state", "USER_ABORTED", { timeout: 15_000 });
    expect(Date.now() - t0, "the stop did not wait out the 60 s hang").toBeLessThan(20_000);
    await expect(page.getByTestId("computer-presence-entry")).toContainText("Stopped by you");
    await expect(page.getByTestId("computer-presence-text")).toContainText("You pressed Stop");
    await expect(page.getByTestId("computer-live")).toContainText("stopped by you");
    await expect(page.getByTestId("computer-stop")).toHaveCount(0);
    await accessible(page, "presence after Stop");
    // Sticky for the turn: the Core refuses the model's next call with the typed state (the abandon escalation ends the run), before any approval is asked.
    afterStop.open();
    await expect(cardOf(page, 4)).toHaveAttribute("data-code", "USER_ABORTED", { timeout: 90_000 });
    await expect(cardOf(page, 4).getByTestId("computer-card-status")).toContainText("Stopped by you");
    // The in-flight press: its outcome is unknown (it may have happened), said as such.
    await expect(cardOf(page, 3)).toHaveAttribute("data-state", "UNKNOWN_OUTCOME");
    await expect(cardOf(page, 3).getByTestId("computer-card-status")).toContainText("Outcome unknown");
    // The turn is over, so the stop is forgotten with it; what stands is the Core's other truth: the press may have happened.
    await expect(page.getByTestId("computer-presence-entry")).toHaveAttribute("data-state", "LATCHED", { timeout: 60_000 });
    await expect(cardOf(page, 3)).toHaveAttribute("data-latched", "true");
    expect(sid).toBeTruthy();
    await accessibleInAllThemes(page, "presence with an unresolved latch and the action cards");
    // The person looks at the application and resolves it; the surface returns to the Core's truth: nothing is held.
    await cardOf(page, 3).getByTestId("computer-latch-resolve").click();
    await expect(cardOf(page, 3).getByTestId("computer-latch-note")).toContainText("Resolved");
    await expect(page.getByTestId("computer-presence")).toHaveCount(0);
  } finally {
    await closeApp(app);
    model.server.close();
  }
});

test("PX-074: action cards for success, a typed refusal and an unknown outcome; the latch is resolved by the person and the surface returns to the Core's truth", async () => {
  test.setTimeout(300_000);
  const repo = newRepo();
  const rig = computerRig();
  let old = "";
  const model = await computerModel([
    plan(),
    start(FORM),
    () => call("computer.state", {}),
    (s) => {
      old = elementOf(s, "Submit");
      return call("computer.set_value", { element: elementOf(s, "Name"), value: "Ada Lovelace" });
    },
    () => call("computer.state", {}),
    // An element id from the look before the last one: the Core refuses it and nothing is done.
    () => call("computer.press", { element: old }),
    // The latest look is current: this input is applied, and the actuator dies before it answers.
    (s) => call("computer.press", { element: elementOf(s, "Crash after delivery") }),
    finish(),
  ]);
  const { app, page } = await launch(rig.dataDir, { env: { ...rig.env, MODBIT_OPENAI_BASE_URL: model.url } });
  try {
    await setContentSize(app, page, 1280, 800);
    const taskId = await startAndOpen(page, "fill the form", repo);
    await approveWithPointer(page, "computer.start");
    await approveWithPointer(page, "computer.set_value");

    // #1 start control, #2 read the interface, #3 a typed value: done, approved, through the interface, and the change was seen.
    await expect(cardOf(page, 3)).toHaveAttribute("data-status", "ok", { timeout: 90_000 });
    await expect(cardOf(page, 1).getByTestId("computer-card-title")).toHaveText("Start control");
    await expect(cardOf(page, 1).getByTestId("computer-card-approval")).toContainText("your approval");
    await expect(cardOf(page, 2).getByTestId("computer-card-observed")).toContainText("elements");
    await expect(cardOf(page, 3).getByTestId("computer-card-title")).toHaveText("Set a value");
    await expect(cardOf(page, 3).getByTestId("computer-card-target")).toContainText("Name");
    await expect(cardOf(page, 3).getByTestId("computer-card-args")).toContainText("Ada Lovelace");
    await expect(cardOf(page, 3).getByTestId("computer-card-how")).toContainText("through the application's interface");
    await expect(cardOf(page, 3).getByTestId("computer-card-bundle")).toContainText(FORM);
    await expect(cardOf(page, 3).getByTestId("computer-card-approval")).toHaveAttribute("data-intent-hash", /^[0-9a-f]{64}$/);

    // #5 a stale element: refused with its typed code before any approval, in plain words, and nothing was done.
    await expect(cardOf(page, 5)).toHaveAttribute("data-code", "TARGET_STALE", { timeout: 90_000 });
    await expect(cardOf(page, 5).getByTestId("computer-card-status")).toContainText("target stale");
    await expect(cardOf(page, 5).getByTestId("computer-card-detail")).toContainText("Nothing was done");
    await expect(cardOf(page, 5).getByTestId("computer-card-approval")).toHaveCount(0);

    // #6 an input whose outcome is unknown (the actuator died after applying it): the latch, said so, with the person's way out.
    await approveWithPointer(page, "computer.press");
    await expect(cardOf(page, 6)).toHaveAttribute("data-latched", "true", { timeout: 90_000 });
    await expect(cardOf(page, 6).getByTestId("computer-card-status")).toContainText("Outcome unknown");
    await expect(cardOf(page, 6).getByTestId("computer-card-detail")).toContainText("Nothing more is sent");
    await expect(page.getByTestId("computer-presence-entry")).toHaveAttribute("data-state", "LATCHED");
    await accessible(page, "the action cards: done, refused and unknown outcome");
    await cardOf(page, 6).getByTestId("computer-latch-resolve").focus();
    await page.keyboard.press("Enter");
    await expect(cardOf(page, 6).getByTestId("computer-latch-note")).toContainText("Resolved");
    await expect(page.getByTestId("computer-presence")).toHaveCount(0);
    await expect(cardOf(page, 6)).toHaveAttribute("data-latched", "false");
    // The Core agrees: no latch stands.
    expect(await page.evaluate((t) => window.modbit.computerCards(t.s, t.t).then((r) => r.cards.some((c) => c.latched)), { s: await sessionOf(page), t: taskId })).toBe(false);
  } finally {
    await closeApp(app);
    model.server.close();
  }
});

test("PX-074: a screenshot card shows the masked frame through the artifact path; secure fields are blacked out and counted; a frame the task did not take cannot be read", async () => {
  test.setTimeout(240_000);
  const repo = newRepo();
  const rig = computerRig();
  const model = await computerModel([plan(), start(LOGIN), () => call("computer.state", {}), () => call("computer.screenshot", {})]);
  const { app, page } = await launch(rig.dataDir, { env: { ...rig.env, MODBIT_OPENAI_BASE_URL: model.url } });
  try {
    await setContentSize(app, page, 1280, 800);
    const taskId = await startAndOpen(page, "look at the login window", repo);
    await approveWithPointer(page, "computer.start");
    const shot = cardOf(page, 3);
    await expect(shot.getByTestId("computer-frame-img")).toBeVisible({ timeout: 90_000 });
    await expect.poll(() => shot.getByTestId("computer-frame-img").evaluate((i: HTMLImageElement) => i.complete && i.naturalWidth > 0)).toBe(true);
    await expect(shot.getByTestId("computer-card-observed")).toContainText("blacked out");
    await accessible(page, "a screenshot card");
    // The full-size frame is the stored one: 1280 x 800 (the model's canvas), and the password field's region is black.
    await shot.getByTestId("computer-frame-full").click();
    const full = shot.getByTestId("computer-frame-full-img");
    await expect(full).toBeVisible();
    const probe = await full.evaluate(async (img: HTMLImageElement) => {
      await img.decode();
      const c = document.createElement("canvas");
      c.width = img.naturalWidth;
      c.height = img.naturalHeight;
      const g = c.getContext("2d")!;
      g.drawImage(img, 0, 0);
      return { w: img.naturalWidth, h: img.naturalHeight, any: g.getImageData(0, 0, c.width, c.height).data.some((v) => v !== 0) };
    });
    expect([probe.w, probe.h]).toEqual([1280, 800]);
    expect(probe.any, "the frame is a picture, not blank").toBe(true);
    // Only a frame this task's observations name can be read: another object, or a made-up reference, is refused.
    const ref = await page.evaluate(async (t) => {
      const s = await window.modbit.localState();
      const cards = await window.modbit.computerCards(s.sessionId!, t);
      return cards.cards.find((c) => c.observation?.artifactRef)?.observation?.artifactRef ?? "";
    }, taskId);
    expect(ref).toMatch(/^[0-9a-f]{64}$/);
    const refused = await page.evaluate((t) => window.modbit.computerFrame(t, "0".repeat(64), true).then(() => "read", (e: Error) => e.message), taskId);
    expect(refused).toContain("UNKNOWN_ARTIFACT");
    const notHex = await page.evaluate((t) => window.modbit.computerFrame(t, "../../etc/passwd", true).then(() => "read", (e: Error) => e.message), taskId);
    expect(notHex).toContain("BAD_ARGUMENT");
  } finally {
    await closeApp(app);
    model.server.close();
  }
});

test("PX-074: every call is approved on its own exact card; no mode, rule or Always covers native control; a stale intent is refused; Skip is typed; the keyboard operates the card", async () => {
  test.setTimeout(300_000);
  const repo = newRepo();
  const rig = computerRig();
  const model = await computerModel([
    plan(),
    start(FORM),
    () => call("computer.state", {}),
    (s) => call("computer.press", { element: elementOf(s, "Move the window") }),
    () => call("computer.state", {}),
    (s) => call("computer.set_value", { element: elementOf(s, "Name"), value: "Ada" }),
  ]);
  const { app, page } = await launch(rig.dataDir, { env: { ...rig.env, MODBIT_OPENAI_BASE_URL: model.url } });
  try {
    await setContentSize(app, page, 1280, 800);
    const taskId = await startAndOpen(page, "submit the order", repo);
    const sid = await sessionOf(page);
    // The widest run mode there is, chosen through the Core with its acknowledgement: it still asks.
    await page.evaluate(([s, t]) => window.modbit.setRunMode(s!, t!, "RUN_EVERYTHING", true), [sid, taskId]);
    await expect(page.getByTestId("approval-tool")).toHaveText("computer.start", { timeout: 90_000 });
    const grantHash = (await page.getByTestId("approval-card").getAttribute("data-intent-hash"))!;
    await expect(page.getByTestId("computer-approval")).toHaveAttribute("data-kind", "observation_grant");
    await expect(page.getByTestId("computer-approval-grant")).toContainText("approved separately");
    await expect(page.getByTestId("computer-approval-reason")).toContainText("the test needs it");
    for (const id of ["approval-always", "approval-prefix", "approval-scope", "approval-change-mode", "approval-always-why"]) await expect(page.getByTestId(id), id).toHaveCount(0);
    // No rule can be made from it, whatever the renderer sends: main refuses, and so does the Core.
    const approvalId = (await page.getByTestId("approval-card").getAttribute("data-approval-id"))!;
    const always = await page.evaluate(([s, a, h]) => window.modbit.allowAlways(s!, a!, h!, { pattern: ["computer.start"], scope: "TASK", expiresAtMs: 0, coversAlwaysAsk: true }).then(() => "made", (e: Error) => e.message), [sid, approvalId, grantHash]);
    expect(always).toContain("ALWAYS_REFUSED");
    // A stale intent is refused by the Core and the card stays exactly as it was.
    const stale = await page.evaluate(([s, a]) => window.modbit.resolveApproval(s!, a!, true, "stale", "0".repeat(64)).then(() => "accepted", (e: Error) => e.message), [sid, approvalId]);
    expect(stale).toContain("INTENT_MISMATCH");
    await expect(page.getByTestId("approval-card")).toHaveAttribute("data-intent-hash", grantHash);
    // Keyboard only: Enter, then Enter at the second step.
    await accessible(page, "the observation grant card");
    await page.getByTestId("approval-card").focus();
    await page.keyboard.press("Enter");
    await expect(page.getByTestId("approval-confirm")).toContainText("observe");
    await accessible(page, "the second step of an observation grant");
    await page.keyboard.press("Enter");

    // The first press is approved with the keyboard: that approves this one call and nothing else.
    await expect(page.getByTestId("approval-tool")).toHaveText("computer.press", { timeout: 90_000 });
    const firstHash = (await page.getByTestId("approval-card").getAttribute("data-intent-hash"))!;
    expect(firstHash).not.toBe(grantHash);
    await approveWithKeyboard(page, "computer.press");
    await expect(page.getByTestId("approval-last")).toContainText("Approved", { timeout: 30_000 });
    await expect(cardOf(page, 3)).toHaveAttribute("data-outcome", "SUCCEEDED", { timeout: 90_000 });
    await expect(cardOf(page, 3).getByTestId("computer-card-approval")).toHaveAttribute("data-intent-hash", firstHash);
    // The second press is a different call with its own card and intent: the earlier approval covers nothing here.
    await expect(page.getByTestId("approval-card")).not.toHaveAttribute("data-intent-hash", firstHash, { timeout: 60_000 });
    await expect(page.getByTestId("approval-tool")).toHaveText("computer.set_value");
    await expect(page.getByTestId("approval-count")).toContainText("1 pending");
    // Escape skips it: a typed user decision, nothing is done, and the card says Skipped.
    await page.getByTestId("approval-card").focus();
    await page.keyboard.press("Escape");
    await expect(cardOf(page, 5)).toHaveAttribute("data-code", "APPROVAL_DENIED", { timeout: 60_000 });
    await expect(cardOf(page, 5).getByTestId("computer-card-status")).toContainText("Skipped");
  } finally {
    await closeApp(app);
    model.server.close();
  }
});

test("PX-074: a Core killed with control active leaves the surface showing only what the restarted Core says", async () => {
  test.setTimeout(300_000);
  const repo = newRepo();
  const rig = computerRig();
  const model = await computerModel([plan(), start(FORM), () => call("computer.state", {}), (s) => call("computer.press", { element: elementOf(s, "Hang after delivery") })]);
  const { app, page } = await launch(rig.dataDir, { env: { ...rig.env, MODBIT_OPENAI_BASE_URL: model.url } });
  try {
    await setContentSize(app, page, 1280, 800);
    const taskId = await startAndOpen(page, "operate the form app", repo);
    await approveWithKeyboard(page, "computer.start");
    await approveWithKeyboard(page, "computer.press");
    await expect.poll(() => existsSync(rig.hangMarker), { timeout: 60_000 }).toBe(true);
    await expect(page.getByTestId("computer-presence-entry")).toHaveAttribute("data-state", "CONTROLLING");

    // The Core dies with the input in flight: while it is gone the surface claims nothing about the machine.
    const info = await page.evaluate(() => window.modbit.debugCoreInfo());
    process.kill(info!.pid, "SIGKILL");
    await expect(page.getByTestId("computer-presence")).toHaveCount(0, { timeout: 30_000 });
    await expect(page.getByTestId("core-status")).toContainText("Core connected", { timeout: 90_000 });
    await expect(page.getByTestId("core-status")).not.toContainText(`pid ${info!.pid}`);
    // The restarted Core's truth: nobody holds the machine, and the input that was in flight is of unknown outcome.
    const entry = page.getByTestId("computer-presence-entry");
    await expect(entry).toHaveAttribute("data-state", "LATCHED", { timeout: 60_000 });
    await expect(entry).toHaveAttribute("data-task-id", taskId);
    await expect(page.getByTestId("computer-stop")).toHaveCount(0);
    const view = await page.evaluate(() => window.modbit.computerPresence());
    expect(view?.controls.map((c) => ({ state: c.state, control: c.controlId, latch: c.latch.includes("CORE_RESTART") }))).toEqual([{ state: "LATCHED", control: "", latch: true }]);
    await accessible(page, "presence after a Core restart");

    // The person looks at the machine and resolves it: the Core agrees and the surface is clear.
    const sid = await sessionOf(page);
    await page.evaluate(([s, t]) => window.modbit.computerResolveLatch(s!, t!, "looked"), [sid, taskId]);
    await expect(page.getByTestId("computer-presence")).toHaveCount(0, { timeout: 30_000 });
  } finally {
    await closeApp(app);
    model.server.close();
  }
});

test("PX-074: Stop is one chord away and keyboard-only, even behind a modal dialog", async () => {
  test.setTimeout(240_000);
  const repo = newRepo();
  const rig = computerRig();
  // The model holds its next turn: the turn must still be running when the person looks at what the Stop left behind.
  const hold = new Gate();
  const model = await computerModel([plan(), start(FORM), () => call("computer.state", {}), (s) => call("computer.press", { element: elementOf(s, "Hang after delivery") }), () => [{ wait: hold }, ...call("computer.state", {})]]);
  const { app, page } = await launch(rig.dataDir, { env: { ...rig.env, MODBIT_OPENAI_BASE_URL: model.url } });
  try {
    await setContentSize(app, page, 1280, 800);
    await startAndOpen(page, "operate the form app", repo);
    await approveWithKeyboard(page, "computer.start");
    await approveWithKeyboard(page, "computer.press");
    await expect.poll(() => existsSync(rig.hangMarker), { timeout: 60_000 }).toBe(true);
    await expect(page.getByTestId("computer-presence-entry")).toHaveAttribute("data-state", "CONTROLLING");
    // Stop is reachable by Tab: it is a real button, named for the task.
    await page.getByTestId("computer-stop").focus();
    expect(await page.evaluate(() => document.activeElement?.getAttribute("data-testid"))).toBe("computer-stop");
    // A modal dialog blocks the shell's other keys; the Stop chord still answers.
    await openPermissions(page);
    await page.keyboard.press(`${MOD}+Shift+s`);
    await expect(page.getByTestId("computer-presence-entry")).toHaveAttribute("data-state", "USER_ABORTED", { timeout: 15_000 });
    await expect(page.getByTestId("computer-live")).toContainText("stopped by you");
  } finally {
    await closeApp(app);
    model.server.close();
  }
});

test("PX-074: a call blocked by a missing permission routes the permission window once; closing it keeps it closed; a grant is noticed", async () => {
  test.setTimeout(240_000);
  const repo = newRepo();
  const rig = computerRig({ permissions: { accessibility: false, screen_recording: true } });
  const model = await computerModel([plan(), start(FORM), () => call("computer.state", {}), () => call("computer.state", {})]);
  const { app, page } = await launch(rig.dataDir, { env: { ...rig.env, MODBIT_OPENAI_BASE_URL: model.url } });
  try {
    await setContentSize(app, page, 1280, 800);
    await startAndOpen(page, "read the form app", repo);
    await expect(page.getByTestId("computer-permissions")).toHaveCount(0);
    await approveWithKeyboard(page, "computer.start");
    // The read is refused PERMISSION_REQUIRED by the actuator; the window routes itself, once, here in the root window.
    await expect(page.getByTestId("computer-permissions")).toBeVisible({ timeout: 90_000 });
    await expect(page.getByTestId("perm-accessibility")).toHaveAttribute("data-status", "allow");
    await expect(page.getByTestId("perm-screen_recording")).toHaveAttribute("data-status", "ready");
    await page.getByTestId("perm-done").click();
    await expect(page.getByTestId("computer-permissions")).toHaveCount(0);
    // The refusal is a card in its own typed words.
    await expect(cardOf(page, 3)).toHaveAttribute("data-code", "PERMISSION_REQUIRED");
    await expect(cardOf(page, 3).getByTestId("computer-card-detail")).toContainText("permission");
    // Closed means closed: a grant elsewhere does not pop it open again, and the next read is not blocked.
    setPermissions(rig, { accessibility: true, screen_recording: true });
    await expect.poll(() => page.evaluate(() => window.modbit.computerPresence().then((p) => p?.accessibility))).toBe("GRANTED");
    await expect(page.getByTestId("computer-permissions")).toHaveCount(0);
  } finally {
    await closeApp(app);
    model.server.close();
  }
});
