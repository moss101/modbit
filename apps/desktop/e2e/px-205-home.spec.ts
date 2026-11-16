/**
 * QUAL-PX-205 / PX-E2E-205 (DR-PX-2026-11-11-021 MWS-12; DR-PX-2026-11-04-020 AWS-01 and AWS-10 items 3 and 6): the measured
 * home on the real Electron build, a real Core and a scripted model.
 *
 * First run: a fresh profile with no provider and no trusted folder. Home is the product's name and the composer in a tray with
 * where the task runs attached; the first-run steps are under it and each does something real (the provider's live test call,
 * a folder trusted through the Core with its starter tasks, the cursor put in the box). The location selector shows cloud
 * disabled with its reason while the graph says PX-128 is not COMPLETE. The mode, the model and the run mode chosen on home are
 * on the Core before the start: the first request the model receives is on the chosen pin, the task's posture is the chosen
 * mode and its run mode the chosen one. Send creates, starts and opens; Alt+Enter creates and opens without starting.
 *
 * Waiting: two tasks wait on approvals. A protected write is decided from home's list (and its effect is real: the remote is
 * added); the destructive one offers no decision on home and opens its task, where the card asks twice, and the Core still lists
 * it as pending. Every control on home is accounted for (a control nobody proves fails), and every disabled one says why.
 *
 * Retained mutations: a destructive effect decided while home showed it; cloud enabled in the location menu; a first request
 * off the chosen pin; a control nobody proves; the environment strip moved above the composer. Each fails its check.
 * With MODBIT_SHOTS_DIR set the same run writes the evidence screenshots.
 */
import { expect, test, type Page } from "@playwright/test";
import { mkdirSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { createServer, type Server } from "node:http";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { agentRow, expectTaskState, setDensity, toNewTask } from "./support/new-task.ts";
import { accessible, appDir, box, closeApp, git, launch, makeRepo, setContentSize } from "./support/ui-harness.ts";

const SHOTS = process.env.MODBIT_SHOTS_DIR ?? "";
const URL1 = "https://example.invalid/one.git";
const tmp = (name: string) => mkdtempSync(join(tmpdir(), `modbit-e2e-px205-${name}-`));

// ---------------------------------------------------------------- the scripted model

type Part = string | { call: { name: string; args: unknown } };
interface Body {
  model?: string;
  messages?: { role: string; content?: unknown }[];
  tools?: { function?: { name?: string } }[];
  [k: string]: unknown;
}
interface Routed {
  server: Server;
  url: string;
  bodies: () => Body[];
  /** The requests whose conversation holds `goal`, in the order they arrived. */
  forGoal: (goal: string) => Body[];
}

/**
 * An OpenAI-compatible scripted model that answers each task from its own script: the script is chosen by a key the task's goal
 * holds, the step by the number of tool results already in the request. Every request body is recorded as the Core sent it.
 */
function routedModel(routes: Record<string, Part[][]>): Promise<Routed> {
  const seen: Body[] = [];
  return new Promise((done) => {
    const server = createServer((req, res) => {
      let raw = "";
      req.on("data", (c) => (raw += c));
      req.on("end", () => {
        const body = JSON.parse(raw || "{}") as Body;
        seen.push(body);
        const text = JSON.stringify(body.messages ?? []);
        const key = Object.keys(routes).find((k) => text.includes(k));
        const results = (body.messages ?? []).filter((m) => m.role === "tool").length;
        const parts = (key ? routes[key]![results] : undefined) ?? ["Nothing further."];
        res.writeHead(200, { "content-type": "text/event-stream" });
        const send = (o: unknown) => res.write(`data: ${JSON.stringify(o)}\n\n`);
        let calls = 0;
        for (const p of parts) {
          if (typeof p === "string") send({ id: "c", model: "scripted", choices: [{ index: 0, delta: { content: p }, finish_reason: null }] });
          else send({ id: "c", model: "scripted", choices: [{ index: 0, delta: { tool_calls: [{ index: calls, id: `call_${results}_${calls++}`, type: "function", function: { name: p.call.name, arguments: JSON.stringify(p.call.args) } }] }, finish_reason: null }] });
        }
        send({ id: "c", model: "scripted", choices: [{ index: 0, delta: {}, finish_reason: calls > 0 ? "tool_calls" : "stop" }], usage: { prompt_tokens: 10, completion_tokens: 5 } });
        res.write("data: [DONE]\n\n");
        res.end();
      });
    });
    server.listen(0, "127.0.0.1", () =>
      done({
        server,
        url: `http://127.0.0.1:${(server.address() as { port: number }).port}`,
        bodies: () => seen,
        forGoal: (goal) => seen.filter((b) => JSON.stringify(b.messages ?? []).includes(goal)),
      }),
    );
  });
}

// ---------------------------------------------------------------- screenshots

async function setTheme(page: Page, theme: "light" | "dark"): Promise<void> {
  await page.evaluate((t) => document.documentElement.setAttribute("data-theme", t), theme);
  await page.evaluate(() => new Promise<void>((d) => requestAnimationFrame(() => requestAnimationFrame(() => d()))));
}

/**
 * The evidence screenshots of one state in light and dark and in both densities (only when asked for): `show` brings the state
 * back after the density changed (Settings is opened and closed to change it), `hide` puts things back after the pictures.
 */
async function shots(page: Page, name: string, show: () => Promise<void> = async () => {}, hide: () => Promise<void> = async () => {}): Promise<void> {
  if (!SHOTS) return;
  mkdirSync(SHOTS, { recursive: true });
  for (const density of ["comfortable", "compact"] as const) {
    await setDensity(page, density);
    await show();
    for (const theme of ["light", "dark"] as const) {
      await setTheme(page, theme);
      await page.waitForTimeout(400);
      await page.screenshot({ path: join(SHOTS, `${name}-${density}-${theme}.png`) });
    }
    await hide();
  }
  await setDensity(page, "comfortable");
  await setTheme(page, "light");
}

// ---------------------------------------------------------------- the checks

/** MWS-12's arrangement (empty when it holds): the name, then the tray holding the composer with its bar and, under it, the environment strip. */
async function trayDrift(page: Page): Promise<string[]> {
  const bad: string[] = [];
  const brand = await box(page, "home-brand");
  const tray = await box(page, "home-tray");
  const composer = await box(page, "home-composer");
  const env = await box(page, "home-env");
  const goal = await box(page, "goal");
  const bar = await box(page, "home-bar");
  const centre = await box(page, "region-conversation");
  if (brand.y + brand.height > tray.y + 1) bad.push("the name is not above the tray");
  if ((await page.getByTestId("home-brand").textContent()) !== "Modbit") bad.push("the name is not the product's");
  if (tray.width > 768 + 1) bad.push(`the tray is ${tray.width} wide, wider than the 768 column`);
  if (Math.abs(tray.x + tray.width / 2 - (centre.x + centre.width / 2)) > 4) bad.push("the tray is not centred in the centre region");
  const inside = (b: { x: number; y: number; width: number; height: number }) => b.x >= tray.x - 1 && b.y >= tray.y - 1 && b.x + b.width <= tray.x + tray.width + 1 && b.y + b.height <= tray.y + tray.height + 1;
  if (!inside(composer)) bad.push("the composer is not in the tray");
  if (!inside(env)) bad.push("the environment strip is not in the tray");
  if (env.y < composer.y + composer.height - 1) bad.push("the environment strip is not under the composer");
  const radius = await page.getByTestId("home-composer").evaluate((el) => getComputedStyle(el).borderTopLeftRadius);
  if (radius !== "14px") bad.push(`the composer's radius is ${radius}`);
  const trayRadius = parseFloat(await page.getByTestId("home-tray").evaluate((el) => getComputedStyle(el).borderTopLeftRadius));
  if (!(trayRadius > 14)) bad.push(`the tray's radius ${trayRadius} is not around the composer's`);
  if (bar.y < goal.y + goal.height - 1) bad.push("the bar is not under the text");
  // The bar: add context, the mode, then at the right the model and send, on one line.
  const xs: number[] = [];
  for (const id of ["home-add-context", "home-mode-button", "model-chip", "run"]) {
    const b = await box(page, id);
    if (Math.abs(b.y + b.height / 2 - (bar.y + bar.height / 2)) > 2) bad.push(`${id} is not on the bar's line`);
    xs.push(b.x);
  }
  if (xs.some((x, i) => i > 0 && x <= xs[i - 1]!)) bad.push(`the bar's order is not add context, mode, model, send: ${xs.join(" ")}`);
  const send = await box(page, "run");
  if (send.x + send.width < composer.x + composer.width - 24) bad.push("send is not at the bar's end");
  // The strip: location, folder, own worktree, then the run mode at the right.
  const ex: number[] = [];
  for (const id of ["home-location", "home-folder", "home-switch", "home-runmode"]) {
    const loc = id === "home-switch" ? page.locator(".home-switch") : page.getByTestId(id);
    const b = (await loc.boundingBox())!;
    if (!(b.y >= env.y - 1 && b.y + b.height <= env.y + env.height + 1)) bad.push(`${id} is not in the strip`);
    ex.push(b.x);
  }
  if (ex.some((x, i) => i > 0 && x <= ex[i - 1]!)) bad.push(`the strip's order is not location, folder, own worktree, run mode: ${ex.join(" ")}`);
  const rm = await box(page, "home-runmode");
  if (rm.x + rm.width < env.x + env.width - 16) bad.push("the run mode is not at the strip's end");
  // Under the tray: what waits on the person, then the steps.
  const under = [tray];
  for (const id of ["home-waiting", "welcome"]) if ((await page.getByTestId(id).count()) === 1) under.push(await box(page, id));
  if (under.some((b, i) => i > 0 && b.y < under[i - 1]!.y + under[i - 1]!.height)) bad.push("waiting and the steps are not under the tray in that order");
  return bad;
}

/**
 * "Nothing is drawn that does nothing" (DR-PX-2026-11-04-020 AWS-07; the QUAL's last fail clause): every control home draws is
 * one this spec drives to its effect, and every disabled control says why. A control that is not on the list fails.
 */
const PROVEN = new Set(["goal", "issue-url", "home-issue-remove", "home-add-context", "home-mode-button", "model-chip", "run", "home-location", "workspace", "isolate", "home-runmode", "home-waiting-skip", "home-waiting-allow", "home-waiting-open", "provider-kind", "provider-key", "provider-url", "provider-test", "welcome-repo-toggle", "trust-root", "trust-confirm", "welcome-write-task", "starter-task"]);
async function inertControls(page: Page): Promise<string[]> {
  const found = await page.getByTestId("home").evaluate((root) => {
    const out: { id: string; disabled: boolean; why: string }[] = [];
    for (const el of root.querySelectorAll<HTMLElement>("button, input, textarea, select, summary, a[href], [role='button'], [role='switch'], [tabindex]:not([tabindex='-1'])")) {
      const host = el.tagName === "SUMMARY" ? el.closest("details") : el;
      const id = host?.getAttribute("data-testid") ?? `${el.tagName.toLowerCase()}${el.className ? `.${String(el.className).split(" ")[0]}` : ""}`;
      const disabled = (el as HTMLButtonElement).disabled === true || el.getAttribute("aria-disabled") === "true";
      const label = el.closest("label");
      const why = el.getAttribute("title") ?? label?.getAttribute("title") ?? (el.getAttribute("aria-describedby") ? document.getElementById(el.getAttribute("aria-describedby")!)?.textContent ?? "" : "");
      out.push({ id, disabled, why });
    }
    return out;
  });
  const bad: string[] = [];
  for (const c of found) {
    if (!PROVEN.has(c.id)) bad.push(`control ${c.id} is drawn and nothing proves what it does`);
    else if (c.disabled && !c.why.trim()) bad.push(`control ${c.id} is disabled and does not say why`);
  }
  return bad;
}

/** The location menu (AWS-10 item 6): this machine chosen; cloud disabled with its reason while PX-128 is not COMPLETE in the graph. */
function px128Status(): string {
  const g = JSON.parse(readFileSync(resolve(appDir, "..", "..", "graph", "project-graph.json"), "utf8")) as { nodes: { id: string; status?: string }[] };
  return g.nodes.find((n) => n.id === "PX-128")?.status ?? "NOT_FOUND";
}
async function cloudDrift(page: Page): Promise<string[]> {
  const bad: string[] = [];
  const cloud = page.getByTestId("menu-item-location-cloud");
  if ((await cloud.count()) !== 1) return ["the location menu has no cloud entry"];
  const disabled = (await cloud.getAttribute("aria-disabled")) === "true";
  const text = (await cloud.textContent()) ?? "";
  if (px128Status() !== "COMPLETE") {
    if (!disabled) bad.push("cloud can be chosen while PX-128 is not COMPLETE");
    if (!/not available from this app yet/.test(text)) bad.push(`cloud does not say why it is disabled: "${text}"`);
  }
  if ((await page.getByTestId("menu-item-location-local").getAttribute("aria-checked")) !== "true") bad.push("this machine is not the chosen location");
  return bad;
}

/** Whether a pending approval was decided while home showed it: the Core must still list it, and home must offer no decision for it. */
async function destructiveDecidedFromHome(page: Page, sessionId: string, approvalId: string): Promise<string[]> {
  const bad: string[] = [];
  const pending = await page.evaluate((s) => window.modbit.dockApprovals(s).then((l) => l.map((a) => a.approvalId)), sessionId);
  if (!pending.includes(approvalId)) bad.push("the destructive effect is no longer pending: it was decided while home showed it");
  const row = page.locator(`[data-testid="home-waiting-item"][data-approval-id="${approvalId}"]`);
  if ((await row.count()) === 1) {
    for (const id of ["home-waiting-allow", "home-waiting-skip"]) if ((await row.getByTestId(id).count()) !== 0) bad.push(`home offers ${id} for an effect that cannot be undone`);
    if ((await row.getByTestId("home-waiting-open").count()) !== 1) bad.push("the destructive effect does not open its task");
  }
  return bad;
}

/** The first request a task's model received is on the pin chosen for it (read at the scripted provider, as the Core sent it). */
function offPin(first: Body | undefined, want: string): string[] {
  if (!first) return ["no model request was made for the task"];
  return first.model === want ? [] : [`the first model request is on ${first.model}, not on the chosen ${want}`];
}

const sessionOf = (page: Page) => page.evaluate(() => window.modbit.localState().then((l) => l.sessionId ?? ""));
const conversationTask = async (page: Page) => {
  const conv = page.getByTestId("conversation");
  await expect(conv).toHaveAttribute("data-task-id", /.+/, { timeout: 30_000 });
  return (await conv.getAttribute("data-task-id"))!;
};

// ---------------------------------------------------------------- first run, choices on home, send and Alt+Enter

test("PX-205 first run: the tray, the first-run steps that do something, the location with cloud disabled, the mode, the pin and the run mode on the Core before the first request; send starts, Alt+Enter does not", async () => {
  test.setTimeout(360_000);
  const repo = makeRepo(tmp("repo"));
  const SENT = "Plan the notes change from home";
  const KEPT = "Kept on home for later";
  const OFF = "Started off its pin on purpose";
  const model = await routedModel({ [SENT]: [["Planned."]], [KEPT]: [["Must not run before it is started."]], [OFF]: [["Ran."]] });
  const { app, page } = await launch(tmp("first"), { env: { MODBIT_OPENAI_BASE_URL: "", MODBIT_ANTHROPIC_BASE_URL: "" } });
  try {
    await setContentSize(app, page, 1440, 900);
    await setTheme(page, "light");
    await expect(page.getByTestId("home")).toBeVisible();

    // ---- MWS-12: the name, the tray, the composer with its bar, the environment strip; the steps under it.
    expect(await trayDrift(page)).toEqual([]);
    const welcome = page.getByTestId("welcome");
    await expect(welcome).toBeVisible();
    await expect(page.getByTestId("welcome-progress")).toHaveText("0 of 3");
    await expect(page.getByTestId("welcome-step-provider")).toHaveAttribute("data-active", "true");
    await expect(page.getByTestId("home-waiting")).toHaveCount(0);
    expect(await inertControls(page)).toEqual([]);
    await accessible(page, "first-run home", '[data-testid="shell"]');
    await shots(page, "home-first-run");

    // ---- The first-run steps, each with a real effect. The provider: a live test call to the scripted provider.
    await page.getByTestId("provider-key").fill("sk-px205-test-key");
    await page.getByTestId("provider-url").fill(model.url);
    await page.getByTestId("provider-test").click();
    await expect(page.getByTestId("provider-result")).toHaveAttribute("data-ok", "true", { timeout: 30_000 });
    await expect(page.getByTestId("welcome-step-provider")).toHaveAttribute("data-done", "true");
    await expect(page.getByTestId("welcome-progress")).toHaveText("1 of 3");
    await expect(page.getByTestId("welcome-step-repository")).toHaveAttribute("data-active", "true");
    // The folder: trusted through the Core, then the starter tasks the Core detects for it.
    await page.getByTestId("welcome-repo-toggle").locator("summary").click();
    await page.getByTestId("trust-root").fill(repo);
    await page.getByTestId("trust-confirm").click();
    await expect(page.getByTestId("repository-trusted")).toContainText(repo.split(/[\\/]/).pop()!, { timeout: 15_000 });
    await expect(page.getByTestId("welcome-progress")).toHaveText("2 of 3");
    await expect(page.getByTestId("workspace"), "the trusted folder is the task's folder").toHaveValue(/.+/);
    // The first task: the step puts the cursor in the box.
    await page.getByTestId("welcome-write-task").click();
    await expect(page.getByTestId("goal")).toBeFocused();

    // ---- AWS-10 item 6: the location selector, cloud disabled with its reason while PX-128 is not COMPLETE.
    expect(px128Status(), "the graph names PX-128").not.toBe("NOT_FOUND");
    await page.getByTestId("home-location").click();
    expect(await cloudDrift(page)).toEqual([]);
    await page.getByTestId("menu-item-location-cloud").click({ force: true });
    await expect(page.getByTestId("home-location")).toContainText(/This (Mac|PC|computer)/);
    await accessible(page, "the location menu", '[data-testid="shell"]');
    // Retained mutation: cloud made choosable fails the check.
    await page.getByTestId("menu-item-location-cloud").evaluate((el) => el.setAttribute("aria-disabled", "false"));
    expect((await cloudDrift(page)).some((d) => d.startsWith("cloud can be chosen"))).toBe(px128Status() !== "COMPLETE");
    await page.keyboard.press("Escape");
    await shots(
      page,
      "home-location-menu",
      async () => {
        await page.getByTestId("home-location").click();
        await expect(page.getByTestId("menu-item-location-cloud")).toBeVisible();
      },
      async () => {
        await page.keyboard.press("Escape");
        await expect(page.getByTestId("menu-item-location-cloud")).toHaveCount(0);
      },
    );

    // ---- Add context: an issue URL is entered from it, and removed again.
    await page.getByTestId("home-add-context").click();
    await page.getByTestId("menu-item-home-add-issue").click();
    await expect(page.getByTestId("issue-url")).toBeFocused();
    await page.getByTestId("issue-url").fill("https://github.test/o/r/issues/7");
    await page.getByTestId("home-issue-remove").click();
    await expect(page.getByTestId("issue-url")).toHaveCount(0);

    // ---- Own worktree: off without a folder (and says why), a switch with a folder.
    const folder = await page.getByTestId("workspace").inputValue();
    await page.getByTestId("workspace").fill("");
    await expect(page.getByTestId("isolate")).toBeDisabled();
    expect(await inertControls(page)).toEqual([]);
    await page.getByTestId("workspace").fill(folder);
    await page.locator(".home-switch").click();
    await expect(page.getByTestId("isolate")).toBeChecked();
    await page.locator(".home-switch").click();
    await expect(page.getByTestId("isolate")).not.toBeChecked();

    // ---- The run mode: one that approves more asks for the acknowledgement first, in the task panel's own dialog.
    await expect(page.getByTestId("home-runmode-label")).toHaveText("Supervised");
    await page.getByTestId("home-runmode").click();
    await page.getByTestId("menu-item-home-runmode-allowlist_sandbox").click();
    await expect(page.getByTestId("runmode-warning")).toBeVisible();
    await expect(page.getByTestId("runmode-warning-confirm")).toBeDisabled();
    await page.getByTestId("runmode-warning-cancel").click();
    await expect(page.getByTestId("home-runmode-label")).toHaveText("Supervised");
    await page.getByTestId("home-runmode").click();
    await page.getByTestId("menu-item-home-runmode-allowlist_sandbox").click();
    await page.getByTestId("runmode-understood").check();
    await page.getByTestId("runmode-warning-confirm").click();
    await expect(page.getByTestId("home-runmode-label")).toHaveText("Supervised with sandbox");

    // ---- The mode and the pin (AWS-10 item 3).
    await page.getByTestId("home-mode-button").click();
    await page.getByTestId("menu-item-plan").click();
    await expect(page.getByTestId("home-mode")).toHaveAttribute("data-mode", "PLAN");
    await page.getByTestId("model-chip").click();
    await page.locator('[data-row-id="openai/gpt-4.1#standard"]').click();
    await expect(page.getByTestId("model-chip-name")).toHaveText("gpt-4.1");

    // ---- Send: created, started and opened; the first request is on the pin; mode and run mode are the Core's.
    await page.getByTestId("goal").fill(SENT);
    await page.getByTestId("run").click();
    const sent = await conversationTask(page);
    await expect.poll(() => model.forGoal(SENT).length, { timeout: 60_000 }).toBeGreaterThan(0);
    expect(offPin(model.forGoal(SENT)[0], "gpt-4.1")).toEqual([]);
    const posture = await page.evaluate((t) => window.modbit.composerPosture(t), sent);
    expect(posture.mode).toBe("PLAN");
    expect(posture.preference).toMatchObject({ pinEndpoint: "openai", pinModel: "gpt-4.1" });
    expect((await page.evaluate((t) => window.modbit.runMode(t), sent)).mode).toBe("ALLOWLIST_SANDBOX");
    // PLAN is read-only at the Core: the first request offers the model no tool that writes.
    const offered = (model.forGoal(SENT)[0]!.tools ?? []).map((t) => t.function?.name ?? "");
    expect(offered.length, "the request offers tools").toBeGreaterThan(0);
    expect(offered.filter((n) => /change\.apply|shell\.exec|git\.commit|git\.push/.test(n)), "a PLAN request offers no writing tool").toEqual([]);

    // ---- Back on home nothing carries over; the steps are gone (a provider and a task exist).
    await toNewTask(page);
    await expect(page.getByTestId("home-mode")).toHaveAttribute("data-mode", "AGENT");
    await expect(page.getByTestId("model-chip-name")).toHaveText("Auto");
    await expect(page.getByTestId("home-runmode-label")).toHaveText("Supervised");
    await expect(page.getByTestId("welcome")).toHaveCount(0);

    // ---- Alt+Enter: created and opened, not started; the model never hears of it.
    await page.getByTestId("goal").fill(KEPT);
    await page.getByTestId("workspace").fill(repo);
    await page.getByTestId("goal").press("Alt+Enter");
    const kept = await conversationTask(page);
    expect(kept).not.toBe(sent);
    await expectTaskState(page, kept, "Queued");
    await expect(page.getByTestId("conv-start")).toBeVisible();
    await page.waitForTimeout(1500);
    expect(model.forGoal(KEPT), "the kept task reached the model").toEqual([]);

    // ---- Retained mutation: a first request off the chosen pin fails the check. Chosen on home (gpt-4.1), kept with Alt+Enter,
    // then moved to another row through the bridge before the start: the request the model gets is not on the chosen pin.
    await toNewTask(page);
    await page.getByTestId("model-chip").click();
    await page.locator('[data-row-id="openai/gpt-4.1#standard"]').click();
    await page.getByTestId("goal").fill(OFF);
    await page.getByTestId("goal").press("Alt+Enter");
    const off = await conversationTask(page);
    const other = await page.evaluate(() => window.modbit.composerVariants().then((c) => c.models.find((m) => m.endpoint === "openai" && m.model !== "gpt-4.1" && m.pinAllowed)?.model ?? ""));
    expect(other, "the catalog has another model that can be pinned").not.toBe("");
    const sid = await sessionOf(page);
    await page.evaluate(([s, t, m]) => window.modbit.composerSetPreference(s!, t!, { pin: { endpoint: "openai", model: m! } }), [sid, off, other]);
    await page.getByTestId("conv-start").click();
    await expect.poll(() => model.forGoal(OFF).length, { timeout: 60_000 }).toBeGreaterThan(0);
    expect(offPin(model.forGoal(OFF)[0], "gpt-4.1").join(""), "the check fails on a request off the chosen pin").toMatch(/not on the chosen gpt-4\.1/);
  } finally {
    await closeApp(app);
    model.server.closeAllConnections();
    model.server.close();
  }
});

// ---------------------------------------------------------------- waiting on the person

test("PX-205 waiting: a protected write is decided from home's list with a real effect; the destructive effect offers no decision on home and opens its task, where the card asks twice; every control is accounted for", async () => {
  test.setTimeout(360_000);
  const repo = makeRepo(tmp("repo"));
  writeFileSync(join(repo, "scratch.tmp"), "untracked\n");
  const ADD = "Add the origin1 remote";
  const CLEAN = "Clean the build output";
  const model = await routedModel({
    [ADD]: [["Adding the remote.", { call: { name: "shell.exec", args: { argv: ["git", "remote", "add", "origin1", URL1] } } }], ["The remote is added."]],
    [CLEAN]: [["Cleaning the build output.", { call: { name: "shell.exec", args: { argv: ["git", "clean", "-fd"] } } }], ["Done."]],
  });
  const { app, page } = await launch(tmp("waiting"), { env: { MODBIT_OPENAI_BASE_URL: model.url } });
  try {
    await setContentSize(app, page, 1440, 900);
    await setTheme(page, "light");
    // Both tasks are sent from home: each is created, started and opened, and each comes to wait on an approval.
    const ids: Record<string, string> = {};
    for (const goal of [ADD, CLEAN]) {
      await toNewTask(page);
      await page.getByTestId("goal").fill(goal);
      await page.getByTestId("workspace").fill(repo);
      await page.getByTestId("run").click();
      ids[goal] = await conversationTask(page);
      await expect(page.getByTestId("approval-card").first()).toBeVisible({ timeout: 60_000 });
    }
    const sid = await sessionOf(page);
    await expect.poll(() => page.evaluate((s) => window.modbit.dockApprovals(s).then((l) => l.length), sid), { timeout: 60_000 }).toBe(2);
    const listed = await page.evaluate((s) => window.modbit.dockApprovals(s), sid);
    const write = listed.find((a) => a.taskId === ids[ADD])!;
    const destructive = listed.find((a) => a.taskId === ids[CLEAN])!;
    expect(destructive.effectClass).toBe("Destructive");
    // The PLAN check of the first run is not vacuous: a task in the default mode is offered writing tools.
    expect((model.forGoal(ADD)[0]!.tools ?? []).map((t) => t.function?.name ?? "").filter((n) => /change\.apply|shell\.exec/.test(n)).length, "an AGENT request offers writing tools").toBeGreaterThan(0);
    expect(write.effectClass).not.toMatch(/destructive|external/i);

    // ---- Home: the two are waiting on the person, under the tray, with what each wants in words.
    await toNewTask(page);
    const items = page.getByTestId("home-waiting-item");
    await expect(items).toHaveCount(2);
    expect(await trayDrift(page)).toEqual([]);
    // A long folder path and a narrower window (the 900 minimum gives the centre 612) keep the strip on one line: the folder gives way.
    for (const width of [1100, 900]) {
      await setContentSize(app, page, width, 800);
      expect(await trayDrift(page), `at ${width} wide`).toEqual([]);
    }
    await setContentSize(app, page, 1440, 900);
    const writeRow = page.locator(`[data-testid="home-waiting-item"][data-approval-id="${write.approvalId}"]`);
    const destructiveRow = page.locator(`[data-testid="home-waiting-item"][data-approval-id="${destructive.approvalId}"]`);
    await expect(writeRow).toContainText(ADD);
    await expect(writeRow.getByTestId("home-waiting-line")).toHaveText(`Wants to run git remote add origin1 ${URL1}`);
    await expect(writeRow).toHaveAttribute("data-intent-hash", write.intentHash);
    await expect(destructiveRow.getByTestId("home-waiting-line")).toHaveText("Wants to run git clean -fd");
    await expect(destructiveRow).toContainText("Cannot be undone");
    // Plain words: no tool name, no effect class, no Core command on the list.
    expect(await page.getByTestId("home-waiting").innerText()).not.toMatch(/shell\.exec|Destructive|EXTERNAL|ResolveApproval|StartTask|_/);
    expect(await destructiveDecidedFromHome(page, sid, destructive.approvalId)).toEqual([]);
    expect(await inertControls(page)).toEqual([]);
    await accessible(page, "home with two tasks waiting", '[data-testid="shell"]');
    await shots(page, "home-waiting", async () => {
      await toNewTask(page);
      await expect(items).toHaveCount(2);
    });

    // ---- The destructive one opens its task; there the card is the one that decides, and it asks twice.
    await destructiveRow.getByTestId("home-waiting-open").click();
    await expect(page.getByTestId("conversation")).toHaveAttribute("data-task-id", ids[CLEAN]!);
    const card = page.locator(`[data-testid="approval-card"][data-approval-id="${destructive.approvalId}"]`);
    await expect(card).toBeVisible();
    await expect(card).toHaveAttribute("data-effect-class", "Destructive");
    await card.getByTestId("approval-run").click();
    await expect(page.getByTestId("approval-confirm")).toBeVisible();
    await shots(page, "home-destructive-opens-task", async () => {
      await expect(card).toBeVisible();
      if ((await page.getByTestId("approval-confirm").count()) === 0) await card.getByTestId("approval-run").click();
      await expect(page.getByTestId("approval-confirm")).toBeVisible();
    });
    // The pictures change the density through Settings, which mounts the card again on its first step; Back is pressed where the second step still shows.
    if ((await page.getByTestId("approval-confirm").count()) === 1) await page.getByTestId("approval-back").click();
    await expect(page.getByTestId("approval-confirm")).toHaveCount(0);
    expect(await destructiveDecidedFromHome(page, sid, destructive.approvalId)).toEqual([]);

    // ---- The protected write is decided from the list, bound to its hash, and its effect is real.
    await toNewTask(page);
    await expect(writeRow.getByTestId("home-waiting-allow")).toBeVisible();
    await writeRow.getByTestId("home-waiting-allow").click();
    await expect(page.getByTestId("home-waiting-note")).toHaveAttribute("data-kind", "ok");
    await expect(page.getByTestId("home-waiting-note")).toContainText(`Allowed once: “${ADD}”`);
    await expect.poll(() => page.evaluate((s) => window.modbit.dockApprovals(s).then((l) => l.map((a) => a.approvalId)), sid)).toEqual([destructive.approvalId]);
    await expect.poll(() => git(repo, "remote").trim().split(/\s+/), { timeout: 60_000 }).toContain("origin1");
    await expect(items).toHaveCount(1);
    // The destructive effect is still waiting, still undecided, and the file it would delete is still there.
    expect(await destructiveDecidedFromHome(page, sid, destructive.approvalId)).toEqual([]);
    expect(readFileSync(join(repo, "scratch.tmp"), "utf8")).toBe("untracked\n");
    await expect(agentRow(page, ids[CLEAN]!)).toBeVisible();

    // ---- Retained mutations.
    // A control nobody proves: a button added to the strip fails the census.
    await page.getByTestId("home-env").evaluate((el) => {
      const b = document.createElement("button");
      b.type = "button";
      b.textContent = "Branch";
      b.setAttribute("data-testid", "home-branch");
      el.appendChild(b);
    });
    expect(await inertControls(page)).toEqual(["control home-branch is drawn and nothing proves what it does"]);
    await page.getByTestId("home-branch").evaluate((el) => el.remove());
    // The environment strip drawn above the composer fails the arrangement.
    await page.evaluate(() => {
      const s = document.createElement("style");
      s.id = "px205-mutation";
      s.textContent = ".home-tray { display: flex; flex-direction: column-reverse; }";
      document.head.appendChild(s);
    });
    expect(await trayDrift(page)).toContain("the environment strip is not under the composer");
    await page.evaluate(() => document.getElementById("px205-mutation")?.remove());
    expect(await trayDrift(page)).toEqual([]);
    // A destructive effect decided while home showed it (as a list that allowed it would): the check fails, and the Core ran it.
    await page.evaluate(([s, a, h]) => window.modbit.resolveApproval(s!, a!, true, "mutation: allowed from a list", h!), [sid, destructive.approvalId, destructive.intentHash]);
    expect(await destructiveDecidedFromHome(page, sid, destructive.approvalId)).toContain("the destructive effect is no longer pending: it was decided while home showed it");
  } finally {
    await closeApp(app);
    model.server.closeAllConnections();
    model.server.close();
  }
});
