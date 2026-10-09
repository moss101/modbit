/**
 * PX-122 (docs/62, QUAL-PX-122): page-state deltas from the host's mutation
 * observer, `since_fingerprint`, multi-frame accessibility trees and stable
 * identities - on real pages in the real app against a real Core.
 *
 * An update the page makes by itself reaches the Core as a bounded notice
 * and is journaled as a semantic delta while the model is parked (no model
 * call); the model then asks for the changes since the fingerprint it holds
 * and gets exactly those, and a fingerprint nobody holds gets the full page
 * with the reason; same-process and cross-process frames are read with their
 * frame and origin, and acted in; a page that floods the DOM, and that tries
 * to take the observer away, is reported at a bounded rate.
 */
import { expect, test } from "@playwright/test";
import { createServer, type Server } from "node:http";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { closeApp, collectEvents, eventsOf, fixtureRepo, gate, launch, lastJson, readyForReview, refOf, scriptedModel, sleep, startBrowserTask } from "./support/browser-harness.ts";

type Site = { server: Server; port: number; hits: string[] };

function serve(handler: (url: string, hits: string[]) => string | null): Promise<Site> {
  const hits: string[] = [];
  return new Promise((resolve) => {
    const server = createServer((req, res) => {
      const url = req.url ?? "/";
      hits.push(url);
      const body = handler(url, hits);
      if (body === null) {
        res.writeHead(404).end("not found");
        return;
      }
      res.writeHead(200, { "content-type": "text/html; charset=utf-8" });
      res.end(body);
    });
    server.listen(0, "127.0.0.1", () => resolve({ server, port: (server.address() as { port: number }).port, hits }));
  });
}

test("a page's own update is journaled as a delta with no model call; since_fingerprint answers exactly the changes since, an unknown fingerprint the full page with the reason; frames are read and acted in with their origin", async () => {
  test.setTimeout(240_000);
  const repo = fixtureRepo();
  // The cross-process frame lives on another site: `localhost` is not `127.0.0.1`.
  const other = await serve((url) => (url.startsWith("/frame-b") ? `<!doctype html><html><head><title>B</title></head><body><button type="button" onclick="this.textContent='Inner B clicked'">Inner B</button><label for="bf">B field</label><input id="bf" type="text"></body></html>` : null));
  const site = await serve((url) => {
    if (url.startsWith("/frame-a")) return `<!doctype html><html><body><button type="button" id="a">Inner A</button><label for="fa">Frame field</label><input id="fa" type="text"></body></html>`;
    if (url.startsWith("/dynamic")) {
      return `<!doctype html><html><head><title>Dynamic</title></head><body><main><h1>Dynamic</h1><p id="status">idle</p><ul id="list" aria-label="Items"><li><a href="/item/1">Item 1</a></li></ul><form aria-label="Compose" id="compose"><label for="subject">Subject</label><input id="subject" name="subject" type="text"><button type="button" id="send">Send</button></form><iframe src="/frame-a" title="Frame A" width="300" height="80"></iframe><iframe src="http://localhost:${other.port}/frame-b" title="Frame B" width="300" height="80"></iframe></main><script>setTimeout(function(){document.getElementById('status').textContent='updated by the page';var li=document.createElement('li');li.innerHTML='<a href="/item/2">Item 2</a>';document.getElementById('list').appendChild(li);},1800);</script></body></html>`;
    }
    return null;
  });
  const changed = gate();
  let calls = 0;
  const model = await scriptedModel([
    { calls: [{ name: "plan.update", args: { outcome: "the dynamic page is read", expected_files: [] } }] },
    { calls: [{ name: "browser.navigate", args: { url: `http://127.0.0.1:${site.port}/dynamic` } }] },
    { calls: [{ name: "browser.snapshot", args: { mode: "full" } }] },
    // The model is parked here while the page updates itself: no model call happens meanwhile.
    async () => {
      calls = model.toolTexts.length;
      await changed.opened;
      return { calls: [{ name: "browser.snapshot", args: { since_fingerprint: lastJson(model.toolTexts, '"entities":').state_fingerprint } }] };
    },
    // Frames: the same-process frame's field is filled ...
    (texts) => ({ calls: [{ name: "browser.act", args: { ref: refOf(texts, "textbox", "Frame field"), action: "fill", value: "typed in frame A", expect: { value: { ref: refOf(texts, "textbox", "Frame field"), equals: "typed in frame A" } } } }] }),
    // ... a fingerprint nobody holds is answered with the full page and the reason ...
    { calls: [{ name: "browser.snapshot", args: { since_fingerprint: "0".repeat(64) } }] },
    // ... and the cross-process frame's button is clicked.
    (texts) => ({ calls: [{ name: "browser.act", args: { ref: refOf(texts, "button", "Inner B"), action: "click", expect: { text_contains: "Inner B clicked" } } }] }),
    { calls: [{ name: "task.complete", args: { summary: "read the dynamic page and its frames", self_review: { findings: [] } } }] },
  ]);
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-observer-"));
  try {
    const { app, page } = await launch(dataDir, { MODBIT_OPENAI_BASE_URL: model.url, MODBIT_BROWSER_ALLOW_TARGETS: "127.0.0.1,localhost" });
    await collectEvents(page);
    await startBrowserTask(page, repo, "read the dynamic page");
    await expect.poll(() => model.toolTexts.length, { timeout: 60_000 }).toBe(3);
    // The first snapshot: the page, its two frames with their origins, the item list.
    const first = lastJson<{ entities: any[]; frames: any[]; state_fingerprint: string; page_kind: string }>(model.toolTexts, '"entities":');
    expect(first.frames.map((f) => [f.origin, f.cross_process])).toEqual([
      [`http://127.0.0.1:${site.port}`, false],
      [`http://localhost:${other.port}`, true],
    ]);
    const innerA = first.entities.find((e) => e.name === "Inner A");
    const innerB = first.entities.find((e) => e.name === "Inner B");
    expect(innerA.frame.id).toBe("f1");
    expect(innerB.frame).toEqual({ id: "f2", origin: `http://localhost:${other.port}` });
    expect(first.entities.some((e) => e.name === "Item 2")).toBe(false);
    // The update the page makes by itself reaches the Core as a bounded notice and is read and journaled without the model.
    await expect
      .poll(async () => (await eventsOf(page, "BrowserPageChanged")).filter((e) => e.from_fingerprint !== "" && e.text_added >= 1 && e.text_removed >= 1).length, { timeout: 30_000 })
      .toBeGreaterThan(0);
    expect(model.toolTexts.length, "no model call happened while the page changed").toBe(calls);
    const change = (await eventsOf(page, "BrowserPageChanged")).find((e) => e.from_fingerprint !== "" && e.text_added >= 1 && e.text_removed >= 1)!;
    expect(change.latency_ms).toBeLessThan(3_000);
    expect(change.from_fingerprint).not.toBe(change.to_fingerprint);
    expect(change.page_ref).toMatch(/^[0-9a-f]{64}$/);
    changed.open();
    await expect.poll(() => model.toolTexts.length, { timeout: 60_000 }).toBeGreaterThanOrEqual(5);
    // since_fingerprint: exactly the changes since the page the model held.
    const delta = lastJson<Record<string, any>>([model.toolTexts[3]!], '"mode"');
    expect(delta.mode).toBe("delta");
    expect(delta.from_fingerprint).toBe(first.state_fingerprint);
    expect(delta.added.map((e: any) => e.name), model.toolTexts[3]!.slice(0, 2500)).toEqual(["Item 2"]);
    expect(delta.text_added).toEqual(["updated by the page"]);
    expect(delta.text_removed).toEqual(["idle"]);
    expect(delta.rehydrate).toBeUndefined();
    await readyForReview(page, model);
    // A fingerprint nobody holds: the full page, with the reason.
    const full = lastJson<Record<string, any>>([model.toolTexts[5]!], '"mode"');
    expect(full.mode).toBe("full");
    expect(full.rehydrate.reason).toBe("FINGERPRINT_UNKNOWN");
    expect(full.rehydrate.since_fingerprint).toBe("0".repeat(64));
    expect(full.entities.length).toBeGreaterThan(5);
    // The same-process frame's field took the text; the cross-process frame's button was clicked.
    expect(model.toolTexts[4]).toContain("status: SUCCESS");
    expect(model.toolTexts[4]).toContain('"held":true');
    expect(model.toolTexts[6]).toContain("status: SUCCESS");
    expect(model.toolTexts[6]).toContain("Inner B clicked");
    // The Core's record carries the observer's counters.
    const runtime = await eventsOf(page, "BrowserPageObserved");
    expect(runtime.length).toBeGreaterThan(0);
    expect(runtime.every((e) => /^[0-9a-f]{64}$/.test(e.page_ref))).toBe(true);
    await closeApp(app);
  } finally {
    model.server.close();
    site.server.close();
    other.server.close();
  }
});

test("a page that mutates thousands of nodes a second, and tries to take the observer away, is reported at a bounded rate", async () => {
  test.setTimeout(180_000);
  const repo = fixtureRepo();
  const site = await serve((url) =>
    url.startsWith("/flood")
      ? `<!doctype html><html><head><title>Flood</title></head><body><main><h1>Flood</h1><div id="sink"></div></main><script>
        // The page does what it can to blind the observer: it removes and replaces the MutationObserver, freezes the prototypes, and floods the DOM.
        try { delete window.MutationObserver; window.MutationObserver = function () { this.observe = function () {}; this.disconnect = function () {}; }; Object.freeze(Node.prototype); Object.freeze(Document.prototype); } catch (e) {}
        var n = 0, t0 = Date.now();
        var timer = setInterval(function () { var sink = document.getElementById('sink'); for (var i = 0; i < 400; i++) { var d = document.createElement('div'); d.textContent = 'row ' + (n++); sink.appendChild(d); } if (sink.childNodes.length > 3000) sink.textContent = ''; if (Date.now() - t0 > 2500) { clearInterval(timer); document.title = 'Flood over'; } }, 10);
      </script></body></html>`
      : null,
  );
  const done = gate();
  const model = await scriptedModel([
    { calls: [{ name: "plan.update", args: { outcome: "the flood page is read", expected_files: [] } }] },
    { calls: [{ name: "browser.navigate", args: { url: `http://127.0.0.1:${site.port}/flood` } }] },
    async () => {
      await done.opened;
      return { calls: [{ name: "browser.snapshot", args: {} }] };
    },
    { calls: [{ name: "task.complete", args: { summary: "read the flood page", self_review: { findings: [] } } }] },
  ]);
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-flood-"));
  try {
    const { app, page } = await launch(dataDir, { MODBIT_OPENAI_BASE_URL: model.url, MODBIT_BROWSER_ALLOW_TARGETS: "127.0.0.1" });
    await collectEvents(page);
    const { bsid } = await startBrowserTask(page, repo, "read the flood page");
    await expect.poll(() => model.toolTexts.length, { timeout: 60_000 }).toBe(2);
    const t0 = Date.now();
    // The page changes for about 2.5 seconds; thousands of nodes a second.
    await expect.poll(async () => (await page.evaluate((id) => window.modbit.describeBrowser(id), bsid))?.title, { timeout: 30_000 }).toBe("Flood over");
    const seconds = (Date.now() - t0) / 1000 + 2.5;
    await sleep(1500);
    const events = (await eventsOf(page, "BrowserPageChanged")).filter((e) => e.added > 0 || e.changed > 0 || e.text_added > 0);
    const described = await page.evaluate((id) => window.modbit.describeBrowser(id), bsid);
    // The observer saw the flood (its counter moved on every report) although the page replaced MutationObserver ...
    expect(described!.changeSeq).toBeGreaterThan(5);
    expect(events.length).toBeGreaterThan(0);
    // ... and what reached the Core is bounded: a few reads a second at most, however many nodes the page touched.
    expect(events.length).toBeLessThanOrEqual(Math.ceil(seconds * 3) + 2);
    // Reports are folded: far more reports than reads reached the Core.
    expect(described!.changeSeq).toBeGreaterThan(events.length);
    done.open();
    await readyForReview(page, model);
    await closeApp(app);
  } finally {
    model.server.close();
    site.server.close();
  }
});
