/**
 * QUAL-PX-216 / PX-E2E-216 (DR-PX-2026-11-11-021 MWS-19): the browser pane's site decisions, on the real Electron build with a
 * real Core, a real Chromium view and a local test site on two origins (two ports of 127.0.0.1).
 *
 * The agent's navigation to an origin nobody has decided asks in the pane:
 *  - Allow once permits that navigation and the next asks again;
 *  - Always allow persists across a restart of the app;
 *  - Deny refuses with its reason, is refused again on a retry without asking twice, and persists across a restart;
 *  - an answer for an origin other than the one the card shows is refused, and nothing is recorded;
 *  - a target the destination policy forbids is refused before any ask;
 *  - Settings lists the decisions and removes one, after which the origin asks again.
 *
 * The scripted model stands in for the provider only; the host, the Core, the page loads and the store are real. With
 * MODBIT_SHOTS_DIR set the same run writes the evidence screenshots. The decisions file is read back from disk to show that a
 * decision is the person's file and a use of Allow once is never written down.
 */
import { expect, test, type Page } from "@playwright/test";
import { createServer, type Server } from "node:http";
import { mkdirSync, mkdtempSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { gearMenu, showPane } from "./support/new-task.ts";
import { setContentSize } from "./support/ui-harness.ts";
import { closeApp, fixtureRepo, gate, launch, scriptedModel, startBrowserTask, toolResults } from "./support/browser-harness.ts";

const SHOTS = process.env.MODBIT_SHOTS_DIR ?? "";

function site(title: string): Promise<{ server: Server; origin: string; port: number }> {
  return new Promise((resolve) => {
    const server = createServer((_req, res) => {
      res.writeHead(200, { "content-type": "text/html; charset=utf-8" });
      res.end(`<!doctype html><html><head><title>${title}</title></head><body><main><h1>${title}</h1></main></body></html>`);
    });
    server.listen(0, "127.0.0.1", () => {
      const port = (server.address() as { port: number }).port;
      resolve({ server, origin: `http://127.0.0.1:${port}`, port });
    });
  });
}

/** The person's decisions as the file holds them (an Allow once is never in it). */
function decisionsOnDisk(dataDir: string): { origin: string; decision: string }[] {
  try {
    const j = JSON.parse(readFileSync(join(dataDir, "browser-sites.json"), "utf8")) as { sites: { origin: string; decision: string }[] };
    return j.sites.map((s) => ({ origin: s.origin, decision: s.decision }));
  } catch {
    return [];
  }
}

const hostLog = (page: Page) => page.evaluate(() => window.modbit.browserLog());
const kinds = async (page: Page, prefix: string): Promise<string[]> => (await hostLog(page)).filter((l) => l.kind.startsWith(prefix)).map((l) => `${l.kind} ${l.code}`);

async function setTheme(page: Page, theme: "light" | "dark"): Promise<void> {
  await page.evaluate((t) => document.documentElement.setAttribute("data-theme", t), theme);
  await page.evaluate(() => new Promise<void>((d) => requestAnimationFrame(() => requestAnimationFrame(() => d()))));
}

/** Screenshots of the running app in both themes (the theme is an attribute: the page and its open controls stay as they are). */
async function shots(page: Page, name: string): Promise<void> {
  if (!SHOTS) return;
  mkdirSync(SHOTS, { recursive: true });
  for (const theme of ["light", "dark"] as const) {
    await setTheme(page, theme);
    await page.waitForTimeout(400);
    await page.screenshot({ path: join(SHOTS, `${name}-${theme}.png`) });
  }
  await setTheme(page, "light");
}

/** What the person has told the agent so far, in the words the tool result carries. */
const code = (text: string): string => /SITE_PENDING|SITE_DENIED|TARGET_NOT_ALLOWED|status: SUCCESS/.exec(text)?.[0] ?? text.slice(0, 160);

test("QUAL-PX-216: the agent asks before opening an undecided site; Allow once is one navigation, Always allow and Deny persist across a restart, Settings removes a decision, and a forbidden target never asks", async () => {
  test.setTimeout(420_000);
  const a = await site("Site A");
  const b = await site("Site B");
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-px216-"));
  // The policy names loopback (so the destination policy lets these sites be reached) and no site: every origin is undecided.
  const env = { MODBIT_BROWSER_ALLOW_TARGETS: "127.0.0.1", MODBIT_BROWSER_ALLOW_SITES: "" };

  // ------------------------------------------------------------------ the first run
  const once = gate();
  const always = gate();
  const denied = gate();
  const wrongOrigin = gate();
  const model1 = await scriptedModel([
    { calls: [{ name: "plan.update", args: { outcome: "the sites are visited", expected_files: [] } }] },
    { calls: [{ name: "browser.navigate", args: { url: `${a.origin}/one` } }] }, // 1: undecided -> asks
    async () => (await once.opened, { calls: [{ name: "browser.navigate", args: { url: `${a.origin}/two` } }] }), // 2: Allow once -> loads
    { calls: [{ name: "browser.navigate", args: { url: `${a.origin}/three` } }] }, // 3: the once is used up -> asks again
    async () => (await always.opened, { calls: [{ name: "browser.navigate", args: { url: `${a.origin}/four` } }] }), // 4: Always allow -> loads
    { calls: [{ name: "browser.navigate", args: { url: `http://127.0.0.2:${a.port}/` } }] }, // 5: forbidden target -> refused before any ask
    // The Core parks a task after three turns in a row without progress, so a refused call is followed by one that loads.
    { calls: [{ name: "browser.navigate", args: { url: `${a.origin}/five` } }] }, // 6: allowed -> loads
    { calls: [{ name: "browser.navigate", args: { url: `${b.origin}/one` } }] }, // 7: undecided -> asks
    async () => (await wrongOrigin.opened, { calls: [{ name: "browser.navigate", args: { url: `${b.origin}/two` } }] }), // 8: still undecided -> the same ask
    { calls: [{ name: "browser.navigate", args: { url: `${a.origin}/six` } }] }, // 9: allowed -> loads
    async () => (await denied.opened, { calls: [{ name: "browser.navigate", args: { url: `${b.origin}/three` } }] }), // 10: Deny -> refused
    { calls: [{ name: "browser.navigate", args: { url: `${b.origin}/four` } }] }, // 11: a retry of a denied origin -> refused again, no ask
    { calls: [{ name: "task.complete", args: { summary: "visited", self_review: { findings: [] } } }] },
  ]);
  {
    const { app, page } = await launch(dataDir, { MODBIT_OPENAI_BASE_URL: model1.url, ...env });
    // The panes tile in the dock only in a window wide enough for them (a narrower one folds them under Apps); CI's default window is narrower than a desktop.
    await setContentSize(app, page, 1280, 800);
    try {
      const { bsid } = await startBrowserTask(page, fixtureRepo(), "visit the two sites");
      await showPane(page, "browser");
      const card = page.getByTestId("browser-site");

      // 1: nothing decided, so the agent is refused and the pane asks, naming the origin and the three answers.
      await toolResults(model1, 2);
      expect(code(model1.toolTexts[1]!)).toBe("SITE_PENDING");
      await expect(card).toHaveAttribute("data-origin", a.origin, { timeout: 30_000 });
      await expect(card).toContainText(`The agent wants to open ${a.origin}.`);
      for (const id of ["browser-site-deny", "browser-site-once", "browser-site-always"]) await expect(page.getByTestId(id)).toBeVisible();
      await shots(page, "browser-site-ask");
      expect(decisionsOnDisk(dataDir)).toEqual([]);

      // 2: Allow once lets that navigation through...
      await page.getByTestId("browser-site-once").click();
      await expect(card).toHaveCount(0);
      once.open();
      await toolResults(model1, 3);
      expect(code(model1.toolTexts[2]!)).toBe("status: SUCCESS");
      // ...and is not written down; the next navigation to the same origin asks again.
      expect(decisionsOnDisk(dataDir)).toEqual([]);
      await toolResults(model1, 4);
      expect(code(model1.toolTexts[3]!)).toBe("SITE_PENDING");
      await expect(card).toHaveAttribute("data-origin", a.origin, { timeout: 30_000 });

      // 3: an answer for another origin than the one shown is refused and records nothing; the card is still there.
      const described = await page.evaluate((id) => window.modbit.describeBrowser(id), bsid);
      expect(described?.siteAsk?.origin).toBe(a.origin);
      expect(await page.evaluate(([id, askId, o]) => window.modbit.decideBrowserSite(id!, askId!, o!, "allow_always"), [bsid, described!.siteAsk!.id, b.origin])).toBe(false);
      expect(await page.evaluate(([id, o]) => window.modbit.decideBrowserSite(id!, "not-the-ask", o!, "allow_always"), [bsid, a.origin])).toBe(false);
      expect(decisionsOnDisk(dataDir)).toEqual([]);
      await expect(card).toHaveAttribute("data-origin", a.origin);

      // 4: Always allow is the person's file, and the agent now loads the origin without asking.
      await page.getByTestId("browser-site-always").click();
      await expect(card).toHaveCount(0);
      expect(decisionsOnDisk(dataDir)).toEqual([{ origin: a.origin, decision: "allow" }]);
      always.open();
      await toolResults(model1, 5);
      expect(code(model1.toolTexts[4]!)).toBe("status: SUCCESS");

      // 5: a target the destination policy forbids is refused before any ask: no card, no ask on the host's log.
      await toolResults(model1, 6);
      expect(code(model1.toolTexts[5]!)).toBe("TARGET_NOT_ALLOWED");
      // (The agent has already gone on to the next site by now, so what is checked is the asks about the forbidden address and about A.)
      const asked = await kinds(page, "site-asked");
      expect(asked.filter((k) => k.includes("127.0.0.2"))).toEqual([]);
      expect(asked.filter((k) => k.endsWith(a.origin))).toHaveLength(2);

      // 6: the other origin is undecided, so it asks; the pane names that origin, not the first one.
      await toolResults(model1, 8);
      expect(code(model1.toolTexts[6]!)).toBe("status: SUCCESS");
      expect(code(model1.toolTexts[7]!)).toBe("SITE_PENDING");
      await expect(card).toHaveAttribute("data-origin", b.origin, { timeout: 30_000 });
      wrongOrigin.open();
      await toolResults(model1, 10);
      expect(code(model1.toolTexts[8]!)).toBe("SITE_PENDING");
      expect(code(model1.toolTexts[9]!)).toBe("status: SUCCESS");
      expect((await kinds(page, "site-asked")).filter((k) => k.endsWith(b.origin))).toHaveLength(1);

      // 7: Deny is recorded with the person's file, refuses with its reason and does not ask again on the retry.
      await page.getByTestId("browser-site-deny").click();
      await expect(card).toHaveCount(0);
      expect(decisionsOnDisk(dataDir)).toEqual([{ origin: a.origin, decision: "allow" }, { origin: b.origin, decision: "deny" }]);
      denied.open();
      await toolResults(model1, 12);
      expect(code(model1.toolTexts[10]!)).toBe("SITE_DENIED");
      expect(model1.toolTexts[10]).toContain("the person denied");
      expect(model1.toolTexts[10]).toContain("asking again changes nothing");
      expect(code(model1.toolTexts[11]!)).toBe("SITE_DENIED");
      await expect(card).toHaveCount(0);
      expect((await kinds(page, "site-asked")).filter((k) => k.endsWith(b.origin))).toHaveLength(1);
      expect(await kinds(page, "site-")).toContain(`site-allowed-once ${a.origin}`);
    } finally {
      once.open();
      always.open();
      denied.open();
      wrongOrigin.open();
      await closeApp(app);
      model1.server.close();
    }
  }

  // ------------------------------------------------------------------ after a restart
  const removed = gate();
  const model2 = await scriptedModel([
    { calls: [{ name: "plan.update", args: { outcome: "the sites are visited again", expected_files: [] } }] },
    { calls: [{ name: "browser.navigate", args: { url: `${a.origin}/five` } }] }, // 1: Always allow persisted -> loads without an ask
    { calls: [{ name: "browser.navigate", args: { url: `${b.origin}/five` } }] }, // 2: Deny persisted -> refused without an ask
    async () => (await removed.opened, { calls: [{ name: "browser.navigate", args: { url: `${a.origin}/six` } }] }), // 3: removed in Settings -> asks again
    { calls: [{ name: "task.complete", args: { summary: "visited again", self_review: { findings: [] } } }] },
  ]);
  {
    const { app, page } = await launch(dataDir, { MODBIT_OPENAI_BASE_URL: model2.url, ...env });
    // The panes tile in the dock only in a window wide enough for them (a narrower one folds them under Apps); CI's default window is narrower than a desktop.
    await setContentSize(app, page, 1280, 800);
    try {
      await startBrowserTask(page, fixtureRepo(), "visit the sites again");
      await toolResults(model2, 3);
      expect(code(model2.toolTexts[1]!)).toBe("status: SUCCESS");
      expect(code(model2.toolTexts[2]!)).toBe("SITE_DENIED");
      expect(await kinds(page, "site-asked")).toEqual([]);
      await showPane(page, "browser");
      await expect(page.getByTestId("browser-site")).toHaveCount(0);

      // Settings lists what the person decided, in plain words, and removes one.
      await gearMenu(page, "settings.open");
      await page.getByTestId("settings-nav-browser").click();
      const rows = page.getByTestId("settings-site");
      await expect(rows).toHaveCount(2, { timeout: 30_000 });
      await expect(page.locator(`[data-testid="settings-site"][data-origin="${a.origin}"]`)).toContainText("Always allowed");
      await expect(page.locator(`[data-testid="settings-site"][data-origin="${b.origin}"]`)).toContainText("Denied");
      await shots(page, "settings-sites");
      await page.locator(`[data-testid="settings-site"][data-origin="${a.origin}"]`).getByTestId("settings-site-remove").click();
      await expect(rows).toHaveCount(1);
      expect(decisionsOnDisk(dataDir)).toEqual([{ origin: b.origin, decision: "deny" }]);
      await shots(page, "settings-sites-removed");
      removed.open();

      // The removed origin asks again.
      await toolResults(model2, 4);
      expect(code(model2.toolTexts[3]!)).toBe("SITE_PENDING");
      await page.getByTestId("nav-back").click();
      await showPane(page, "browser");
      await expect(page.getByTestId("browser-site")).toHaveAttribute("data-origin", a.origin, { timeout: 30_000 });
      expect(await kinds(page, "site-asked")).toEqual([`site-asked ${a.origin}`]);
    } finally {
      removed.open();
      await closeApp(app);
      model2.server.close();
    }
  }

  // ------------------------------------------------------------------ and once more: what was removed stays removed, what was denied stays denied
  const model3 = await scriptedModel([
    { calls: [{ name: "plan.update", args: { outcome: "one more look", expected_files: [] } }] },
    { calls: [{ name: "browser.navigate", args: { url: `${b.origin}/six` } }] }, // 1: Deny persisted again
    { calls: [{ name: "browser.navigate", args: { url: `${a.origin}/seven` } }] }, // 2: the removed one asks
    { calls: [{ name: "task.complete", args: { summary: "done", self_review: { findings: [] } } }] },
  ]);
  {
    const { app, page } = await launch(dataDir, { MODBIT_OPENAI_BASE_URL: model3.url, ...env });
    // The panes tile in the dock only in a window wide enough for them (a narrower one folds them under Apps); CI's default window is narrower than a desktop.
    await setContentSize(app, page, 1280, 800);
    try {
      await startBrowserTask(page, fixtureRepo(), "one more look");
      await toolResults(model3, 3);
      expect(code(model3.toolTexts[1]!)).toBe("SITE_DENIED");
      expect(code(model3.toolTexts[2]!)).toBe("SITE_PENDING");
      expect(await kinds(page, "site-asked")).toEqual([`site-asked ${a.origin}`]);
    } finally {
      await closeApp(app);
      model3.server.close();
    }
  }
  a.server.close();
  b.server.close();
});
