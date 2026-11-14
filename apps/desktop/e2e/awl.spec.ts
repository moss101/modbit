/**
 * The agent workspace language gate (DR-PX-2026-11-03-019 section 6; QUAL-PX-192..194) with the measured workspace's numbers
 * (DR-PX-2026-11-11-021 section 6; QUAL-PX-198), on the real Electron build against a real Core: the emitted colour variables
 * equal the record's tables in light and dark, the shell has the measured geometry and unframed panes in both densities, the
 * surfaces read the mapping of MWS-02, the navigation sits in the task list, the control recipes are the record's, and axe passes
 * in three themes. The screens are the agent workspace of DR-PX-2026-11-04-020: home is one composer, the removed Fleet surfaces
 * stay gone, and the conversation, composer and apps panel have the approved layout. Retained mutations show each check failing.
 */
import { dockDrift } from "./support/dock-gate.ts";
import { expect, test, type Page } from "@playwright/test";
import { existsSync, mkdirSync, mkdtempSync, realpathSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { COMPACT, GEOMETRY, TYPE, densityMetrics, densityVariables, sharedVariables, themeVariables, type Density } from "@modbit/design-tokens";
import { nodeTerminal } from "./support/apps-harness.ts";
import { gearMenu, openTaskMore, runTask, setDensity } from "./support/new-task.ts";
import { streamingModel } from "./support/stream-model.ts";
import { accessible, box, closeApp, git, launch, liftMinimumSize, makeRepo, setContentSize } from "./support/ui-harness.ts";

/** The navigation rows of MWS-04, top to bottom. The search moved from a row of its own into the Tasks head. */
const NAV = ["agents-new-task", "open-inbox", "open-automations", "open-customize"];
/** The Tasks head: search, list options, and more. */
const HEAD = ["agents-search-open", "agents-tools-toggle", "agents-more"];
/** What the rebuilds removed: none of these may come back. The last two are the Repositories section DR-PX-2026-11-11-021 removed. */
const REMOVED = ["fleet", "fleet-empty", "fleet-loading", "search", "agents-fleet", "task-card", "column-needsAttention", "column-running", "column-waiting", "column-readyForReview", "column-completed", "column-failed", "status-row", "brand", "composer-suggestions", "language-labels", "platform-state", "agent-repositories", "agent-repository"];
const DENSITIES: Density[] = ["comfortable", "compact"];

async function setTheme(page: Page, theme: "light" | "dark" | "high-contrast"): Promise<void> {
  await page.evaluate((t) => document.documentElement.setAttribute("data-theme", t), theme);
  await page.evaluate(() => new Promise<void>((done) => requestAnimationFrame(() => requestAnimationFrame(() => done()))));
}

/** The emitted colour variables that differ from the record's table for a theme (empty when they all match). */
async function tokenDrift(page: Page, theme: "light" | "dark"): Promise<string[]> {
  const want = themeVariables(theme);
  const names = Object.keys(want).filter((k) => k.startsWith("--color-") || k.startsWith("--mb-color-"));
  const got = await page.evaluate((ns) => {
    const cs = getComputedStyle(document.documentElement);
    return Object.fromEntries(ns.map((n) => [n, cs.getPropertyValue(n).trim()]));
  }, names);
  return names.filter((n) => got[n]!.toLowerCase() !== want[n]!.toLowerCase()).map((n) => `${n}: ${got[n]} != ${want[n]}`);
}

/** The emitted size variables that differ from the token source for a density: the shared block with the density's overrides (MWS-03). */
async function scaleDrift(page: Page, density: Density): Promise<string[]> {
  const want = { ...sharedVariables(), ...densityVariables(density) };
  // A variable written over another one is compared where it is used; these are the ones written as a number.
  const names = Object.keys(want).filter((k) => /^--(ui-font-size|mb-(top-bar|row|group-head|list-foot|rail|column|composer-radius|space|radius|control)|radius-)/.test(k) && !want[k]!.includes("var("));
  const got = await page.evaluate((ns) => {
    const cs = getComputedStyle(document.documentElement);
    return Object.fromEntries(ns.map((n) => [n, cs.getPropertyValue(n).trim()]));
  }, names);
  return names.filter((n) => got[n] !== want[n]).map((n) => `${n}: ${got[n]} != ${want[n]}`);
}

/** Computed style of a probe element built with the given markup in the page (removed afterwards). */
async function probe(page: Page, html: string, props: string[]): Promise<Record<string, string>> {
  return page.evaluate(
    ([markup, ps]) => {
      const host = document.createElement("div");
      host.className = "shell-frame";
      host.innerHTML = markup!;
      document.body.appendChild(host);
      const el = host.firstElementChild as HTMLElement;
      const cs = getComputedStyle(el);
      const out = Object.fromEntries((ps as string[]).map((p) => [p, cs.getPropertyValue(p)]));
      host.remove();
      return out;
    },
    [html, props] as const,
  );
}

/** A CSS value resolved in the page (a variable written as hex compares with the computed rgb()). */
async function resolved(page: Page, name: string, prop: "background-color" | "color" | "font-size" | "border-top-color"): Promise<string> {
  return page.evaluate(([n, pr]) => {
    const el = document.createElement("i");
    el.style.setProperty(pr!, `var(${n})`);
    document.body.appendChild(el);
    const out = getComputedStyle(el).getPropertyValue(pr!);
    el.remove();
    return out;
  }, [name, prop] as const);
}

const squash = (c: string) => c.replace(/\s+/g, "");
const clear = (c: string) => squash(c) === "rgba(0,0,0,0)";

/** What the shell and the recipes break in a density (empty when the record holds): the numbers of section 5.1 and 5.2. */
async function shellDrift(page: Page, density: Density = "comfortable"): Promise<string[]> {
  const bad: string[] = [];
  const m = densityMetrics(density);
  const agents = await box(page, "region-agents");
  const center = await box(page, "region-conversation");
  if (Math.abs(agents.width - GEOMETRY.listDefault) > 2) bad.push(`agent list ${agents.width}`);
  const bar = await box(page, "top-bar");
  if (Math.abs(bar.height - m.topBar) > 1) bad.push(`top bar ${bar.height}`);
  const head = await page.locator(".agent-region-head").boundingBox();
  if (!head || Math.abs(head.height - m.topBar) > 1) bad.push(`list head ${head?.height}`);
  // AFW-A04: a 1 px rule at 8 % under the bar.
  const rule = await page.getByTestId("top-bar").evaluate((el) => ({ width: getComputedStyle(el).borderBottomWidth, color: getComputedStyle(el).borderBottomColor }));
  if (rule.width !== "1px" || squash(rule.color) !== squash(await resolved(page, "--color-tab-border", "border-top-color"))) bad.push(`top bar rule ${JSON.stringify(rule)}`);
  if (Math.abs(center.x - (agents.x + agents.width)) > 1) bad.push(`gap between the agent list and the conversation: ${center.x - agents.x - agents.width}`);
  const frame = await page.locator(".shell-frame").evaluate((el) => {
    const cs = getComputedStyle(el);
    return { radius: cs.borderTopLeftRadius, border: cs.borderTopWidth, pad: getComputedStyle(el.parentElement!).paddingTop };
  });
  if (frame.radius !== "0px" || frame.border !== "0px" || frame.pad !== "0px") bad.push(`framed pane ${JSON.stringify(frame)}`);
  // AFW-A05: navigation rows at the row height, 2 apart, radius 8, inset 8 from the list's edges; exactly the rows of MWS-04, in order.
  const rows = await page.getByTestId("agent-nav").locator("button").evaluateAll((els) => els.map((e) => ({ id: e.getAttribute("data-testid") ?? "", y: e.getBoundingClientRect().y, x: e.getBoundingClientRect().x, w: e.getBoundingClientRect().width, h: e.getBoundingClientRect().height, radius: getComputedStyle(e).borderTopLeftRadius })));
  if (rows.map((r) => r.id).join(" ") !== NAV.join(" ")) bad.push(`navigation rows are ${rows.map((r) => r.id).join(" ")}`);
  rows.forEach((r, i) => {
    if (Math.abs(r.h - m.row) > 0.5) bad.push(`navigation row ${r.id} is ${r.h}`);
    if (r.radius !== `${GEOMETRY.rowRadius}px`) bad.push(`navigation row ${r.id} radius ${r.radius}`);
    if (Math.abs(r.x - agents.x - GEOMETRY.rowInset) > 0.5 || Math.abs(agents.x + agents.width - (r.x + r.w) - GEOMETRY.rowInset) > 1.5) bad.push(`navigation row ${r.id} inset ${r.x - agents.x} and ${agents.x + agents.width - (r.x + r.w)}`);
    if (i > 0 && Math.abs(r.y - (rows[i - 1]!.y + rows[i - 1]!.h) - m.rowGap) > 0.5) bad.push(`navigation row ${r.id} is ${r.y - (rows[i - 1]!.y + rows[i - 1]!.h)} from the row above`);
  });
  for (const id of HEAD) if ((await page.getByTestId("agents-head").getByTestId(id).count()) !== 1) bad.push(`${id} is not in the Tasks head`);
  // MWS-03: chrome text at the density's size (14 in Comfortable, 13 on 18 in Compact), from the one --ui-font-size.
  const type = density === "compact" ? COMPACT.type : TYPE;
  const chrome = await page.getByTestId("agents-new-task").evaluate((el) => ({ body: getComputedStyle(document.body).fontSize, size: getComputedStyle(el).fontSize, line: getComputedStyle(el).lineHeight }));
  if (chrome.body !== `${type.chrome.size}px` || chrome.size !== `${type.chrome.size}px` || Math.abs(parseFloat(chrome.line) - type.chrome.line) > 0.1) bad.push(`chrome text ${JSON.stringify(chrome)}`);
  const v = await page.evaluate(() => {
    // Resolved through a probe so a variable written as hex compares with the computed rgb().
    const resolve = (name: string, prop: "background-color" | "color" | "border-top-color") => {
      const el = document.createElement("i");
      el.style.setProperty(prop, `var(${name})`);
      document.body.appendChild(el);
      const out = getComputedStyle(el).getPropertyValue(prop);
      el.remove();
      return out;
    };
    return {
      selected: resolve("--color-selected", "background-color"),
      primary: resolve("--color-primary", "background-color"),
      control: resolve("--mb-color-border-control", "border-top-color"),
      muted: resolve("--mb-color-text-muted", "color"),
    };
  });
  const rgba = squash;
  const bare = await probe(page, "<button>Bare</button>", ["background-color", "border-top-color", "border-top-left-radius", "height"]);
  if (rgba(bare["background-color"]!) !== rgba(v.selected)) bad.push(`bare button fill ${bare["background-color"]} != ${v.selected}`);
  const primary = await probe(page, '<button class="mb-btn" data-variant="primary" data-size="md">Go</button>', ["background-color", "font-weight"]);
  if (primary["font-weight"] !== "500") bad.push(`primary weight ${primary["font-weight"]}`);
  if (rgba(primary["background-color"]!) !== rgba(v.primary)) bad.push(`primary fill ${primary["background-color"]} != ${v.primary}`);
  const outline = await probe(page, '<button class="mb-btn" data-variant="outline" data-size="md">Edit</button>', ["background-color", "border-top-color", "height"]);
  if (!clear(outline["background-color"]!) || rgba(outline["border-top-color"]!) !== rgba(v.control) || outline.height !== "28px") bad.push(`outline recipe ${JSON.stringify(outline)}`);
  const ghost = await probe(page, '<button class="mb-btn" data-variant="ghost" data-size="md">More</button>', ["background-color", "color", "border-top-color"]);
  if (!clear(ghost["background-color"]!) || rgba(ghost.color!) !== rgba(v.muted) || !clear(ghost["border-top-color"]!)) bad.push(`ghost recipe ${JSON.stringify(ghost)}`);
  const secondary = await probe(page, '<button class="mb-btn" data-variant="secondary" data-size="md">Do</button>', ["background-color", "border-top-color", "height", "border-top-left-radius"]);
  if (rgba(secondary["background-color"]!) !== rgba(v.selected)) bad.push(`secondary fill ${secondary["background-color"]}`);
  if (secondary.height !== "28px" || secondary["border-top-left-radius"] !== "6px") bad.push(`secondary geometry ${JSON.stringify(secondary)}`);
  const tab = await probe(page, '<button class="mb-tab" role="tab" aria-selected="true">Changes</button>', ["background-color", "border-top-left-radius", "height", "border-bottom-width"]);
  if (rgba(tab["background-color"]!) !== rgba(v.selected) || tab["border-top-left-radius"] !== "8px" || tab.height !== "28px") bad.push(`tab recipe ${JSON.stringify(tab)}`);
  const composer = await probe(page, '<div class="cmp-box"></div>', ["border-top-left-radius"]);
  if (composer["border-top-left-radius"] !== `${GEOMETRY.composerRadius}px`) bad.push(`composer radius ${composer["border-top-left-radius"]}`);
  return bad;
}

/** What breaks the surface mapping of MWS-02 (empty when it holds): the list on sidebar; the conversation, the top bar and the panel on editor. */
async function surfaceDrift(page: Page): Promise<string[]> {
  const bad: string[] = [];
  const editor = squash(await resolved(page, "--color-panel", "background-color"));
  const sidebar = squash(await resolved(page, "--color-sidebar", "background-color"));
  const bg = (selector: string) => page.locator(selector).first().evaluate((el) => getComputedStyle(el).backgroundColor);
  if (squash(await bg('[data-testid="region-agents"]')) !== sidebar) bad.push(`the task list is on ${await bg('[data-testid="region-agents"]')}, not sidebar ${sidebar}`);
  if (squash(await bg(".shell-frame")) !== editor) bad.push(`the conversation is on ${await bg(".shell-frame")}, not editor ${editor}`);
  // The top bar and the content draw nothing over the frame's surface.
  for (const sel of ['[data-testid="top-bar"]', '[data-testid="shell-content"]']) if (!clear(await bg(sel)) && squash(await bg(sel)) !== editor) bad.push(`${sel} is on ${await bg(sel)}, not editor ${editor}`);
  if ((await page.getByTestId("region-apps").count()) === 1 && squash(await bg('[data-testid="region-apps"]')) !== editor) bad.push(`the apps panel is on ${await bg('[data-testid="region-apps"]')}, not editor ${editor}`);
  return bad;
}

/** What breaks the approved home and agent list (empty when they hold). */
async function homeDrift(page: Page): Promise<string[]> {
  const bad: string[] = [];
  for (const id of REMOVED) if ((await page.getByTestId(id).count()) !== 0) bad.push(`removed surface ${id} is back`);
  const col = await box(page, "goal");
  const center = await box(page, "region-conversation");
  const window = await page.evaluate(() => innerHeight);
  if (Math.abs(col.x + col.width / 2 - (center.x + center.width / 2)) > 4) bad.push("the home composer is not centred horizontally");
  if (col.y < window * 0.2) bad.push(`the home composer is not in the middle of the window: y ${col.y} of ${window}`);
  // MWS-04: the task list reads top to bottom: the head, the navigation rows, the Tasks head, the groups, the person.
  const y = async (id: string) => (await box(page, id)).y;
  const order = [await y("toggle-agent-list"), ...(await Promise.all(NAV.map(y))), await y("agents-head"), await y("agents-scroll"), await y("agent-foot")];
  if (order.some((v, i) => i > 0 && v <= order[i - 1]!)) bad.push(`agent list order ${order.join(" ")}`);
  if (Math.abs((await box(page, "agent-foot")).height - GEOMETRY.listFoot) > 1) bad.push(`the person's row is ${(await box(page, "agent-foot")).height} high`);
  for (const id of ["core-status", "open-help", "open-settings"]) if ((await page.getByTestId("agent-foot").getByTestId(id).count()) !== 1) bad.push(`${id} is not in the person's row`);
  const foot = await box(page, "agent-foot");
  if (Math.abs(foot.y + foot.height - window) > 1) bad.push("the person's row is not at the foot of the list");
  if ((await page.getByTestId("agents-controls").count()) !== 0) bad.push("the list's search and filters show before they are asked for");
  return bad;
}

/** What breaks the approved conversation, composer and apps panel in a density (empty when they hold). */
async function workspaceDrift(page: Page, density: Density = "comfortable"): Promise<string[]> {
  const bad: string[] = [];
  const m = densityMetrics(density);
  for (const id of REMOVED) if ((await page.getByTestId(id).count()) !== 0) bad.push(`removed surface ${id} is back`);
  // The top bar: title, the toggles, More; no toolbar under it. The state and the branch are the details rail's (MWS-09), so with More closed the bar holds neither.
  for (const id of ["top-bar-title", "task-details-toggle"]) if ((await page.getByTestId("top-bar").getByTestId(id).count()) !== 1) bad.push(`${id} is not in the top bar`);
  if ((await page.getByTestId("task-more").count()) === 0) for (const id of ["top-bar-state", "top-bar-branch"]) if ((await page.getByTestId("top-bar").getByTestId(id).count()) !== 0) bad.push(`${id} is in the top bar, not in the details rail`);
  if (await page.getByTestId("conv-density").isVisible()) bad.push("density is on the surface, not under More");
  // MWS-01: one column, 768 at most, with its text inset to 736: the composer takes the column, the message and the rows the
  // text's width, and both are centred on the same line. Where the centre is narrower than the text they are one width.
  const scroll = await box(page, "conv-scroll");
  const box_ = await box(page, "composer");
  const user = await box(page, "conv-user");
  if (box_.width > GEOMETRY.column + 1 || user.width > GEOMETRY.columnText + 1) bad.push(`column wider than ${GEOMETRY.column}: composer ${box_.width}, message ${user.width}`);
  if (Math.abs(user.width - Math.min(GEOMETRY.columnText, box_.width)) > 2 || Math.abs(user.x + user.width / 2 - (box_.x + box_.width / 2)) > 2) bad.push(`composer and message are not one column: ${box_.x}/${box_.width} and ${user.x}/${user.width}`);
  if (scroll.y + scroll.height > box_.y + 1) bad.push("the transcript runs under the composer");
  // MWS-07: the composer is one line (add context, the text, then stop or send) and, under the box at the right, the mode, the model, the run mode and the ring.
  bad.push(...(await composerDrift(page)));
  // A step is one line with its verb and icon.
  const steps = page.getByTestId("conv-group");
  for (let i = 0; i < (await steps.count()); i++) {
    const sum = await steps.nth(i).locator("summary").evaluate((el) => ({ text: el.textContent ?? "", icon: el.querySelector("svg") !== null }));
    if (/worked for/i.test(sum.text) || !sum.icon) bad.push(`step ${i} does not say what it did: ${sum.text}`);
  }
  // The pane dock (MWS-10): the panes tiled with heads in the window's top row at the bar's height (MWS-01: every pane head is the bar), no tab strip, the commit bar at the foot.
  const head = await page.getByTestId("apps-panel").locator(".pane-head").first().boundingBox();
  if (!head || head.y > 1 || Math.abs(head.height - m.topBar) > 1) bad.push(`pane head ${JSON.stringify(head)}`);
  bad.push(...(await dockDrift(page, m.topBar)));
  const changesHead = page.locator(".changes-head");
  if ((await changesHead.count()) === 1 && Math.abs((await changesHead.boundingBox())!.height - m.topBar) > 1) bad.push(`changes head ${(await changesHead.boundingBox())!.height}`);
  // MWS-03: the transcript's text at the density's size and line (15 on 21, 14 on 20 in Compact).
  const body = density === "compact" ? COMPACT.type.body : TYPE.body;
  const text = await page.getByTestId("conv-user").first().evaluate((el) => ({ size: getComputedStyle(el).fontSize, line: getComputedStyle(el).lineHeight }));
  if (text.size !== `${body.size}px` || Math.abs(parseFloat(text.line) - body.line) > 0.1) bad.push(`transcript text ${JSON.stringify(text)}`);
  const panel = await box(page, "apps-panel");
  if ((await page.getByTestId("apps-panel-task-title").count()) !== 0) bad.push("the panel repeats the task's title");
  if ((await page.getByTestId("app-changes").count()) === 1) {
    // MWS-10: the commit bar stands at the foot of the Changes pane (the whole panel where Changes is its only pane).
    const commit = await box(page, "changes-commit");
    const holder = (await page.getByTestId("pane-changes").count()) === 1 ? await box(page, "pane-changes") : panel;
    if (Math.abs(commit.y + commit.height - (holder.y + holder.height)) > 1) bad.push("the commit bar is not at the foot of the Changes pane");
    // One row: a note may sit under it (no git identity on this machine), and an approval replaces it.
    const row = page.locator(".changes-commit-actions");
    if ((await row.count()) === 1 && (await row.boundingBox())!.height > 40) bad.push(`the commit row is ${(await row.boundingBox())!.height} tall`);
    const line = page.locator(".hunk .dl").first();
    if ((await line.count()) === 1 && (await line.evaluate((el) => getComputedStyle(el).display)) !== "grid") bad.push("diff lines have no number columns");
  }
  return bad;
}

test("PX-192..194, PX-198: the agent workspace language on the real app: tokens, the measured geometry in both densities, the surface mapping, navigation, recipes and the approved home, and the gate fails on each superseded value", async () => {
  test.setTimeout(300_000);
  const { app, page } = await launch(mkdtempSync(join(tmpdir(), "modbit-e2e-awl-")));
  try {
    await setContentSize(app, page, 1280, 800);
    // Comfortable is the default: nothing chose it and the root carries no override.
    expect(await page.evaluate(() => document.documentElement.dataset.density ?? null), "the default density").toBeNull();
    for (const density of DENSITIES) {
      await setDensity(page, density);
      expect(await scaleDrift(page, density), `${density} scale`).toEqual([]);
      for (const theme of ["light", "dark"] as const) {
        await setTheme(page, theme);
        expect(await tokenDrift(page, theme), `${theme} tokens in ${density}`).toEqual([]);
        expect(await shellDrift(page, density), `${theme} shell in ${density}`).toEqual([]);
        expect(await surfaceDrift(page), `${theme} surfaces in ${density}`).toEqual([]);
        expect(await homeDrift(page), `${theme} home in ${density}`).toEqual([]);
      }
    }
    // A density changes sizes and nothing else: the colours are the theme's in both (checked above), and Compact is not Comfortable.
    expect(await shellDrift(page, "comfortable"), "Compact measured against the Comfortable numbers").not.toEqual([]);
    await setDensity(page, "comfortable");
    expect(await shellDrift(page, "compact"), "Comfortable measured against the Compact numbers").not.toEqual([]);
    // Each navigation entry still does what it did from its old place; the search is the Tasks head's.
    await page.getByTestId("open-customize").click();
    await expect(page.getByTestId("top-bar-title")).toHaveText("Customize");
    await page.getByTestId("open-customize").click();
    await page.getByTestId("open-inbox").click();
    await expect(page.getByTestId("top-bar-title")).toHaveText("Inbox");
    await expect(page.getByTestId("inbox")).toBeVisible();
    await page.getByTestId("open-inbox").click();
    await page.getByTestId("agents-search-open").click();
    await expect(page.getByRole("dialog")).toBeVisible();
    await page.keyboard.press("Escape");
    await page.getByTestId("open-help").click();
    await expect(page.getByTestId("help")).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(page.getByTestId("help")).toHaveCount(0);

    // At 1000 x 700 and the 900 x 600 minimum the geometry and recipes hold, in both densities and in light, dark and high contrast.
    await liftMinimumSize(app);
    for (const density of DENSITIES) {
      await setDensity(page, density);
      for (const [w, h] of [[1000, 700], [900, 600]] as const) {
        await setContentSize(app, page, w, h);
        for (const theme of ["light", "dark", "high-contrast"] as const) {
          await setTheme(page, theme);
          expect(await shellDrift(page, density), `${theme} shell at ${w} x ${h} in ${density}`).toEqual([]);
          expect(await surfaceDrift(page), `${theme} surfaces at ${w} x ${h} in ${density}`).toEqual([]);
        }
      }
      await setContentSize(app, page, 1280, 800);
      for (const theme of ["light", "dark", "high-contrast"] as const) {
        await setTheme(page, theme);
        expect(await shellDrift(page, density), `${theme} shell at 1280 x 800 in ${density}`).toEqual([]);
        await accessible(page, `shell in ${theme}, ${density}`, '[data-testid="shell"]');
      }
    }
    await setDensity(page, "comfortable");

    // The retained mutations (section 6 of both records): each restored value fails the gate, and removing it passes again.
    await setTheme(page, "light");
    const mutate = (css: string) => page.evaluate((c) => {
      const s = document.createElement("style");
      s.id = "awl-mutation";
      s.textContent = c;
      document.head.appendChild(s);
    }, css);
    const restore = () => page.evaluate(() => document.getElementById("awl-mutation")?.remove());
    await mutate(':root[data-theme="light"] { --color-primary: #000000; }');
    expect((await tokenDrift(page, "light")).some((d) => d.startsWith("--color-primary:")), "a ZDL primary fails the token check").toBe(true);
    await restore();
    await mutate(":root { --mb-top-bar: 48px; }");
    expect((await shellDrift(page)).some((d) => d.startsWith("top bar")), "a 48 px top bar fails the geometry check").toBe(true);
    expect((await scaleDrift(page, "comfortable")).some((d) => d.startsWith("--mb-top-bar:")), "a 48 px top bar fails the scale check").toBe(true);
    await restore();
    // DR-PX-2026-11-11-021 section 6: the superseded numbers of DR-PX-2026-11-03-019, each on its own surface.
    await mutate('[data-testid="region-agents"] { width: 260px !important; }');
    expect((await shellDrift(page)).some((d) => d.startsWith("agent list 260")), "a list at 260 fails").toBe(true);
    await restore();
    await mutate(":root { --mb-top-bar: 40px; }");
    expect((await shellDrift(page)).filter((d) => d.startsWith("top bar 40") || d.startsWith("list head 40")).length, "a bar at 40 fails in Comfortable, on the top bar and the list's head").toBe(2);
    await restore();
    await mutate(":root { --mb-row: 28px; }");
    expect((await shellDrift(page)).filter((d) => d.startsWith("navigation row")).length, "navigation rows at 28 fail, every one").toBe(NAV.length);
    await restore();
    await mutate(".agent-nav { gap: 1px !important; }");
    expect((await shellDrift(page)).some((d) => d.includes("from the row above")), "rows 1 apart fail").toBe(true);
    await restore();
    await mutate(":root { --ui-font-size: 13px; }");
    expect((await shellDrift(page)).some((d) => d.startsWith("chrome text")), "13 px chrome text fails in Comfortable").toBe(true);
    await restore();
    await mutate(".shell-frame { background: var(--color-background) !important; }");
    expect((await surfaceDrift(page)).some((d) => d.startsWith("the conversation is on")), "the conversation back on chrome fails").toBe(true);
    await restore();
    await mutate(".cmp-box { border-radius: 12px !important; }");
    expect((await shellDrift(page)).some((d) => d.startsWith("composer radius 12px")), "a 12 px composer fails").toBe(true);
    await restore();
    await mutate(":where(button:not(.mb-btn)) { background: #000000 !important; }");
    expect((await shellDrift(page)).some((d) => d.startsWith("bare button")), "a black bare button fails the recipe check").toBe(true);
    await restore();
    await mutate(".home-col { margin: 0; }");
    expect((await homeDrift(page)).some((d) => d.includes("composer is not")), "a composer pushed into the corner fails the home check").toBe(true);
    await restore();
    expect(await shellDrift(page), "clean again").toEqual([]);
    expect(await surfaceDrift(page), "clean again").toEqual([]);
    expect(await homeDrift(page), "clean again").toEqual([]);
  } finally {
    await closeApp(app);
  }
});

/** Waits until no CSS transition is running (an opacity read mid-fade is not the state). */
async function settled(page: Page): Promise<void> {
  await page.evaluate(() => new Promise<void>((done) => requestAnimationFrame(() => requestAnimationFrame(() => done()))));
  await page.evaluate(async () => {
    for (let round = 0; round < 8; round++) {
      const running = document.getAnimations().filter((a) => a instanceof CSSTransition);
      if (running.length === 0) return;
      await Promise.all(running.map((a) => a.finished.catch(() => undefined)));
    }
  });
}

/** What the conversation and composer break of AWL-07..08 (empty when the record holds). */
async function densityDrift(page: Page): Promise<string[]> {
  const bad: string[] = [];
  // MWS-02: the person's message is a chrome fill on the conversation's editor surface (it was an editor bubble on chrome).
  const fill = await resolved(page, "--color-background", "background-color");
  const user = await page.getByTestId("conv-user").first().evaluate((el) => {
    const cs = getComputedStyle(el);
    return { bg: cs.backgroundColor, radius: cs.borderTopLeftRadius };
  });
  if (user.bg !== fill || user.radius !== "12px") bad.push(`user bubble ${JSON.stringify(user)} (chrome ${fill})`);

  // A step group is one line with no box.
  const groups = page.getByTestId("conv-group");
  if ((await groups.count()) === 0) bad.push("no step group in Compact");
  for (let i = 0; i < (await groups.count()); i++) {
    const g = await groups.nth(i).evaluate((el) => {
      const cs = getComputedStyle(el);
      const sum = el.querySelector("summary")!;
      return { open: (el as HTMLDetailsElement).open, h: el.getBoundingClientRect().height, sum: sum.getBoundingClientRect().height, border: cs.borderTopWidth, bg: cs.backgroundColor, radius: cs.borderTopLeftRadius };
    });
    if (g.sum > 24.5 || (!g.open && g.h > 24.5) || g.border !== "0px" || !clear(g.bg) || g.radius !== "0px") bad.push(`step group ${i} ${JSON.stringify(g)}`);
  }

  // A completed turn's footer: transparent at rest, shown on hover and on keyboard focus, in the tab order and the accessibility tree.
  const footer = page.locator('[data-testid="conv-footer"][data-outcome="COMPLETED"]').last();
  if ((await footer.count()) === 0) bad.push("no completed turn footer");
  else {
    await page.mouse.move(1, 1);
    await page.locator("body").evaluate(() => (document.activeElement as HTMLElement | null)?.blur());
    await settled(page);
    const rest = await footer.evaluate((el) => getComputedStyle(el).opacity);
    if (rest !== "0") bad.push(`footer at rest has opacity ${rest}`);
    const lines = await footer.evaluate((el) => ({ h: el.getBoundingClientRect().height, ws: getComputedStyle(el).whiteSpace }));
    if (lines.h > 28 || lines.ws !== "nowrap") bad.push(`footer is not one line ${JSON.stringify(lines)}`);
    await footer.hover();
    await settled(page);
    const hovered = await footer.evaluate((el) => getComputedStyle(el).opacity);
    if (hovered !== "1") bad.push(`footer on hover has opacity ${hovered}`);
    await page.mouse.move(1, 1);
    const copy = footer.getByRole("button", { name: "Copy answer" });
    if ((await copy.count()) !== 1) bad.push("the footer's Copy answer is not in the accessibility tree");
    else {
      const tab = await copy.evaluate((el) => ({ index: (el as HTMLElement).tabIndex, vis: getComputedStyle(el).visibility, disp: getComputedStyle(el).display }));
      if (tab.index < 0 || tab.vis !== "visible" || tab.disp === "none") bad.push(`Copy answer is out of the tab order ${JSON.stringify(tab)}`);
      await copy.focus();
      await settled(page);
      const focused = await footer.evaluate((el) => getComputedStyle(el).opacity);
      if (focused !== "1") bad.push(`footer with keyboard focus has opacity ${focused}`);
      await copy.blur();
    }
  }

  // MWS-01: the composer is a 14 px box.
  const cmp = await page.locator(".cmp-box").first().evaluate((el) => getComputedStyle(el).borderTopLeftRadius);
  if (cmp !== `${GEOMETRY.composerRadius}px`) bad.push(`composer radius ${cmp}`);

  // Headings are sentence case: nothing is upper-cased by style and none is written in capitals.
  const headings = await page.locator("h1, h2, h3, h4, summary").evaluateAll((els) => els.filter((e) => (e as HTMLElement).offsetParent !== null).map((e) => ({ text: (e.textContent ?? "").trim(), transform: getComputedStyle(e).textTransform })));
  for (const h of headings) {
    const words = h.text.split(/\s+/).filter((w) => /^[A-Za-z]{4,}$/.test(w));
    if (h.transform !== "none" || words.some((w) => w === w.toUpperCase())) bad.push(`heading not sentence case: ${JSON.stringify(h)}`);
  }

  return bad;
}

/**
 * What the composer breaks of MWS-07 (empty when it holds): with nothing typed the box is one line holding add context, the
 * text and stop or send side by side; the mode, the model, the run mode and the context ring are under the box, in its right half.
 */
async function composerDrift(page: Page): Promise<string[]> {
  const bad: string[] = [];
  const outer = await box(page, "composer");
  const input = await box(page, "composer-input");
  const add = await box(page, "add-context");
  const endId = (await page.getByTestId("composer-send").count()) === 1 ? "composer-send" : "composer-stop";
  const end = await box(page, endId);
  const overlapsRow = (b: { y: number; height: number }) => b.y < input.y + input.height - 1 && b.y + b.height > input.y + 1;
  if (!overlapsRow(add) || !overlapsRow(end)) bad.push(`composer: not one line (add context ${add.y}, text ${input.y} to ${input.y + input.height}, ${endId} ${end.y})`);
  if (add.x + add.width > input.x + 1 || end.x < input.x + input.width - 1) bad.push("composer: add context, the text, then stop or send are not in that order");
  const lineHeight = await page.getByTestId("composer-input").evaluate((el) => parseFloat(getComputedStyle(el).lineHeight));
  if ((await page.getByTestId("composer-input").inputValue()) === "" && (input.height > lineHeight + 12 || outer.height > lineHeight + 30)) bad.push(`composer: an empty box is ${outer.height} high with a ${input.height} text area: more than one line`);
  if (Math.abs(end.width - 28) > 1 || Math.abs(end.height - 28) > 1) bad.push(`composer: ${endId} is ${end.width} x ${end.height}`);
  // Under the box and at its right: a row that ends at the box's right edge (a narrow centre has no room for "the right half"). Where the task runs is the details rail's (MWS-09), not this bar's.
  if ((await page.getByTestId("composer-foot").getByTestId("conv-location").count()) !== 0) bad.push("composer: where the task runs is under the box, not in the details rail");
  let last = 0;
  for (const id of ["mode-chip", "model-chip", "runmode-button", "context-ring"]) {
    const b = await page.getByTestId("composer-foot").getByTestId(id).boundingBox();
    if (!b) bad.push(`composer: ${id} is not in the bar under the box`);
    else {
      if (b.y < outer.y + outer.height - 1) bad.push(`composer: ${id} is not under the box (${b.y}; the box ends at ${outer.y + outer.height})`);
      last = Math.max(last, b.x + b.width);
    }
  }
  if (Math.abs(outer.x + outer.width - last) > 12) bad.push(`composer: the controls are not under the box at the right (they end at ${last}, the box at ${outer.x + outer.width})`);
  if ((await page.getByTestId("composer").locator('[data-testid="mode-chip"], [data-testid="model-chip"], [data-testid="context-ring"]').count()) !== 0) bad.push("composer: a control of the bar is inside the box");
  return bad;
}

/** What the docked approval card breaks of MWS-08 (empty when it holds): no native select, one disclosure, and the decisions in their order. */
async function approvalDrift(page: Page): Promise<string[]> {
  const bad: string[] = [];
  const card = page.getByTestId("approval-card");
  const selects = await card.locator("select").count();
  if (selects !== 0) bad.push(`approval: ${selects} native select${selects === 1 ? "" : "s"} in the card`);
  if ((await card.locator("[aria-expanded]:not([aria-haspopup])").count()) !== 1) bad.push("approval: not exactly one disclosure");
  if ((await card.getByTestId("approval-intent-hash").count()) !== 1) bad.push("approval: the intent hash is not in the card");
  const title = (await card.getByTestId("approval-title").textContent()) ?? "";
  const tool = (await card.getByTestId("approval-tool").textContent()) ?? "";
  if (tool && title.includes(tool)) bad.push(`approval: the tool's own name (${tool}) is in the headline`);
  const skip = await card.getByTestId("approval-skip").boundingBox();
  const run = await card.getByTestId("approval-run").boundingBox();
  if (!skip || !run || skip.x >= run.x || Math.abs(skip.y + skip.height / 2 - (run.y + run.height / 2)) > 2) bad.push("approval: Skip and Allow once are not one row, in that order");
  return bad;
}

/**
 * What breaks the details rail of MWS-09 and section 5.1 (empty when it holds). Where the task area is wider than 800 and no pane is
 * open: the rail is 320 wide and never under 280, on the editor surface with a 1 px edge at 8 %, beside the conversation and
 * inside the task area, and nothing of it is over the composer. Where the task area is 800 or narrower, or a pane is open, there
 * is no rail and no Details toggle to ask for one that cannot be shown. The details popover is gone everywhere.
 */
async function railDrift(page: Page): Promise<string[]> {
  const bad: string[] = [];
  const shell = page.getByTestId("shell");
  const area = Number(await shell.getAttribute("data-center-px"));
  const paneOpen = (await page.getByTestId("region-apps").count()) === 1 || (await shell.getAttribute("data-stacked")) === "true";
  // The popover: a dialog of the task's details, anywhere, or the state and branch back in the title bar.
  if ((await page.locator('[role="dialog"][data-testid="task-details"], .task-details').count()) !== 0) bad.push("rail: the details popover is back");
  const rails = await page.getByTestId("details-rail").count();
  if (area <= 800 || paneOpen) {
    if (rails !== 0) bad.push(`rail: present at a task area of ${area}${paneOpen ? " with a pane open" : ""}`);
    // With a pane open the toggle may stand, not pressed: asking for the rail closes the pane. Where the rail cannot be shown at all it is not offered.
    if (paneOpen && (await page.getByTestId("details-toggle").count()) === 1 && (await page.getByTestId("details-toggle").getAttribute("aria-pressed")) !== "false") bad.push("rail: its toggle says shown while a pane is open");
    if (!paneOpen && (await page.getByTestId("details-toggle").count()) !== 0) bad.push(`rail: the Details toggle is offered at a task area of ${area}, where the rail cannot be shown`);
    return bad;
  }
  const on = (await page.getByTestId("details-toggle").getAttribute("aria-pressed")) === "true";
  if (!on) return rails === 0 ? bad : [...bad, "rail: shown while its toggle says hidden"];
  if (rails !== 1) return [...bad, `rail: ${rails} rails at a task area of ${area}`];
  const rail = await box(page, "details-rail");
  const style = await page.getByTestId("details-rail").evaluate((el) => {
    const cs = getComputedStyle(el);
    return { min: cs.minWidth, bg: cs.backgroundColor, edge: cs.borderLeftWidth, edgeColor: cs.borderLeftColor, edgeStyle: cs.borderLeftStyle };
  });
  if (Math.abs(rail.width - 320) > 0.5) bad.push(`rail: ${rail.width} wide, not 320`);
  if (style.min !== "280px") bad.push(`rail: its minimum is ${style.min}, not 280px`);
  if (squash(style.bg) !== squash(await resolved(page, "--color-panel", "background-color"))) bad.push(`rail: on ${style.bg}, not the editor surface`);
  if (style.edge !== "1px" || style.edgeStyle !== "solid" || squash(style.edgeColor) !== squash(await resolved(page, "--color-tab-border", "border-top-color"))) bad.push(`rail: its edge is ${style.edge} ${style.edgeStyle} ${style.edgeColor}`);
  const centre = await box(page, "region-conversation");
  const conversation = await box(page, "conversation");
  const bar = await box(page, "top-bar");
  if (Math.abs(rail.x + rail.width - (centre.x + centre.width)) > 1) bad.push("rail: not at the right edge of the task area");
  if (Math.abs(rail.x - (conversation.x + conversation.width)) > 1) bad.push(`rail: not beside the conversation (the conversation ends at ${conversation.x + conversation.width}, the rail starts at ${rail.x})`);
  if (rail.y < bar.y + bar.height - 1) bad.push("rail: over the title bar");
  const composer = await box(page, "composer");
  if (composer.x + composer.width > rail.x + 1) bad.push(`rail: over the composer (the composer ends at ${composer.x + composer.width}, the rail starts at ${rail.x})`);
  if (conversation.width < GEOMETRY.centerMin) bad.push(`rail: the conversation beside it is ${conversation.width} wide, under the centre's minimum`);
  // The sections stand in the record's order with a hairline between them.
  const sections = await page.getByTestId("details-rail").locator("section").evaluateAll((els) => els.map((e) => ({ id: e.getAttribute("data-testid") ?? "", line: getComputedStyle(e).borderTopWidth })));
  const order = ["rail-environment", "rail-approvals", "rail-context", "rail-tool-servers", "rail-changes", "rail-evidence", "rail-background", "rail-cost"];
  const got = sections.map((s) => s.id);
  if (got.join(" ") !== order.filter((id) => got.includes(id)).join(" ") || got.some((id) => !order.includes(id))) bad.push(`rail: sections are ${got.join(" ")}`);
  for (const id of ["rail-environment", "rail-approvals", "rail-context"]) if (!got.includes(id)) bad.push(`rail: no ${id}`);
  if (sections.some((s) => s.line !== "1px")) bad.push("rail: a section has no hairline above it");
  for (const id of ["top-bar-state", "rail-details-toggle"]) if ((await page.getByTestId("details-rail").getByTestId(id).count()) !== 1) bad.push(`rail: ${id} is not in the rail's head`);
  return bad;
}

/** What breaks the conversation's column where the centre has room for it (MWS-01): the composer 768, its text 736, inset 16 on each side. */
async function columnDrift(page: Page): Promise<string[]> {
  const bad: string[] = [];
  // The conversation itself: the task area less the details rail, where the rail is shown (MWS-09).
  const centre = await box(page, "conversation");
  if (centre.width < GEOMETRY.column + 48) return [`the conversation is ${centre.width} wide: too narrow to measure the column`];
  const composer = await box(page, "composer");
  const user = await box(page, "conv-user");
  if (Math.abs(composer.width - GEOMETRY.column) > 1) bad.push(`column: the composer is ${composer.width}, not ${GEOMETRY.column}`);
  if (Math.abs(user.width - GEOMETRY.columnText) > 1) bad.push(`column: the text is ${user.width}, not ${GEOMETRY.columnText}`);
  const inset = (GEOMETRY.column - GEOMETRY.columnText) / 2;
  if (Math.abs(user.x - composer.x - inset) > 1.5 || Math.abs(composer.x + composer.width - (user.x + user.width) - inset) > 1.5) bad.push(`column: the text is inset ${user.x - composer.x} and ${composer.x + composer.width - (user.x + user.width)}, not ${inset}`);
  if (Math.abs(composer.x + composer.width / 2 - (centre.x + centre.width / 2)) > 8) bad.push("column: the composer is not centred in the conversation");
  return bad;
}

test("PX-194, PX-198: a scripted task with tool steps, an edit, a terminal and an approval shows the approved workspace in light and dark and in both densities, keeps the footer reachable, and the gate fails on a restored value", async () => {
  test.setTimeout(300_000);
  const repo = makeRepo(mkdtempSync(join(tmpdir(), "modbit-awl-repo-")));
  mkdirSync(`${repo}.modbit-worktrees`, { recursive: true });
  const wt = join(`${repo}.modbit-worktrees`, "held");
  git(repo, "worktree", "add", "-q", "-b", "task/held", wt);
  const wtReal = realpathSync.native(wt).replace(/^\\\\\?\\/, "");
  const model = await streamingModel([
    { parts: ["Looking at the notes first.", { call: { name: "plan.update", args: { outcome: "edit the notes and close the held worktree", expected_files: ["notes.txt"], protected_effects: ["git.worktree.close"] } } }] },
    { parts: [{ call: { name: "fs.read", args: { path: "notes.txt" } } }] },
    { parts: [{ call: { name: "change.apply", args: { path: "notes.txt", op: "replace", content: "line 1\nline 2 changed\nline 3\n" } } }] },
    { parts: [{ call: nodeTerminal("console.log('finished')") }] },
    { parts: [{ call: { name: "git.worktree.close", args: { path: wtReal } } }] },
    { parts: ["Edited the notes and closed the held worktree.", { call: { name: "task.complete", args: { summary: "edited and closed", self_review: { findings: [] } } } }] },
  ]);
  const { app, page } = await launch(mkdtempSync(join(tmpdir(), "modbit-e2e-awl-density-")), { env: { MODBIT_OPENAI_BASE_URL: model.url } });
  try {
    await setContentSize(app, page, 1280, 800);
    // The honest language labels are in Settings, whole.
    await gearMenu(page, "settings.open");
    await expect(page.getByTestId("language-labels")).toContainText("Alpha baseline", { timeout: 15_000 });
    await expect(page.getByTestId("language-labels")).toContainText("everything else unsupported");
    await page.getByTestId("nav-back").click();
    await setTheme(page, "light");

    // The run: plan, read, edit, a terminal, then a protected effect that waits for the docked approval.
    const taskId = await runTask(page, "Edit the notes and close the held worktree", repo);
    await expect(page.getByTestId("conversation")).toHaveAttribute("data-loaded", "true");
    await expect(page.getByTestId("approval-card")).toBeVisible({ timeout: 90_000 });
    // One decision surface: the approval is decided on the docked card, the width of the composer, and not again in the transcript.
    await expect(page.getByTestId("conv-approve")).toHaveCount(0);
    // MWS-08: the card has no native select and one disclosure; the retained mutation (two selects in the card, as before this record) fails.
    expect(await approvalDrift(page), "the docked approval").toEqual([]);
    await page.getByTestId("approval-card").evaluate((el) => {
      for (const id of ["awl-select-1", "awl-select-2"]) {
        const sel = document.createElement("select");
        sel.id = id;
        sel.setAttribute("aria-label", id);
        el.appendChild(sel);
      }
    });
    expect(await approvalDrift(page), "two selects in the card fail").toEqual(["approval: 2 native selects in the card"]);
    await page.evaluate(() => document.querySelectorAll("#awl-select-1, #awl-select-2").forEach((el) => el.remove()));
    expect(await approvalDrift(page), "clean again").toEqual([]);
    const docked = await box(page, "approval-card");
    const under = await box(page, "composer");
    expect(Math.abs(docked.width - under.width), `approval card ${docked.width}, composer ${under.width}`).toBeLessThanOrEqual(4);
    expect(existsSync(wt)).toBe(true);
    await page.getByTestId("approval-run").click();
    await page.getByTestId("approval-confirm-run").click();
    await expect(page.getByTestId("approval-last")).toContainText("Approved", { timeout: 15_000 });
    // The run either goes on by itself or waits for the person to resume it; either way the approved effect runs once.
    const resume = page.getByTestId("conv-start");
    await expect(page.getByTestId("review").or(resume)).toBeVisible({ timeout: 90_000 });
    if (await resume.isVisible()) await resume.click();
    await expect(page.getByTestId("review")).toBeVisible({ timeout: 90_000 });
    expect(existsSync(wt)).toBe(false);
    await page.keyboard.press("Escape");
    await expect(page.getByTestId("review")).toHaveCount(0);

    // The finished conversation in Compact, with the Changes app open beside it.
    await expect(page.getByTestId("conversation")).toHaveAttribute("data-task-id", taskId);
    await openTaskMore(page);
    await page.getByTestId("conv-density").selectOption("COMPACT");
    await page.getByTestId("task-details-toggle").click();
    await expect(page.getByTestId("task-more")).toHaveCount(0);
    await expect(page.getByTestId("conv-group").first()).toBeVisible();
    await expect(page.locator('[data-testid="conv-footer"][data-outcome="COMPLETED"]').first()).toBeAttached();
    await page.getByTestId("pane-toggle-changes").click();
    await expect(page.getByTestId("pane-changes")).toBeVisible();
    await expect(page.getByTestId("app-changes")).toBeVisible({ timeout: 30_000 });
    for (const density of DENSITIES) {
      await setDensity(page, density);
      await expect(page.getByTestId("conversation")).toHaveAttribute("data-task-id", taskId);
      for (const theme of ["light", "dark"] as const) {
        await setTheme(page, theme);
        expect(await densityDrift(page), `${theme} conversation in ${density}`).toEqual([]);
        expect(await workspaceDrift(page, density), `${theme} workspace in ${density}`).toEqual([]);
        expect(await surfaceDrift(page), `${theme} surfaces in ${density}`).toEqual([]);
        await accessible(page, `the finished task in ${theme}, ${density}`, '[data-testid="shell"]');
      }
    }
    // The shell's density is not the conversation's: the conversation still says Compact, whichever the shell is in.
    await expect(page.getByTestId("conversation")).toHaveAttribute("data-density", "COMPACT");
    expect(await workspaceDrift(page, "comfortable"), "Compact measured against the Comfortable numbers").not.toEqual([]);
    await setDensity(page, "comfortable");

    // The retained mutation: each restored pre-AWL value fails the gate, and removing it passes again.
    await setTheme(page, "light");
    const mutate = (css: string) => page.evaluate((c) => {
      const el = document.createElement("style");
      el.id = "awl-mutation";
      el.textContent = c;
      document.head.appendChild(el);
    }, css);
    const restore = () => page.evaluate(() => document.getElementById("awl-mutation")?.remove());
    const fails = async (css: string, prefix: string) => {
      await mutate(css);
      const drift = await densityDrift(page);
      await restore();
      return drift.some((d) => d.startsWith(prefix));
    };
    expect(await fails('.conv-footer[data-outcome="COMPLETED"] { opacity: 1 !important; }', "footer at rest"), "an always-visible footer fails").toBe(true);
    expect(await fails(".conv-group { border: 1px solid red !important; }", "step group"), "a boxed step group fails").toBe(true);
    expect(await fails(".conv-user { background: var(--color-panel) !important; }", "user bubble"), "the message back on the editor surface fails").toBe(true);
    expect(await fails(".cmp-box { border-radius: 12px !important; }", "composer radius"), "a 12 px composer fails").toBe(true);
    expect(await fails(".apps-panel h2, .apps-panel h3, .conv-group > summary { text-transform: uppercase !important; }", "heading not sentence case"), "upper-cased headings fail").toBe(true);
    const failsWorkspace = async (css: string, needle: string) => {
      await mutate(css);
      const drift = await workspaceDrift(page);
      await restore();
      return drift.some((d) => d.includes(needle));
    };
    expect(await failsWorkspace(".conv-user { font-size: 14px !important; line-height: 22px !important; }", "transcript text"), "the old 14 on 22 transcript fails").toBe(true);
    expect(await failsWorkspace(".pane-head { height: 40px !important; }", "pane head"), "a 40 px pane head fails in Comfortable").toBe(true);
    // MWS-07 and section 6: the superseded two-row composer fails, in each of the ways it can come back.
    expect(await failsWorkspace(".cmp-line { flex-direction: column !important; align-items: stretch !important; }", "composer: not one line"), "a two-row composer (send under the text) fails").toBe(true);
    expect(await failsWorkspace(".cmp-one .cmp-textarea { min-height: 44px !important; }", "more than one line"), "the old two-line text area fails").toBe(true);
    expect(await failsWorkspace(".cmp-under { position: absolute !important; top: 0 !important; }", "is not under the box"), "the model, run mode and ring above the box fail").toBe(true);
    expect(await failsWorkspace(".cmp-under { flex-direction: row-reverse !important; }", "not under the box at the right"), "the controls at the left fail").toBe(true);
    expect(await failsWorkspace(".apps-panel { padding-top: 40px !important; }", "pane head"), "pane heads pushed down fail").toBe(true);
    // MWS-10 and section 6: the superseded apps panel (one app at a time behind a tab strip) fails the dock's gate, as does a pane under its minimum.
    await page.getByTestId("pane-toggle-files").click();
    await expect(page.getByTestId("pane-files")).toBeVisible();
    expect(await workspaceDrift(page), "two panes tiled").toEqual([]);
    expect(await failsWorkspace(".apps-panel .pane ~ .pane, .apps-panel .dock-col ~ .dock-col { display: none !important; }", "pane(s) open"), "one app at a time fails").toBe(true);
    expect(await failsWorkspace(".apps-panel .pane { max-height: 60px !important; min-height: 0 !important; }", "under 100"), "a pane under 100 px fails").toBe(true);
    await page.getByTestId("pane-close-files").click();
    await expect(page.getByTestId("pane-files")).toHaveCount(0);
    expect(await workspaceDrift(page), "clean again").toEqual([]);

    // MWS-09: while the pane is open the rail gives way, and the popover is gone.
    expect(await railDrift(page), "the rail with a pane open").toEqual([]);
    // MWS-01: with the panel closed and the window wide enough for the conversation beside its details rail, the column is 768 with its text at 736. The superseded columns fail.
    await page.getByTestId("pane-close-changes").click();
    await expect(page.getByTestId("region-apps")).toHaveCount(0);
    await setContentSize(app, page, 1440, 800);
    await expect(page.getByTestId("details-rail")).toBeVisible();
    for (const density of DENSITIES) {
      await setDensity(page, density);
      expect(await columnDrift(page), `the column in ${density}`).toEqual([]);
    }
    await setDensity(page, "comfortable");
    const failsColumn = async (css: string) => {
      await mutate(css);
      const drift = await columnDrift(page);
      await restore();
      return drift.filter((d) => d.startsWith("column:"));
    };
    expect(await failsColumn(":root { --mb-column: 640px; --mb-column-text: 640px; }"), "a column at 640 fails").toEqual([`column: the composer is 640, not ${GEOMETRY.column}`, `column: the text is 640, not ${GEOMETRY.columnText}`, "column: the text is inset 0 and 0, not 16"]);
    expect((await failsColumn(".conv-inner { max-width: 840px !important; } .cmp > * { max-width: 840px !important; }")).length, "AWL-07's 840 column fails").toBeGreaterThan(0);
    expect((await failsColumn(":root { --mb-column-text: 768px; }")).length, "text that is not inset fails").toBeGreaterThan(0);
    expect(await columnDrift(page), "clean again").toEqual([]);

    // MWS-09 and section 5.1: the rail is 320 beside the conversation in both densities and both themes, absent at a task area of 800, and the popover is gone.
    for (const density of DENSITIES) {
      await setDensity(page, density);
      for (const theme of ["light", "dark"] as const) {
        await setTheme(page, theme);
        expect(await railDrift(page), `${theme} rail in ${density}`).toEqual([]);
      }
      // Compact is the tokens: a rail row is the density's row height, and its text the chrome's size.
      const row = await page.getByTestId("details-rail").locator(".rail-row").first().evaluate((el) => ({ h: el.getBoundingClientRect().height, font: getComputedStyle(el).fontSize }));
      expect([row.h, row.font], `a rail row in ${density}`).toEqual([densityMetrics(density).row, `${density === "compact" ? COMPACT.type.chrome.size : TYPE.chrome.size}px`]);
    }
    await setDensity(page, "comfortable");
    await setTheme(page, "light");
    for (const theme of ["light", "dark", "high-contrast"] as const) {
      await setTheme(page, theme);
      await accessible(page, `the details rail in ${theme}`, '[data-testid="details-rail"]');
    }
    await setTheme(page, "light");
    const list = Number(await page.getByTestId("shell").getAttribute("data-list-px"));
    await setContentSize(app, page, list + 801, 800);
    await expect(page.getByTestId("details-rail")).toHaveCount(1);
    expect(await railDrift(page), "a task area of 801").toEqual([]);
    await setContentSize(app, page, list + 800, 800);
    await expect(page.getByTestId("details-rail")).toHaveCount(0);
    expect(await railDrift(page), "a task area of 800").toEqual([]);
    // The retained mutations, each failing: a rail drawn at 800, the popover back, and (wide again) a rail at another width, without its minimum, off the editor surface, without its edge, and over the composer.
    const inject = (html: string, into: string) => page.evaluate(([h, sel]) => {
      const host = document.createElement("div");
      host.id = "awl-rail-mutation";
      host.innerHTML = h!;
      document.querySelector(sel!)!.appendChild(host);
    }, [html, into]);
    const eject = () => page.evaluate(() => document.getElementById("awl-rail-mutation")?.remove());
    await inject('<aside data-testid="details-rail" aria-label="mutation" style="width:320px"></aside>', ".shell-main");
    expect(await railDrift(page), "a rail at a task area of 800 fails").toEqual(["rail: present at a task area of 800"]);
    await eject();
    await inject('<div role="dialog" aria-label="mutation" data-testid="task-details"></div>', '[data-testid="top-bar"]');
    expect(await railDrift(page), "the popover back fails").toEqual(["rail: the details popover is back"]);
    await eject();
    expect(await railDrift(page), "clean again at 800").toEqual([]);
    await setContentSize(app, page, 1440, 800);
    await expect(page.getByTestId("details-rail")).toHaveCount(1);
    const failsRail = async (css: string, needle: string) => {
      await mutate(css);
      const drift = await railDrift(page);
      await restore();
      return drift.some((d) => d.includes(needle));
    };
    expect(await failsRail(".details-rail { width: 260px !important; min-width: 0 !important; }", "wide, not 320"), "a 260 rail fails").toBe(true);
    expect(await failsRail(".details-rail { min-width: 0 !important; }", "its minimum is"), "a rail with no minimum fails").toBe(true);
    expect(await failsRail(".details-rail { background: var(--color-sidebar) !important; }", "not the editor surface"), "a rail on the sidebar surface fails").toBe(true);
    expect(await failsRail(".details-rail { border-left: 0 !important; }", "its edge is"), "a rail with no edge fails").toBe(true);
    expect(await failsRail(".details-rail { position: absolute !important; right: 0; top: 45px; bottom: 0; }", "over the composer"), "a rail floating over the composer fails").toBe(true);
    expect(await failsRail(".rail-sec { border-top: 0 !important; }", "no hairline"), "sections with no hairline fail").toBe(true);
    expect(await railDrift(page), "clean again").toEqual([]);
  } finally {
    await closeApp(app);
    model.server.close();
  }
});
