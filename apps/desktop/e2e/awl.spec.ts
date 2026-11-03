/**
 * The agent workspace language gate (DR-PX-2026-11-03-019 section 6; QUAL-PX-192..194), on the real Electron build against a
 * real Core: the emitted colour variables equal the record's tables in light and dark, the shell has the restored geometry and
 * unframed panes, the navigation sits in the agent list, the control recipes are the record's, the status row is one line, and
 * axe passes in three themes. A retained mutation restores one ZDL value at a time and shows the checks failing each.
 */
import { expect, test, type Page } from "@playwright/test";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { GEOMETRY, themeVariables } from "@modbit/design-tokens";
import { accessible, box, closeApp, launch, liftMinimumSize, setContentSize } from "./support/ui-harness.ts";

const NAV = ["agents-new-task", "agents-search-open", "agents-fleet", "agents-worktrees", "open-automations", "open-customize", "open-dashboard"];

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
    if (Math.abs(h - 30) > 0.5) bad.push(`navigation row ${id} is ${h}`);
  }
  const v = await page.evaluate(() => {
    const cs = getComputedStyle(document.documentElement);
    return { selected: cs.getPropertyValue("--color-selected").trim(), primary: cs.getPropertyValue("--color-primary").trim() };
  });
  const rgba = (c: string) => c.replace(/\s+/g, "");
  const bare = await probe(page, "<button>Bare</button>", ["background-color", "border-top-color", "border-top-left-radius", "height"]);
  if (rgba(bare["background-color"]!) !== rgba(v.selected)) bad.push(`bare button fill ${bare["background-color"]} != ${v.selected}`);
  const primary = await probe(page, '<button class="mb-btn" data-variant="primary" data-size="md">Go</button>', ["background-color", "font-weight"]);
  if (primary["font-weight"] !== "500") bad.push(`primary weight ${primary["font-weight"]}`);
  const secondary = await probe(page, '<button class="mb-btn" data-variant="secondary" data-size="md">Do</button>', ["background-color", "border-top-color", "height", "border-top-left-radius"]);
  if (rgba(secondary["background-color"]!) !== rgba(v.selected)) bad.push(`secondary fill ${secondary["background-color"]}`);
  if (secondary.height !== "28px" || secondary["border-top-left-radius"] !== "6px") bad.push(`secondary geometry ${JSON.stringify(secondary)}`);
  const tab = await probe(page, '<button class="mb-tab" role="tab" aria-selected="true">Changes</button>', ["background-color", "border-top-left-radius", "height", "border-bottom-width"]);
  if (rgba(tab["background-color"]!) !== rgba(v.selected) || tab["border-top-left-radius"] !== "8px" || tab.height !== "28px") bad.push(`tab recipe ${JSON.stringify(tab)}`);
  const composer = await probe(page, '<div class="cmp-box"></div>', ["border-top-left-radius"]);
  if (composer["border-top-left-radius"] !== "12px") bad.push(`composer radius ${composer["border-top-left-radius"]}`);
  const row = page.getByTestId("status-row");
  const st = await row.evaluate((el) => ({ h: el.getBoundingClientRect().height, scroll: el.scrollHeight, client: el.clientHeight, ws: getComputedStyle(el).whiteSpace }));
  if (st.h > 29 || st.scroll > st.client + 1 || st.ws !== "nowrap") bad.push(`status row wraps ${JSON.stringify(st)}`);
  return bad;
}

test("PX-192..194: the agent workspace language on the real app: tokens, unframed geometry, navigation, recipes, one-line status row, and the gate fails on a restored ZDL value", async () => {
  test.setTimeout(180_000);
  const { app, page } = await launch(mkdtempSync(join(tmpdir(), "modbit-e2e-awl-")));
  try {
    await setContentSize(app, page, 1280, 800);
    for (const theme of ["light", "dark"] as const) {
      await setTheme(page, theme);
      expect(await tokenDrift(page, theme), `${theme} tokens`).toEqual([]);
      expect(await shellDrift(page), `${theme} shell`).toEqual([]);
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

    // At the 900 x 600 minimum the status row is still one line and the geometry holds.
    await liftMinimumSize(app);
    await setContentSize(app, page, 900, 600);
    await setTheme(page, "light");
    expect(await shellDrift(page), "light shell at 900").toEqual([]);
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
    expect(await shellDrift(page), "clean again").toEqual([]);
  } finally {
    await closeApp(app);
  }
});
