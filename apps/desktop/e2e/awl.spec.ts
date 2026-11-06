/**
 * The agent workspace language gate (DR-PX-2026-11-03-019 section 6; QUAL-PX-192..194), on the real Electron build against a
 * real Core: the emitted colour variables equal the record's tables in light and dark, the shell has the restored geometry and
 * unframed panes, the navigation sits in the agent list, the control recipes are the record's, and axe passes in three themes.
 * The screens are the agent workspace of DR-PX-2026-11-04-020: home is one composer, the removed Fleet surfaces stay gone, and
 * the conversation, composer and apps panel have the approved layout. Retained mutations show each check failing.
 */
import { expect, test, type Page } from "@playwright/test";
import { existsSync, mkdirSync, mkdtempSync, realpathSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { GEOMETRY, themeVariables } from "@modbit/design-tokens";
import { nodeTerminal } from "./support/apps-harness.ts";
import { gearMenu, openTaskDetails, runTask } from "./support/new-task.ts";
import { streamingModel } from "./support/stream-model.ts";
import { accessible, box, closeApp, git, launch, liftMinimumSize, makeRepo, setContentSize } from "./support/ui-harness.ts";

const NAV = ["agents-new-task", "agents-search-open", "open-automations", "open-inbox", "open-customize"];
/** What the rebuild removed: none of these may come back. */
const REMOVED = ["fleet", "fleet-empty", "fleet-loading", "search", "agents-fleet", "task-card", "column-needsAttention", "column-running", "column-waiting", "column-readyForReview", "column-completed", "column-failed", "status-row", "brand", "composer-suggestions", "language-labels", "platform-state"];

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

/** What the shell and the recipes break (empty when the record holds). */
async function shellDrift(page: Page): Promise<string[]> {
  const bad: string[] = [];
  const agents = await box(page, "region-agents");
  const center = await box(page, "region-conversation");
  if (Math.abs(agents.width - GEOMETRY.listDefault) > 2) bad.push(`agent list ${agents.width}`);
  const bar = await box(page, "top-bar");
  if (Math.abs(bar.height - 40) > 1) bad.push(`top bar ${bar.height}`);
  if (Math.abs(center.x - (agents.x + agents.width)) > 1) bad.push(`gap between the agent list and the conversation: ${center.x - agents.x - agents.width}`);
  const frame = await page.locator(".shell-frame").evaluate((el) => {
    const cs = getComputedStyle(el);
    return { radius: cs.borderTopLeftRadius, border: cs.borderTopWidth, pad: getComputedStyle(el.parentElement!).paddingTop };
  });
  if (frame.radius !== "0px" || frame.border !== "0px" || frame.pad !== "0px") bad.push(`framed pane ${JSON.stringify(frame)}`);
  for (const id of NAV) {
    const el = page.getByTestId("region-agents").getByTestId(id);
    if ((await el.count()) !== 1) {
      bad.push(`navigation entry ${id} is not in the agent list`);
      continue;
    }
    const h = (await el.boundingBox())!.height;
    if (Math.abs(h - 28) > 0.5) bad.push(`navigation row ${id} is ${h}`);
  }
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
      selected: getComputedStyle(document.documentElement).getPropertyValue("--color-selected").trim(),
      primary: resolve("--color-primary", "background-color"),
      control: resolve("--mb-color-border-control", "border-top-color"),
      muted: resolve("--mb-color-text-muted", "color"),
    };
  });
  const rgba = (c: string) => c.replace(/\s+/g, "");
  const clear = (c: string) => rgba(c) === "rgba(0,0,0,0)";
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
  if (composer["border-top-left-radius"] !== "12px") bad.push(`composer radius ${composer["border-top-left-radius"]}`);
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
  // The agent list reads top to bottom: controls, navigation, list, the person.
  const y = async (id: string) => (await box(page, id)).y;
  const order = [await y("toggle-agent-list"), await y("agents-new-task"), await y("agent-list"), await y("agent-foot")];
  if (order.some((v, i) => i > 0 && v <= order[i - 1]!)) bad.push(`agent list order ${order.join(" ")}`);
  const foot = await box(page, "agent-foot");
  if (Math.abs(foot.y + foot.height - window) > 1) bad.push("the person's row is not at the foot of the list");
  if ((await page.getByTestId("agents-controls").count()) !== 0) bad.push("the list's search and filters show before they are asked for");
  return bad;
}

/** What breaks the approved conversation, composer and apps panel (empty when they hold). */
async function workspaceDrift(page: Page): Promise<string[]> {
  const bad: string[] = [];
  for (const id of REMOVED) if ((await page.getByTestId(id).count()) !== 0) bad.push(`removed surface ${id} is back`);
  // The top bar: title, state, branch, overflow; no toolbar under it.
  for (const id of ["top-bar-title", "top-bar-state", "task-details-toggle"]) if ((await page.getByTestId("top-bar").getByTestId(id).count()) !== 1) bad.push(`${id} is not in the top bar`);
  if (await page.getByTestId("conv-density").isVisible()) bad.push("density is on the surface, not in the task's details");
  // One 640 px column; the message, the composer box and the column's rows share it.
  const scroll = await box(page, "conv-scroll");
  const box_ = await box(page, "composer");
  const user = await box(page, "conv-user");
  if (box_.width > 641 || user.width > 641) bad.push(`column wider than 640: composer ${box_.width}, message ${user.width}`);
  if (Math.abs(box_.x - user.x) > 2 || Math.abs(box_.width - user.width) > 2) bad.push(`composer and message are not one column: ${box_.x}/${box_.width} and ${user.x}/${user.width}`);
  if (scroll.y + scroll.height > box_.y + 1) bad.push("the transcript runs under the composer");
  // The composer: the text on top, then one bar with the mode, model and a round send; the location and run mode under the box.
  const input = await box(page, "composer-input");
  const send = await box(page, "composer-send");
  const mode = await box(page, "mode-chip");
  if (send.y < input.y + input.height - 1 || mode.y < input.y + input.height - 1) bad.push("the mode and send are not in a bar under the text");
  if (Math.abs(send.width - 24) > 1 || Math.abs(send.height - 24) > 1) bad.push(`send is ${send.width} x ${send.height}`);
  if (send.x <= mode.x) bad.push("send is not at the end of the bar");
  const foot = await box(page, "composer-foot");
  if (foot.y < box_.y + box_.height - 1) bad.push("the location and run mode are not under the box");
  // A step is one line with its verb and icon.
  const steps = page.getByTestId("conv-group");
  for (let i = 0; i < (await steps.count()); i++) {
    const sum = await steps.nth(i).locator("summary").evaluate((el) => ({ text: el.textContent ?? "", icon: el.querySelector("svg") !== null }));
    if (/worked for/i.test(sum.text) || !sum.icon) bad.push(`step ${i} does not say what it did: ${sum.text}`);
  }
  // The apps panel: tabs in the window's top row, a 40 px head under them, the commit bar at the foot.
  const tabs = await page.getByTestId("apps-panel").locator(".mb-tablist").boundingBox();
  if (!tabs || tabs.y > 1 || Math.abs(tabs.height - 40) > 1) bad.push(`panel tabs ${JSON.stringify(tabs)}`);
  const panel = await box(page, "apps-panel");
  if ((await page.getByTestId("apps-panel-task-title").count()) !== 0) bad.push("the panel repeats the task's title");
  if ((await page.getByTestId("app-changes").count()) === 1) {
    const commit = await box(page, "changes-commit");
    if (Math.abs(commit.y + commit.height - (panel.y + panel.height)) > 1) bad.push("the commit bar is not at the foot of the panel");
    // One row: a note may sit under it (no git identity on this machine), and an approval replaces it.
    const row = page.locator(".changes-commit-actions");
    if ((await row.count()) === 1 && (await row.boundingBox())!.height > 40) bad.push(`the commit row is ${(await row.boundingBox())!.height} tall`);
    const line = page.locator(".hunk .dl").first();
    if ((await line.count()) === 1 && (await line.evaluate((el) => getComputedStyle(el).display)) !== "grid") bad.push("diff lines have no number columns");
  }
  return bad;
}

test("PX-192..194: the agent workspace language on the real app: tokens, unframed geometry, navigation, recipes and the approved home, and the gate fails on a restored ZDL value", async () => {
  test.setTimeout(180_000);
  const { app, page } = await launch(mkdtempSync(join(tmpdir(), "modbit-e2e-awl-")));
  try {
    await setContentSize(app, page, 1280, 800);
    for (const theme of ["light", "dark"] as const) {
      await setTheme(page, theme);
      expect(await tokenDrift(page, theme), `${theme} tokens`).toEqual([]);
      expect(await shellDrift(page), `${theme} shell`).toEqual([]);
      expect(await homeDrift(page), `${theme} home`).toEqual([]);
    }
    // The type of AWL-04: chrome 13 on 18 at the default size.
    const chrome = await page.getByTestId("agents-summary").evaluate((el) => getComputedStyle(document.body).fontSize + " " + getComputedStyle(el).fontSize);
    expect(chrome.split(" ")[0]).toBe("13px");
    // Each navigation entry still does what it did from its old place.
    await page.getByTestId("open-customize").click();
    await expect(page.getByTestId("top-bar-title")).toHaveText("Customize");
    await page.getByTestId("open-customize").click();
    await page.getByTestId("agents-search-open").click();
    await expect(page.getByRole("dialog")).toBeVisible();
    await page.keyboard.press("Escape");

    // At 1000 x 700 and the 900 x 600 minimum the geometry and recipes hold, in both themes.
    await liftMinimumSize(app);
    for (const [w, h] of [[1000, 700], [900, 600]] as const) {
      await setContentSize(app, page, w, h);
      for (const theme of ["light", "dark"] as const) {
        await setTheme(page, theme);
        expect(await shellDrift(page), `${theme} shell at ${w} x ${h}`).toEqual([]);
      }
    }
    await setContentSize(app, page, 1280, 800);

    for (const theme of ["light", "dark", "high-contrast"] as const) {
      await setTheme(page, theme);
      await accessible(page, `shell in ${theme}`, '[data-testid="shell"]');
    }

    // The retained mutation (section 6): each restored ZDL value fails the gate, and removing it passes again.
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
    await restore();
    await mutate(":where(button:not(.mb-btn)) { background: #000000 !important; }");
    expect((await shellDrift(page)).some((d) => d.startsWith("bare button")), "a black bare button fails the recipe check").toBe(true);
    await restore();
    await mutate(".home-col { margin: 0; }");
    expect((await homeDrift(page)).some((d) => d.includes("composer is not")), "a composer pushed into the corner fails the home check").toBe(true);
    await restore();
    expect(await shellDrift(page), "clean again").toEqual([]);
    expect(await homeDrift(page), "clean again").toEqual([]);
  } finally {
    await closeApp(app);
  }
});

/** A CSS value resolved in the page (a variable written as hex compares with the computed rgb()). */
async function resolved(page: Page, name: string, prop: "background-color" | "color" | "font-size"): Promise<string> {
  return page.evaluate(([n, pr]) => {
    const el = document.createElement("i");
    el.style.setProperty(pr!, `var(${n})`);
    document.body.appendChild(el);
    const out = getComputedStyle(el).getPropertyValue(pr!);
    el.remove();
    return out;
  }, [name, prop] as const);
}

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
  const panel = await resolved(page, "--color-panel", "background-color");
  const clear = (c: string) => c.replace(/\s+/g, "") === "rgba(0,0,0,0)";

  // The user message is an editor-surface bubble.
  const user = await page.getByTestId("conv-user").first().evaluate((el) => {
    const cs = getComputedStyle(el);
    return { bg: cs.backgroundColor, radius: cs.borderTopLeftRadius };
  });
  if (user.bg !== panel || user.radius !== "12px") bad.push(`user bubble ${JSON.stringify(user)} (panel ${panel})`);

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

  // The composer is a 12 px box.
  const cmp = await page.locator(".cmp-box").first().evaluate((el) => getComputedStyle(el).borderTopLeftRadius);
  if (cmp !== "12px") bad.push(`composer radius ${cmp}`);

  // Headings are sentence case: nothing is upper-cased by style and none is written in capitals.
  const headings = await page.locator("h1, h2, h3, h4, summary").evaluateAll((els) => els.filter((e) => (e as HTMLElement).offsetParent !== null).map((e) => ({ text: (e.textContent ?? "").trim(), transform: getComputedStyle(e).textTransform })));
  for (const h of headings) {
    const words = h.text.split(/\s+/).filter((w) => /^[A-Za-z]{4,}$/.test(w));
    if (h.transform !== "none" || words.some((w) => w === w.toUpperCase())) bad.push(`heading not sentence case: ${JSON.stringify(h)}`);
  }

  return bad;
}

test("PX-194: a scripted task with tool steps, an edit, a terminal and an approval shows the approved workspace in light and dark, keeps the footer reachable, and the gate fails on a restored value", async () => {
  test.setTimeout(240_000);
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
    await openTaskDetails(page);
    await page.getByTestId("conv-density").selectOption("COMPACT");
    await page.getByTestId("task-details-toggle").click();
    await expect(page.getByTestId("conv-group").first()).toBeVisible();
    await expect(page.locator('[data-testid="conv-footer"][data-outcome="COMPLETED"]').first()).toBeAttached();
    await page.getByTestId("toggle-apps-panel").click();
    await page.getByTestId("tab-changes").click();
    await expect(page.getByTestId("app-changes")).toBeVisible({ timeout: 30_000 });
    for (const theme of ["light", "dark"] as const) {
      await setTheme(page, theme);
      expect(await densityDrift(page), `${theme} density`).toEqual([]);
      expect(await workspaceDrift(page), `${theme} workspace`).toEqual([]);
      await accessible(page, `the finished task in ${theme}`, '[data-testid="shell"]');
    }

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
    expect(await fails(".apps-panel h2, .apps-panel h3, .conv-group > summary { text-transform: uppercase !important; }", "heading not sentence case"), "upper-cased headings fail").toBe(true);
    const failsWorkspace = async (css: string, needle: string) => {
      await mutate(css);
      const drift = await workspaceDrift(page);
      await restore();
      return drift.some((d) => d.includes(needle));
    };
    // Wide enough that the column's cap is what holds it, not the window.
    await setContentSize(app, page, 1600, 900);
    expect(await failsWorkspace(".conv-inner { max-width: 840px !important; } .cmp > * { max-width: 840px !important; }", "wider than 640"), "the old 840 px column fails").toBe(true);
    await setContentSize(app, page, 1280, 800);
    expect(await failsWorkspace(".cmp-bar { position: absolute !important; top: 0 !important; }", "bar under the text"), "a bar above the text fails").toBe(true);
    expect(await failsWorkspace(".apps-panel { padding-top: 40px !important; }", "panel tabs"), "tabs pushed down fail").toBe(true);
    expect(await workspaceDrift(page), "clean again").toEqual([]);
  } finally {
    await closeApp(app);
    model.server.close();
  }
});
