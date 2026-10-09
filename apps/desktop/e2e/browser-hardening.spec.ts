/**
 * PX-073 and PX-120 (docs/62, QUAL-PX-073 and QUAL-PX-120): the browser
 * host's hardening, on real pages in the real app against a real Core.
 *
 *  - Where a session may go: loopback, private ranges, link-local and the
 *    cloud metadata endpoint are refused unless the person's policy names
 *    them - by literal address, by a name that resolves privately, through a
 *    redirect hop, in a sub-frame and a sub-resource - with a typed refusal
 *    that asks the person, and nothing ever reaches the refused server.
 *  - Which pages a tool may act on: every call looks at the page's origin,
 *    so a page that redirected itself client-side after an allowed
 *    navigation is refused; `file:` and an unknown origin always are.
 *  - A certificate error waits for the person; Reject fails the request,
 *    Trust is remembered and then cleared.
 *  - Permissions are denied and named; signing out wipes the partition; a
 *    task sees only the views it owns; hidden views are reclaimed least
 *    recently used, with the reset stated.
 */
import { expect, test } from "@playwright/test";
import { createServer as createHttps } from "node:https";
import { createServer, type Server } from "node:http";
import { mkdtempSync, readFileSync, readdirSync, statSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { closeApp, collectEvents, eventsOf, fixtureRepo, gate, launch, lastJson, readyForReview, refOf, scriptedModel, sleep, startBrowserTask, toolResults } from "./support/browser-harness.ts";

type Site = { server: Server; port: number; hits: string[] };

function serve(handler: (url: string, port: () => number) => { status?: number; headers?: Record<string, string>; body: string } | null): Promise<Site> {
  const hits: string[] = [];
  return new Promise((resolveSite) => {
    let port = 0;
    const server = createServer((req, res) => {
      const url = req.url ?? "/";
      hits.push(url);
      const r = handler(url, () => port);
      if (r === null) {
        res.writeHead(404).end("not found");
        return;
      }
      res.writeHead(r.status ?? 200, { "content-type": "text/html; charset=utf-8", ...(r.headers ?? {}) });
      res.end(r.body);
    });
    server.listen(0, "127.0.0.1", () => {
      port = (server.address() as { port: number }).port;
      resolveSite({ server, port, hits });
    });
  });
}

const page = (title: string, body: string) => `<!doctype html><html><head><title>${title}</title></head><body><main><h1>${title}</h1>${body}</main></body></html>`;

test("PX-120: a destination on this machine or its network is refused unless the policy names it - by address, by a name that resolves privately, at a redirect hop, in a sub-frame, in a sub-resource - and nothing reaches the refused server", async () => {
  test.setTimeout(240_000);
  const repo = fixtureRepo();
  const other = await serve((url) => (url.startsWith("/inner") ? { body: page("Inner secret", "<p>inner</p>") } : { body: page("B secret", "<p>reached B</p>") }));
  const home = await serve((url) => {
    if (url.startsWith("/hop")) return { status: 302, headers: { location: "http://169.254.169.254/latest/meta-data/" }, body: "" };
    if (url.startsWith("/rebind-hop")) return { status: 302, headers: { location: `http://rebind.test:${other.port}/hop-target` }, body: "" };
    if (url.startsWith("/frames")) {
      return { body: page("Frames", `<p>a page with frames</p><iframe src="http://127.0.0.1:${other.port}/inner" title="local frame" width="200" height="60"></iframe><iframe src="http://169.254.169.254/" title="metadata frame" width="200" height="60"></iframe><img alt="beacon" src="http://rebind.test:${other.port}/beacon.png"><script>fetch('http://127.0.0.1:${other.port}/xhr').catch(function(){});</script>`) };
    }
    return { body: page("Home A", `<p>named by the policy</p><a href="/hop">Hop</a>`) };
  });
  const widened = gate();
  const beforeClick = gate();
  const beforeFrames = gate();
  const A = `http://127.0.0.1:${home.port}`;
  const refused = (n: number) => model.toolTexts[n]!;
  const model = await scriptedModel([
    { calls: [{ name: "plan.update", args: { outcome: "the destinations are tried", expected_files: [] } }] },
    { calls: [{ name: "browser.navigate", args: { url: `${A}/` } }] },
    // Loopback on another port: not named.
    { calls: [{ name: "browser.navigate", args: { url: `http://127.0.0.1:${other.port}/secret` } }] },
    { calls: [{ name: "browser.navigate", args: { url: `${A}/` } }] },
    // The metadata endpoint, a private range.
    { calls: [{ name: "browser.navigate", args: { url: "http://169.254.169.254/latest/meta-data/" } }] },
    { calls: [{ name: "browser.navigate", args: { url: "http://10.255.255.1/" } }] },
    { calls: [{ name: "browser.navigate", args: { url: `${A}/` } }] },
    // A name that says nothing and resolves to this machine.
    { calls: [{ name: "browser.navigate", args: { url: `http://rebind.test:${other.port}/` } }] },
    { calls: [{ name: "browser.navigate", args: { url: `${A}/` } }] },
    // A redirect hop to a name that resolves to this machine, and one to the metadata endpoint.
    { calls: [{ name: "browser.navigate", args: { url: `${A}/rebind-hop` } }] },
    { calls: [{ name: "browser.navigate", args: { url: `${A}/` } }] },
    { calls: [{ name: "browser.navigate", args: { url: `${A}/hop` } }] },
    // The page itself is allowed; its sub-frames and sub-resources to local targets are not.
    async () => {
      await beforeFrames.opened;
      return { calls: [{ name: "browser.navigate", args: { url: `${A}/frames` } }] };
    },
    { calls: [{ name: "browser.network", args: { failed_only: true } }] },
    { calls: [{ name: "browser.navigate", args: { url: `${A}/` } }] },
    // A link click that leads through a redirect to the metadata endpoint.
    { calls: [{ name: "browser.snapshot", args: { mode: "full" } }] },
    async (texts) => {
      await beforeClick.opened;
      return { calls: [{ name: "browser.act", args: { ref: refOf(texts, "link", "Hop"), action: "click" } }] };
    },
    { calls: [{ name: "browser.navigate", args: { url: `${A}/` } }] },
    // The model cannot widen the policy through an argument.
    { calls: [{ name: "browser.navigate", args: { url: `http://127.0.0.1:${other.port}/secret`, allow_private: true, policy: { allow_targets: ["127.0.0.1"] } } }] },
    { calls: [{ name: "browser.navigate", args: { url: `${A}/` } }] },
    // The person names the destination: the same one now loads.
    async () => {
      await widened.opened;
      return { calls: [{ name: "browser.navigate", args: { url: `http://127.0.0.1:${other.port}/secret` } }] };
    },
    { calls: [{ name: "task.complete", args: { summary: "tried the destinations", self_review: { findings: [] } } }] },
  ]);
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-targets-"));
  try {
    const { app, page: ui } = await launch(dataDir, {
      MODBIT_OPENAI_BASE_URL: model.url,
      MODBIT_BROWSER_ALLOW_TARGETS: `127.0.0.1:${home.port}`,
      // A name that resolves to this machine without saying so (the operator's static answer).
      MODBIT_BROWSER_HOSTS: "rebind.test=127.0.0.1",
    });
    await collectEvents(ui);
    const { bsid } = await startBrowserTask(ui, repo, "try the destinations");
    beforeFrames.open();
    beforeClick.open();
    await toolResults(model, 20, 120_000);
    // Each refusal is typed, says why, and asks the person; none was a failure of the page.
    for (const i of [2, 4, 5, 7, 9, 11]) {
      expect(refused(i), `result ${i}`).toContain("TARGET_NOT_ALLOWED");
      expect(refused(i), `result ${i}`).toContain('"escalation":"ask_user"');
      expect(refused(i), `result ${i}`).toContain("browser policy");
    }
    expect(refused(2)).toContain("loopback");
    expect(refused(4)).toContain("cloud-metadata");
    expect(refused(5)).toContain("private-network");
    expect(refused(7)).toContain("rebind.test resolves to 127.0.0.1");
    expect(refused(9)).toMatch(/redirect|page now shows the policy's refusal/);
    expect(refused(11)).toContain("169.254.169.254");
    // The named origin loads, and the others never reached their server: the sub-frame, the image and the fetch included.
    expect(refused(1)).toContain("status: SUCCESS");
    expect(other.hits).toEqual([]);
    // The page with frames loaded; the refused sub-requests are on the network record, blocked, not failed by the page.
    expect(refused(12)).toContain("status: SUCCESS");
    const net = lastJson<{ entries: { url: string; failed?: string }[] }>([refused(13)], '"entries"');
    const blocked = net.entries.filter((e) => e.failed?.startsWith("blocked"));
    expect(blocked.length, refused(13).slice(0, 800)).toBeGreaterThanOrEqual(2);
    expect(blocked.map((e) => e.url).join(" ")).toMatch(/169\.254\.169\.254|rebind\.test|127\.0\.0\.1:\d+\/(inner|xhr)/);
    // The click led the page to a destination the policy refuses.
    expect(refused(16)).toContain("TARGET_NOT_ALLOWED");
    expect(refused(16)).toContain("169.254.169.254");
    // The model's attempt to widen the policy is not a tool call at all.
    expect(refused(18)).toContain("SCHEMA_VIOLATION");
    expect(refused(18)).toContain("Additional properties are not allowed");
    expect(other.hits).toEqual([]);
    // What the host recorded and what the log holds.
    const refusals = await ui.evaluate((id) => window.modbit.browserRefusals(id), bsid);
    expect(refusals.map((r) => r.class)).toEqual(expect.arrayContaining(["loopback", "metadata", "private"]));
    expect((await eventsOf(ui, "SecurityEventRecorded")).filter((e) => e.kind === "BROWSER_TARGET_REFUSED").length).toBeGreaterThanOrEqual(6);
    // The person names the destination (their policy, their shell): the same address now loads.
    const policy = await ui.evaluate((p) => window.modbit.setBrowserPolicy({ allowTargets: p }), [`127.0.0.1:${home.port}`, `127.0.0.1:${other.port}`]);
    expect(policy.allowTargets).toContain(`127.0.0.1:${other.port}`);
    widened.open();
    await readyForReview(ui, model);
    expect(model.toolTexts[20]).toContain("status: SUCCESS");
    expect(other.hits).toEqual(["/secret"]);
    await closeApp(app);
  } finally {
    model.server.close();
    home.server.close();
    other.server.close();
  }
});

test("PX-073: every call looks at the page's origin - a client-side redirect after an allowed navigation, file: and an unknown origin are all refused, and the refusal lists what is allowed", async () => {
  test.setTimeout(240_000);
  const repo = fixtureRepo();
  const landed = await serve(() => ({ body: page("Landed C", `<button type="button">Do it</button>`) }));
  const home = await serve((url) => {
    if (url.startsWith("/start")) return { body: page("Start", `<p>allowed page</p><script>setTimeout(function(){location.href='http://127.0.0.1:${landed.port}/landed';}, 400);</script>`) };
    return { body: page("Plain", `<button type="button">Press</button>`) };
  });
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-origin-"));
  const filePage = join(dataDir, "local.html");
  writeFileSync(filePage, "<html><body><button>from a file</button></body></html>");
  const A = `http://127.0.0.1:${home.port}`;
  const g1 = gate();
  const g2 = gate();
  const g3 = gate();
  const model = await scriptedModel([
    { calls: [{ name: "plan.update", args: { outcome: "the origin gate is tried", expected_files: [] } }] },
    { calls: [{ name: "browser.navigate", args: { url: `${A}/start` } }] },
    // The page redirected itself to an origin the policy does not allow; the navigation that brought the session here was allowed.
    async () => {
      await sleep(1_500);
      return { calls: [{ name: "browser.snapshot", args: {} }] };
    },
    { calls: [{ name: "browser.console", args: {} }] },
    { calls: [{ name: "browser.navigate", args: { url: `${A}/plain` } }] },
    // A file: page.
    async () => {
      await g1.opened;
      return { calls: [{ name: "browser.snapshot", args: {} }] };
    },
    { calls: [{ name: "browser.navigate", args: { url: `${A}/plain` } }] },
    async () => {
      await g2.opened;
      return { calls: [{ name: "browser.navigate", args: { url: `${A}/plain` } }] };
    },
    // An origin that cannot be determined (a data: page).
    async () => {
      await g3.opened;
      return { calls: [{ name: "browser.snapshot", args: {} }] };
    },
    { calls: [{ name: "browser.navigate", args: { url: `${A}/plain` } }] },
    { calls: [{ name: "task.complete", args: { summary: "tried the origin gate", self_review: { findings: [] } } }] },
  ]);
  try {
    const { app, page: ui } = await launch(dataDir, {
      MODBIT_OPENAI_BASE_URL: model.url,
      MODBIT_BROWSER_ALLOW_TARGETS: "127.0.0.1",
      MODBIT_BROWSER_ALLOW_ORIGINS: `http://127.0.0.1:${home.port}`,
    });
    const { bsid } = await startBrowserTask(ui, repo, "try the origin gate");
    const viewId = (await ui.evaluate((id) => window.modbit.describeBrowser(id), bsid))!.webContentsId;
    // The person (or anything in the main process) puts another document in the view: the next call must notice.
    const load = (target: string) => app.evaluate(async ({ webContents }, [id, u]) => webContents.fromId(Number(id))!.loadURL(String(u)), [String(viewId), target]);
    // 2: refused on the page it redirected to, with the allowed origins listed.
    await toolResults(model, 4, 60_000);
    expect(model.toolTexts[2]).toContain("ORIGIN_NOT_ALLOWED");
    expect(model.toolTexts[2]).toContain(`http://127.0.0.1:${landed.port} is not an origin the browser policy allows`);
    expect(model.toolTexts[2]).toContain(`allowed: http://127.0.0.1:${home.port}`);
    expect(model.toolTexts[2]).toContain('"escalation":"ask_user"');
    expect(model.toolTexts[3]).toContain("ORIGIN_NOT_ALLOWED");
    // The way out is a navigation to an allowed origin, and the page is read again.
    await toolResults(model, 5, 60_000);
    expect(model.toolTexts[4]).toContain("status: SUCCESS");
    // A file: page refuses every tool, navigation included.
    await load(pathToFileURL(filePage).href);
    g1.open();
    await toolResults(model, 6, 60_000);
    expect(model.toolTexts[5]).toContain("FILE_ORIGIN");
    await toolResults(model, 7, 60_000);
    expect(model.toolTexts[6]).toContain("FILE_ORIGIN");
    // The person brings the view back; then a data: page, an origin that cannot be determined, fails closed.
    await load(`${A}/plain`);
    g2.open();
    await toolResults(model, 8, 60_000);
    await load("data:text/html,<p>no origin</p>");
    g3.open();
    await readyForReview(ui, model);
    expect(model.toolTexts[8]).toContain("ORIGIN_NOT_ALLOWED");
    expect(model.toolTexts[8]).toContain("not an http(s) origin");
    // The landed site was reached by the page itself (the redirect), never acted on.
    expect(landed.hits).toEqual(["/landed"]);
    await closeApp(app);
  } finally {
    model.server.close();
    home.server.close();
    landed.server.close();
  }
});

test("PX-073: a certificate error waits for the person - Reject fails the request, Trust is remembered per workspace, listed, and cleared", async () => {
  test.setTimeout(240_000);
  const repo = fixtureRepo();
  const fixtures = resolve(dirname(fileURLToPath(import.meta.url)), "fixtures");
  const tlsHits: string[] = [];
  const tlsErrors: string[] = [];
  const https = createHttps({ key: readFileSync(join(fixtures, "test-key.pem")), cert: readFileSync(join(fixtures, "test-cert.pem")) }, (req, res) => {
    tlsHits.push(req.url ?? "");
    res.writeHead(200, { "content-type": "text/html" }).end(page("Secure fixture", "<p>served over a self-signed certificate</p>"));
  });
  https.on("tlsClientError", (e) => tlsErrors.push(e.message));
  await new Promise<void>((r) => https.listen(0, "127.0.0.1", r));
  const port = (https.address() as { port: number }).port;
  const url = `https://127.0.0.1:${port}/`;
  const g = [gate(), gate(), gate()];
  const model = await scriptedModel([
    { calls: [{ name: "plan.update", args: { outcome: "the certificate is decided by the person", expected_files: [] } }] },
    { calls: [{ name: "browser.navigate", args: { url } }] },
    async () => {
      await g[0]!.opened;
      return { calls: [{ name: "browser.navigate", args: { url } }] };
    },
    async () => {
      await g[1]!.opened;
      return { calls: [{ name: "browser.navigate", args: { url } }] };
    },
    { calls: [{ name: "browser.snapshot", args: {} }] },
    async () => {
      await g[2]!.opened;
      return { calls: [{ name: "browser.navigate", args: { url } }] };
    },
    { calls: [{ name: "task.complete", args: { summary: "certificates decided", self_review: { findings: [] } } }] },
  ]);
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-cert-"));
  try {
    const { app, page: ui } = await launch(dataDir, { MODBIT_OPENAI_BASE_URL: model.url, MODBIT_BROWSER_ALLOW_TARGETS: "127.0.0.1" });
    const { bsid } = await startBrowserTask(ui, repo, "decide the certificate");
    // The navigation waits for the person; the agent is told at once, in words that say so.
    await toolResults(model, 2, 60_000);
    expect(model.toolTexts[1]).toContain("CERTIFICATE_PENDING");
    expect(model.toolTexts[1]).toContain('"escalation":"ask_user"');
    expect(model.toolTexts[1]).toContain("fingerprint sha256/");
    const pending = (await ui.evaluate((id) => window.modbit.describeBrowser(id), bsid))!.certPending;
    expect(pending).toMatchObject({ hostPort: `127.0.0.1:${port}` });
    expect(pending!.subject).toContain("localhost");
    expect(pending!.issuer).toContain("localhost");
    // The person sees it in the Browser panel with issuer, subject, validity and fingerprint, and rejects it.
    const card = ui.getByTestId("task-card").first();
    await card.getByTestId("task-browser").click();
    await expect(ui.getByTestId("browser-cert")).toBeVisible({ timeout: 15_000 });
    await expect(ui.getByTestId("browser-cert-details")).toContainText("issuer");
    await expect(ui.getByTestId("browser-cert-details")).toContainText("fingerprint sha256/");
    await ui.getByTestId("browser-cert-reject").click();
    await expect(ui.getByTestId("browser-cert")).toHaveCount(0, { timeout: 15_000 });
    expect(tlsHits, "a rejected certificate sends no request").toEqual([]);
    g[0]!.open();
    // Asked again: pending again (a rejection is not remembered).
    await toolResults(model, 3, 60_000);
    const certDump = async () => JSON.stringify({ described: await ui.evaluate((id) => window.modbit.describeBrowser(id), bsid), log: (await ui.evaluate(() => window.modbit.browserLog())).slice(-10).map((l) => `${l.kind}:${l.code}:${l.detail ?? ""}`) });
    expect(model.toolTexts[2], await certDump()).toContain("CERTIFICATE_PENDING");
    await expect(ui.getByTestId("browser-cert")).toBeVisible({ timeout: 15_000 });
    await ui.getByTestId("browser-cert-trust").click();
    await expect(ui.getByTestId("browser-cert-trusts").locator("li")).toHaveCount(1, { timeout: 15_000 });
    await expect(ui.getByTestId("browser-cert-trusts").locator("li").first()).toHaveAttribute("data-host-port", `127.0.0.1:${port}`);
    await expect.poll(() => tlsHits.length, { timeout: 30_000 }).toBeGreaterThan(0);
    await ui.getByTestId("browser-close").click();
    g[1]!.open();
    // Trusted now: the same certificate loads with no prompt, remembered.
    try {
      await toolResults(model, 5, 40_000);
    } catch (e) {
      const log = await ui.evaluate(() => window.modbit.browserLog());
      throw new Error(`${(e as Error).message}\nhost log: ${JSON.stringify(log.slice(-14))}`);
    }
    expect(model.toolTexts[3]).toContain("status: SUCCESS");
    expect(model.toolTexts[4]).toContain("Secure fixture");
    const trusts = await ui.evaluate(() => window.modbit.browserCertificateTrusts());
    expect(trusts).toHaveLength(1);
    expect(trusts[0]).toMatchObject({ hostPort: `127.0.0.1:${port}` });
    // Cleared means cleared: the next navigation asks again.
    expect(await ui.evaluate(() => window.modbit.clearBrowserCertificateTrusts())).toBe(1);
    expect(await ui.evaluate(() => window.modbit.browserCertificateTrusts())).toHaveLength(0);
    g[2]!.open();
    await toolResults(model, 6, 60_000);
    expect(model.toolTexts[5]).toContain("CERTIFICATE_PENDING");
    await ui.evaluate(([id, c]) => window.modbit.decideBrowserCertificate(id!, c!, "reject"), [bsid, (await ui.evaluate((id) => window.modbit.describeBrowser(id), bsid))!.certPending!.id]);
    await closeApp(app);
  } finally {
    model.server.close();
    https.close();
  }
});

test("PX-073: the page's permissions are denied and named, signing out wipes the partition, and the session gives the page no download", async () => {
  test.setTimeout(240_000);
  const repo = fixtureRepo();
  const MARK = `MARKER-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 10)}`;
  const downloads: string[] = [];
  const site = await serve((url) => {
    if (url.startsWith("/dl")) {
      downloads.push(url);
      return { headers: { "content-disposition": "attachment; filename=\"x.bin\"", "content-type": "application/octet-stream" }, body: "BINARY" };
    }
    return {
      body: page("Permissions", `<button type="button" onclick="Notification.requestPermission()">Notify</button><button type="button" onclick="navigator.geolocation.getCurrentPosition(function(){},function(){})">Locate</button><a href="/dl">Download</a><script>document.cookie='sess=${MARK}; path=/; max-age=86400'; localStorage.setItem('k', '${MARK}'); var r = indexedDB.open('db', 1); r.onupgradeneeded = function () { r.result.createObjectStore('s'); }; r.onsuccess = function () { r.result.transaction('s', 'readwrite').objectStore('s').put('${MARK}', 'v'); document.title = 'stored'; };</script>`),
    };
  });
  const written = gate();
  const model = await scriptedModel([
    { calls: [{ name: "plan.update", args: { outcome: "the permissions are tried", expected_files: [] } }] },
    { calls: [{ name: "browser.navigate", args: { url: `http://127.0.0.1:${site.port}/` } }] },
    { calls: [{ name: "browser.snapshot", args: { mode: "full" } }] },
    (texts) => ({ calls: [{ name: "browser.act", args: { ref: refOf(texts, "button", "Notify"), action: "click" } }] }),
    { calls: [{ name: "browser.navigate", args: { url: `http://127.0.0.1:${site.port}/?again` } }] },
    (texts) => ({ calls: [{ name: "browser.act", args: { ref: refOf(texts, "button", "Locate"), action: "click" } }] }),
    async () => {
      await written.opened;
      return { calls: [{ name: "browser.navigate", args: { url: `http://127.0.0.1:${site.port}/?after-sign-out` } }] };
    },
    { calls: [{ name: "task.complete", args: { summary: "tried the permissions", self_review: { findings: [] } } }] },
  ]);
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-perm-"));
  try {
    const { app, page: ui } = await launch(dataDir, { MODBIT_OPENAI_BASE_URL: model.url, MODBIT_BROWSER_ALLOW_TARGETS: "127.0.0.1" });
    const { bsid } = await startBrowserTask(ui, repo, "try the permissions");
    await toolResults(model, 6, 60_000);
    // A page asking for notifications or for the location is denied, and the agent is told which.
    expect(model.toolTexts[3]).toMatch(/PERMISSION_REQUIRED/);
    expect(model.toolTexts[3]).toContain("notifications");
    expect(model.toolTexts[3]).toContain('"escalation":"abandon"');
    expect(model.toolTexts[5]).toContain("PERMISSION_REQUIRED");
    expect(model.toolTexts[5]).toContain("geolocation");
    // A download is never kept: the host refuses every one.
    const described = (await ui.evaluate((id) => window.modbit.describeBrowser(id), bsid))!;
    // The partition holds the page's cookie, local storage and IndexedDB ...
    const userData = await app.evaluate(({ app: a }) => a.getPath("userData"));
    const partitionDir = join(userData, "Partitions", described.partition.replace(/^persist:/, ""));
    await app.evaluate(async ({ session }, partition) => {
      const s = session.fromPartition(partition);
      await s.cookies.flushStore();
      s.flushStorageData();
    }, described.partition);
    const cookiesBefore = await app.evaluate(({ session }, partition) => session.fromPartition(partition).cookies.get({}).then((c) => c.map((x) => x.value)), described.partition);
    expect(cookiesBefore).toContain(MARK);
    await sleep(1500);
    const scan = (dir: string): boolean => {
      let found = false;
      const walk = (d: string) => {
        let names: string[] = [];
        try {
          names = readdirSync(d);
        } catch {
          return;
        }
        for (const n of names) {
          const p = join(d, n);
          let st;
          try {
            st = statSync(p);
          } catch {
            continue;
          }
          if (st.isDirectory()) walk(p);
          else if (st.size < 20_000_000) {
            // Windows keeps some profile files locked while the session lives: a file that cannot be read now is read at the next poll.
            try {
              if (readFileSync(p).includes(MARK)) found = true;
            } catch (e) {
              if (!["EBUSY", "EPERM", "EACCES", "ENOENT"].includes((e as NodeJS.ErrnoException).code ?? "")) throw e;
            }
          }
        }
      };
      walk(dir);
      return found;
    };
    const storageDirs = ["Local Storage", "IndexedDB"].map((d) => join(partitionDir, d));
    await expect.poll(() => storageDirs.some(scan), { timeout: 20_000, message: "the page's storage reached the disk" }).toBe(true);
    // ... and signing out leaves no cookie or storage in it.
    const cleared = await ui.evaluate((id) => window.modbit.clearBrowserData(id), bsid);
    expect(cleared.cleared).toEqual([bsid]);
    expect(await app.evaluate(({ session }, partition) => session.fromPartition(partition).cookies.get({}).then((c) => c.length), described.partition)).toBe(0);
    await expect.poll(() => storageDirs.some(scan), { timeout: 20_000, message: "no marker left in the partition's storage" }).toBe(false);
    const log = await ui.evaluate(() => window.modbit.browserLog());
    expect(log.some((l) => l.kind === "data-cleared")).toBe(true);
    written.open();
    await readyForReview(ui, model);
    expect(downloads).toEqual([]);
    await closeApp(app);
  } finally {
    model.server.close();
    site.server.close();
  }
});

test("PX-073: a task lists and selects only the views it owns; hidden views are reclaimed least recently used - never one the person holds - and the reset is stated", async () => {
  test.setTimeout(240_000);
  const repo = fixtureRepo();
  const site = await serve(() => ({ body: page("Owned", `<p>a page</p>`) }));
  const url = `http://127.0.0.1:${site.port}/`;
  const model = await scriptedModel([
    { calls: [{ name: "plan.update", args: { outcome: "the reclaimed view is used", expected_files: [] } }] },
    { calls: [{ name: "browser.navigate", args: { url } }] },
    { calls: [{ name: "browser.navigate", args: { url } }] },
    { calls: [{ name: "task.complete", args: { summary: "used the reclaimed view", self_review: { findings: [] } } }] },
  ]);
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-views-"));
  try {
    // Two hidden views are kept; the third reclaims the least recently used.
    const { app, page: ui } = await launch(dataDir, { MODBIT_OPENAI_BASE_URL: model.url, MODBIT_BROWSER_ALLOW_TARGETS: "127.0.0.1", MODBIT_BROWSER_HIDDEN_VIEW_CAP: "2" });
    const sessionId = await ui.evaluate(() => window.modbit.createSession());
    const mk = async (n: number) => {
      const t = await ui.evaluate(([s, goal, root]) => window.modbit.createTask(s!, goal!, "", root!), [sessionId, `view ${n}`, repo]);
      const b = await ui.evaluate(([s, tid]) => window.modbit.openBrowser(s!, tid!), [sessionId, t.taskId]);
      return { taskId: t.taskId, bsid: b.browserSessionId };
    };
    const settled = async (b: string) => expect.poll(async () => (await ui.evaluate((id) => window.modbit.describeBrowser(id), b))?.attached, { timeout: 15_000 }).toBe(true);
    const A = await mk(1);
    await settled(A.bsid);
    const B = await mk(2);
    await settled(B.bsid);
    // A task lists and selects only its own views.
    expect((await ui.evaluate((t) => window.modbit.listBrowserViews(t), A.taskId)).map((v) => v.browserSessionId)).toEqual([A.bsid]);
    expect(await ui.evaluate(([t, b]) => window.modbit.selectBrowserView(t!, b!), [A.taskId, B.bsid])).toBeNull();
    expect(await ui.evaluate(([b, t]) => window.modbit.describeBrowser(b!, t!), [B.bsid, A.taskId])).toBeNull();
    expect((await ui.evaluate(([t, b]) => window.modbit.selectBrowserView(t!, b!), [A.taskId, A.bsid]))?.browserSessionId).toBe(A.bsid);
    // The person holds B: it is not reclaimable.
    await ui.evaluate((b) => window.modbit.setBrowserControl(b, "USER"), B.bsid);
    await expect.poll(async () => (await ui.evaluate((id) => window.modbit.describeBrowser(id), B.bsid))?.controller, { timeout: 15_000 }).toBe("USER");
    const C = await mk(3);
    await settled(C.bsid);
    const state = async (b: string) => (await ui.evaluate((id) => window.modbit.describeBrowser(id), b))!.reclaimed;
    // A was used last at its selection above, B is held, C is new: with two hidden allowed and three open, the least recently used that may be reclaimed is A.
    // The cap is enforced as soon as a view can be reclaimed (one still loading its first page waits a moment): wait on the state.
    const dump = async () => JSON.stringify({ views: await Promise.all([A, B, C].map(async (v) => ({ id: v.bsid.slice(0, 6), d: await ui.evaluate((id) => window.modbit.describeBrowser(id), v.bsid) }))), log: (await ui.evaluate(() => window.modbit.browserLog())).map((l) => `${l.kind}:${l.code}`) });
    await expect.poll(() => state(A.bsid), { timeout: 15_000, message: await dump() }).toBe(true);
    expect(await state(B.bsid), await dump()).toBe(false);
    expect(await state(C.bsid), await dump()).toBe(false);
    // The reclaimed view is the owner's still, and the agent's first request of it states the reset.
    await ui.evaluate(([s, r]) => window.modbit.trustRepository(s!, r!), [sessionId, repo]);
    await ui.evaluate(([s, t]) => window.modbit.startTask(s!, t!), [sessionId, A.taskId]);
    await toolResults(model, 3, 90_000);
    expect(model.toolTexts[1]).toContain("VIEW_RESET");
    expect(model.toolTexts[1]).toContain("reclaimed");
    expect(model.toolTexts[1]).toContain('"escalation":"re_observe"');
    expect(model.toolTexts[2]).toContain("status: SUCCESS");
    expect(await state(A.bsid)).toBe(false);
    expect(site.hits.length).toBeGreaterThan(0);
    const log = await ui.evaluate(() => window.modbit.browserLog());
    expect(log.filter((l) => l.kind === "view-reclaimed").length).toBeGreaterThanOrEqual(1);
    expect(log.some((l) => l.kind === "view-revived")).toBe(true);
    await closeApp(app);
  } finally {
    model.server.close();
    site.server.close();
  }
});
