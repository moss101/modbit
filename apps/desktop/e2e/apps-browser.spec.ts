/**
 * REQ-PX-048, the Browser tab, on the real stack: the packaged Electron app, a
 * real Core, a real `WebContentsView` for the task's browser session and a real
 * page served over HTTP. The tab shows who holds the control lease and its
 * generation, takeover from the tab moves the lease in the Core and blocks the
 * agent's input at once, and returning control lets the agent act again.
 * The tab never embeds the page: "Open browser" leads to the host's view.
 */
import { expect, test } from "@playwright/test";
import { createServer } from "node:http";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { accessible, MOD, setContentSize } from "./support/ui-harness.ts";
import { closeApp, fixtureRepo, gate, launch, refOf, scriptedModel, startBrowserTask, toolResults } from "./support/browser-harness.ts";

test("Browser tab: the lease badge follows the Core, Take control moves the lease and blocks the agent's input, Return control lets the agent act", async () => {
  test.setTimeout(180_000);
  const site = await new Promise<{ server: ReturnType<typeof createServer>; url: string }>((resolve) => {
    const server = createServer((_req, res) => {
      res.writeHead(200, { "content-type": "text/html; charset=utf-8" });
      res.end(`<!doctype html><html><head><title>Fixture Login</title></head><body><main><h1>Sign in to the fixture</h1><form aria-label="Login" action="/welcome" method="get"><label for="email">Email</label><input id="email" name="email" type="email"><button type="submit">Sign in</button></form></main></body></html>`);
    });
    server.listen(0, "127.0.0.1", () => resolve({ server, url: `http://127.0.0.1:${(server.address() as { port: number }).port}` }));
  });
  const first = gate();
  const second = gate();
  const model = await scriptedModel([
    { calls: [{ name: "plan.update", args: { outcome: "the email is filled", expected_files: [] } }] },
    { calls: [{ name: "browser.navigate", args: { url: `${site.url}/login` } }] },
    { calls: [{ name: "browser.snapshot", args: {} }] },
    async (texts) => {
      await first.opened;
      return { calls: [{ name: "browser.act", args: { ref: refOf(texts, "textbox", "Email"), action: "fill", value: "agent@example.test" } }] };
    },
    async (texts) => {
      await second.opened;
      return { calls: [{ name: "browser.act", args: { ref: refOf(texts, "textbox", "Email"), action: "fill", value: "agent@example.test" } }] };
    },
    { calls: [{ name: "task.complete", args: { summary: "filled", self_review: { findings: [] } } }] },
  ]);
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-apps-browser-"));
  const { app, page } = await launch(dataDir, { MODBIT_OPENAI_BASE_URL: model.url, MODBIT_BROWSER_ALLOW_TARGETS: "127.0.0.1" });
  try {
    await setContentSize(app, page, 1280, 800);
    const { taskId, bsid } = await startBrowserTask(page, fixtureRepo(), "fill the email");
    await toolResults(model, 3);
    expect(model.toolTexts[1]).toContain("status: SUCCESS");

    // The tab: the agent holds the lease (generation 1) and the page the agent loaded is named.
    const card = page.getByTestId("task-card").first();
    await card.focus();
    await page.keyboard.press(`${MOD}+e`);
    await expect(page.getByTestId("region-apps")).toBeVisible();
    await page.getByTestId("tab-browser").click();
    const tab = page.getByTestId("app-browser");
    await expect(tab).toContainText("Agent holds control", { timeout: 30_000 });
    await expect(page.getByTestId("browser-tab-lease")).toContainText("lease generation 1");
    await expect(page.getByTestId("browser-tab-page")).toContainText("/login");
    await accessible(page, "browser tab, the agent holds control", '[data-testid="region-apps"]');
    expect(await page.evaluate(([id, t]) => window.modbit.browserSession(id!, t!), [bsid, taskId])).toMatchObject({ controller: "AGENT", leaseGeneration: "1" });

    // Take control from the tab: the Core moves the lease; the tab says so.
    await page.getByTestId("browser-tab-take").click();
    await expect(tab).toContainText("You hold control", { timeout: 15_000 });
    await expect(page.getByTestId("browser-tab-lease")).toContainText("lease generation 2");
    await expect(page.getByTestId("browser-tab-return")).toBeVisible();
    await expect(page.getByTestId("browser-tab-take")).toHaveCount(0);
    expect(await page.evaluate(([id, t]) => window.modbit.browserSession(id!, t!), [bsid, taskId])).toMatchObject({ controller: "USER", leaseGeneration: "2" });
    await accessible(page, "browser tab, the person holds control", '[data-testid="region-apps"]');

    // The tab leads to the host's view of the same session, which says the same: the person holds it at generation 2.
    await page.getByTestId("panel-open-browser").click();
    await expect(page.getByTestId("browser")).toHaveAttribute("data-browser-session-id", bsid, { timeout: 30_000 });
    await expect(page.getByTestId("browser-control")).toHaveAttribute("data-controller", "USER");
    await expect(page.getByTestId("browser-control")).toHaveAttribute("data-lease-generation", "2");
    await page.getByTestId("browser-close").click();
    await expect(page.getByTestId("browser")).toHaveCount(0);
    await expect(page.getByTestId("app-browser")).toContainText("You hold control");

    // The agent's fill, released now, is blocked before it reaches the page.
    first.open();
    await toolResults(model, 4);
    expect(model.toolTexts[3]).toContain("HUMAN_ACTIVE");
    expect(model.toolTexts[3]).toContain("lease generation 2");
    expect((await page.evaluate(() => window.modbit.browserLog())).filter((l) => l.kind === "act")).toEqual([]);

    // Return control from the tab: generation 3, and the agent's next fill lands.
    await page.getByTestId("browser-tab-return").click();
    await expect(tab).toContainText("Agent holds control", { timeout: 15_000 });
    await expect(page.getByTestId("browser-tab-lease")).toContainText("lease generation 3");
    second.open();
    await toolResults(model, 5);
    expect(model.toolTexts[4]).toContain("status: SUCCESS");
    const acts = (await page.evaluate(() => window.modbit.browserLog())).filter((l) => l.kind === "act");
    expect(acts.map((l) => `${l.ok ? "ok" : l.code}@${l.generation}`)).toEqual(["ok@3"]);
    expect(await page.evaluate(([id, t]) => window.modbit.browserSession(id!, t!), [bsid, taskId])).toMatchObject({ controller: "AGENT", leaseGeneration: "3" });
  } finally {
    first.open();
    second.open();
    await closeApp(app);
    model.server.close();
    site.server.close();
  }
});
