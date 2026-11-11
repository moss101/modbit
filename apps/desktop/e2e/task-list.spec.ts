/**
 * The measured task list (DR-PX-2026-11-11-021 MWS-04; QUAL-PX-199 / PX-E2E-199) on the real Electron build against a real
 * Core, with a scripted OpenAI-compatible model standing in for the provider only: two projects, tasks in the classes the Core
 * reaches here (needs attention, ready for review and unseen, running, draft) and one child task. The list reads in the order
 * of MWS-04 at three widths; a row is one line with the state at the right and keeps a second line only where the task needs the
 * person; grouping by status, environment and date regroups in place; the keyboard alone reaches every row, pins, archives with
 * undo and opens the preview card; the card reads nothing the list does not already hold. Retained mutations show each check
 * failing. The Core has no path to the Failed class today (nothing emits TaskFailed), so that row is the gallery's fixture
 * (gallery.spec.ts) and is not claimed here.
 */
import { expect, test, type ElectronApplication, type Page } from "@playwright/test";
import { mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import { createServer, type Server } from "node:http";
import { tmpdir } from "node:os";
import { basename, join } from "node:path";
import { COMPACT, GEOMETRY, TYPE, densityMetrics, type Density } from "@modbit/design-tokens";
import { agentRow, openListMore, openListTools, queueTask, runTask, setDensity, toNewTask } from "./support/new-task.ts";
import { accessible, box, closeApp, focusedTestId, launch, makeRepo, setContentSize } from "./support/ui-harness.ts";

const NAV = ["agents-new-task", "open-inbox", "open-automations", "open-customize"];
/** What MWS-04 removed: the Repositories section and its rows, and the Projects heading (the Tasks head took its controls). */
const REMOVED = ["agent-repositories", "agent-repository"];

type Reply = { text?: string; calls?: { name: string; args: unknown }[]; hold?: boolean; wait?: Promise<void> };

/** A scripted model with one script per needle (a task's goal tells its requests from another's), indexed by the tool results so far. A `hold` keeps the stream open: the task keeps running. */
function scriptedModels(byNeedle: [string, Reply[]][]): Promise<{ server: Server; url: string }> {
  return new Promise((done) => {
    const server = createServer((req, res) => {
      let body = "";
      req.on("data", (c) => (body += c));
      req.on("end", () => {
        const parsed = JSON.parse(body || "{}") as { messages?: { role: string }[] };
        const results = (parsed.messages ?? []).filter((m) => m.role === "tool").length;
        const reply = byNeedle.find(([needle]) => body.includes(needle))?.[1][results] ?? { text: "Nothing further." };
        res.writeHead(200, { "content-type": "text/event-stream" });
        const send = (o: unknown) => res.write(`data: ${JSON.stringify(o)}\n\n`);
        void (async () => {
          if (reply.wait) await reply.wait;
          if (reply.text) send({ id: "c", model: "scripted", choices: [{ index: 0, delta: { content: reply.text }, finish_reason: null }] });
          if (reply.hold) return;
          (reply.calls ?? []).forEach((c, i) => send({ id: "c", model: "scripted", choices: [{ index: 0, delta: { tool_calls: [{ index: i, id: `call_${results}_${i}`, type: "function", function: { name: c.name, arguments: JSON.stringify(c.args) } }] }, finish_reason: null }] }));
          send({ id: "c", model: "scripted", choices: [{ index: 0, delta: {}, finish_reason: reply.calls?.length ? "tool_calls" : "stop" }], usage: { prompt_tokens: 10, completion_tokens: 5 } });
          res.write("data: [DONE]\n\n");
          res.end();
        })();
      });
    });
    server.listen(0, "127.0.0.1", () => done({ server, url: `http://127.0.0.1:${(server.address() as { port: number }).port}` }));
  });
}

interface Populated {
  app: ElectronApplication;
  page: Page;
  server: Server;
  repoA: string;
  repoB: string;
  ids: { draft1: string; draft2: string; ready: string; attention: string; parent: string; child: string };
}

/** The app with two projects and six tasks: two drafts in one, and in the other a task ready for review and unseen with two changed files, one waiting on an approval, and a running task with a running child. Home is open. */
async function populate(name: string): Promise<Populated> {
  const base = mkdtempSync(join(tmpdir(), `modbit-e2e-list-${name}-`));
  const repoA = join(base, "modbit");
  const repoB = join(base, "dossier");
  for (const r of [repoA, repoB]) {
    mkdirSync(join(r, "src"), { recursive: true });
    writeFileSync(join(r, "src", ".keep"), "");
    makeRepo(r);
  }
  let finish!: () => void;
  const finished = new Promise<void>((r) => (finish = r));
  const model = await scriptedModels([
    ["Task goal: Re-run the px135 summary", [{ calls: [{ name: "plan.update", args: { outcome: "summary re-run", expected_files: [] } }] }, { text: "Re-running the summary.", hold: true }]],
    ["Add glm-5.3-flash to the price catalog", [
      { text: "Found the gap.", calls: [{ name: "plan.update", args: { outcome: "the catalog prices glm-5.3-flash", expected_files: ["notes.txt"], steps: [{ id: "a", title: "Re-run the px135 summary" }] } }] },
      { calls: [{ name: "agent.spawn", args: { idempotency_key: "child-a", objective: "Re-run the px135 summary", write_scope: ["src/"], work_node: "a", max_turns: 6 } }] },
      { calls: [{ name: "agent.wait", args: { idempotency_key: "child-a", timeout_ms: 600000 } }] },
    ]],
    ["Clean the build output", [{ text: "Cleaning.", calls: [{ name: "shell.exec", args: { argv: ["git", "clean", "-fd"] } }] }]],
    ["Fix accounting reconciliation", [
      { calls: [{ name: "plan.update", args: { outcome: "both legs match", expected_files: ["notes.txt", "src/legs.rs"] } }] },
      { calls: [{ name: "fs.read", args: { path: "notes.txt" } }] },
      { calls: [{ name: "change.apply", args: { path: "notes.txt", op: "replace", content: "line 1\nline 2 changed\nline 3\nline 4\n" } }] },
      { calls: [{ name: "change.apply", args: { path: "src/legs.rs", op: "replace", content: "pub fn key(attempt: u32, leg: u32) -> (u32, u32) {\n    (attempt, leg)\n}\n" } }] },
      { wait: finished, text: "Both legs now match.", calls: [{ name: "task.complete", args: { summary: "matched on attempt and leg", self_review: { findings: [] } } }] },
    ]],
  ]);
  const { app, page } = await launch(mkdtempSync(join(tmpdir(), `modbit-e2e-list-${name}-data-`)), { env: { MODBIT_OPENAI_BASE_URL: model.url, MODBIT_CAPACITY: "model=6,provider=8" } });
  await setContentSize(app, page, 1280, 800);
  const draft1 = await queueTask(page, "Draft the SBOM decision record", repoB);
  const draft2 = await queueTask(page, "Gate threshold profile review", repoB);
  // Left before its result arrives, so the result is unseen.
  const ready = await runTask(page, "Fix accounting reconciliation in the px133 rerun", repoA);
  await expect(agentRow(page, ready)).toHaveAttribute("data-class", "RUNNING", { timeout: 60_000 });
  await toNewTask(page);
  finish();
  await expect(agentRow(page, ready)).toHaveAttribute("data-class", "READY_FOR_REVIEW_UNSEEN", { timeout: 90_000 });
  // The first result opens the Review; back out of it.
  await expect(page.getByTestId("review")).toBeVisible({ timeout: 15_000 });
  await page.keyboard.press("Escape");
  await expect(page.getByTestId("review")).toHaveCount(0);
  const attention = await runTask(page, "Clean the build output", repoA);
  await expect(agentRow(page, attention)).toHaveAttribute("data-class", "NEEDS_ATTENTION", { timeout: 60_000 });
  const parent = await runTask(page, "Add glm-5.3-flash to the price catalog", repoA);
  const childRow = page.locator('[data-testid="agent-row"][data-subagent="true"]');
  await expect(childRow).toHaveCount(1, { timeout: 90_000 });
  await expect(childRow).toHaveAttribute("data-class", "RUNNING", { timeout: 60_000 });
  await expect(agentRow(page, parent)).toHaveAttribute("data-class", "RUNNING");
  const child = (await childRow.getAttribute("data-task-id"))!;
  await toNewTask(page);
  await page.mouse.move(900, 400);
  await expect(page.getByTestId("agent-row")).toHaveCount(6);
  // The row's reason is the Core's attention view, which the app re-reads once the tasks' events pause (use-app.ts): until that
  // read lands the row says the task's own next action. The list is measured once the Core's words are on it.
  await expect(agentRow(page, attention).getByTestId("agent-facts")).toContainText("asks for a", { timeout: 30_000 });
  return { app, page, server: model.server, repoA, repoB, ids: { draft1, draft2, ready, attention, parent, child } };
}

async function done(p: Populated): Promise<void> {
  await closeApp(p.app);
  p.server.closeAllConnections();
  p.server.close();
}

interface RowFacts {
  id: string;
  cls: string;
  title: string;
  lines: number;
  depth: number;
  group: string;
  y: number;
  h: number;
  openX: number;
  openRight: number;
  titleH: number;
  titleX: number;
  /** Where a child task's mark starts; null on a task of its own. */
  markX: number | null;
  fade: string;
  wrap: string;
  status: string;
  facts: string | null;
  factsShown: boolean;
  end: string[];
  changes: string | null;
  name: string;
  radius: string;
}

/** Every row the list has in the document, top to bottom, as measured. */
async function rowsOf(page: Page): Promise<RowFacts[]> {
  return page.getByTestId("agent-row").evaluateAll((els) =>
    els
      .map((e) => {
        const q = (id: string) => e.querySelector<HTMLElement>(`[data-testid="${id}"]`);
        const r = e.getBoundingClientRect();
        const open = q("agent-open")!;
        const title = q("agent-title")!;
        const facts = q("agent-facts");
        // The heading above the row: the last group heading at or before it in the list's canvas.
        const heads = [...e.parentElement!.querySelectorAll<HTMLElement>(".agents-group")].filter((g) => g.getBoundingClientRect().y <= r.y);
        const head = heads.sort((a, b) => b.getBoundingClientRect().y - a.getBoundingClientRect().y)[0];
        return {
          id: e.getAttribute("data-task-id")!,
          cls: e.getAttribute("data-class")!,
          title: title.textContent ?? "",
          lines: Number(e.getAttribute("data-lines")),
          depth: Number(e.getAttribute("data-depth")),
          group: head?.querySelector(".agents-section-label")?.textContent ?? "",
          y: r.y,
          h: r.height,
          openX: open.getBoundingClientRect().x,
          openRight: open.getBoundingClientRect().right,
          titleH: title.getBoundingClientRect().height,
          titleX: title.getBoundingClientRect().x,
          markX: q("agent-child-mark")?.getBoundingClientRect().x ?? null,
          fade: getComputedStyle(title).maskImage || getComputedStyle(title).webkitMaskImage,
          wrap: getComputedStyle(title).whiteSpace,
          status: q("agent-status")?.textContent?.trim() ?? "",
          facts: facts ? facts.innerText.trim() : null,
          factsShown: facts !== null && facts.getBoundingClientRect().height > 0,
          // The marks at the right, as they are seen from left to right.
          end: [...e.querySelector(".agents-end")!.children].sort((a, b) => a.getBoundingClientRect().x - b.getBoundingClientRect().x).map((c) => c.getAttribute("data-testid") ?? ""),
          changes: q("agent-changes")?.textContent ?? null,
          name: open.getAttribute("aria-label") ?? "",
          radius: getComputedStyle(open).borderTopLeftRadius,
        };
      })
      .sort((a, b) => a.y - b.y),
  );
}

const NEEDS_THE_PERSON = new Set(["NEEDS_ATTENTION", "FAILED"]);

/** What the rows break of MWS-04 in a density with Row details at its default (empty when the record holds). */
async function listDrift(page: Page, density: Density = "comfortable"): Promise<string[]> {
  const bad: string[] = [];
  const m = densityMetrics(density);
  const chrome = density === "compact" ? COMPACT.type.chrome : TYPE.chrome;
  const list = await box(page, "region-agents");
  const rows = await rowsOf(page);
  if (rows.length === 0) bad.push("the list has no row");
  rows.forEach((r, i) => {
    const what = `${r.cls} "${r.title.slice(0, 24)}"`;
    const needs = NEEDS_THE_PERSON.has(r.cls);
    if (needs && r.lines !== 2) bad.push(`${what} needs the person but is one line`);
    if (needs && (!r.factsShown || !r.facts || r.facts === "·")) bad.push(`${what} needs the person but shows no reason`);
    if (!needs && r.lines !== 1) bad.push(`quiet row ${what} has a second line`);
    const want = r.lines === 2 ? m.rowTwoLine : m.row;
    if (Math.abs(r.h - want) > 0.5) bad.push(`${what} is ${r.h} high, not ${want}`);
    if (r.radius !== `${GEOMETRY.rowRadius}px`) bad.push(`${what} radius ${r.radius}`);
    if (Math.abs(r.openX - list.x - GEOMETRY.rowInset) > 0.5 || Math.abs(list.x + list.width - r.openRight - GEOMETRY.rowInset) > 1.5) bad.push(`${what} inset ${r.openX - list.x} and ${list.x + list.width - r.openRight}`);
    // The title is one line and fades at its end instead of wrapping or being cut with dots.
    if (r.wrap !== "nowrap" || Math.abs(r.titleH - chrome.line) > 1 || !r.fade.includes("gradient")) bad.push(`${what} title ${JSON.stringify({ wrap: r.wrap, h: r.titleH, fade: r.fade.slice(0, 24) })}`);
    if (r.status === "") bad.push(`${what} has no status words`);
    // At the right: the state's glyph, then the changed files of a task ready for review, then the unread dot.
    const order = r.end.map((id) => (id === "agent-glyph" || id === "agent-attention-dot" ? 0 : id === "agent-changes" ? 1 : id === "agent-unread-dot" ? 2 : 9));
    if (order[0] !== 0 || order.some((v, k) => v === 9 || (k > 0 && v <= order[k - 1]!))) bad.push(`${what} marks are ${r.end.join(" ")}`);
    if (r.changes !== null && !r.cls.startsWith("READY_FOR_REVIEW")) bad.push(`${what} shows a file count without being ready for review`);
    const prev = rows[i - 1];
    if (prev && prev.group === r.group && Math.abs(r.y - (prev.y + prev.h) - m.rowGap) > 0.5) bad.push(`${what} is ${r.y - (prev.y + prev.h)} from the row above`);
    if (r.depth > 0) {
      // A child task: under a row one step out (its parent, or the parent's last child), its mark indented past that row's title.
      const parent = rows.slice(0, i).reverse().find((x) => x.depth === r.depth - 1 && x.group === r.group);
      if (!prev || prev.depth < r.depth - 1 || !parent || r.markX === null || r.markX < parent.titleX + 12 || r.titleX <= r.markX) bad.push(`child ${what} is not indented under its parent`);
    } else if (r.markX !== null) bad.push(`${what} is a task of its own but carries a child's mark`);
  });
  const heads = await page.locator(".agents-group").evaluateAll((els) => els.map((e) => e.getBoundingClientRect().height));
  for (const h of heads) if (Math.abs(h - m.groupHead) > 0.5) bad.push(`a group heading is ${h} high, not ${m.groupHead}`);
  return bad;
}

/** What breaks the order of MWS-04 (empty when the list reads top to bottom as the record says and nothing removed is back). */
async function orderDrift(page: Page): Promise<string[]> {
  const bad: string[] = [];
  const y = async (id: string) => (await page.getByTestId(id).first().boundingBox())?.y ?? -1;
  const order = [await y("toggle-agent-list"), ...(await Promise.all(NAV.map(y))), await y("agents-head"), await y("agents-states"), await y("agents-scroll"), await y("agent-foot")];
  if (order.some((v, i) => v < 0 || (i > 0 && v <= order[i - 1]!))) bad.push(`the list's order is ${order.join(" ")}`);
  const nav = await page.getByTestId("agent-nav").locator("button").evaluateAll((els) => els.map((e) => `${e.getAttribute("data-testid")}:${(e.querySelector(".agent-nav-label")?.textContent ?? "").trim()}`));
  if (nav.join(" | ") !== "agents-new-task:New task | open-inbox:Inbox | open-automations:Automations | open-customize:Customize") bad.push(`the navigation rows are ${nav.join(" | ")}`);
  const head = await page.getByTestId("agents-head").locator("button").evaluateAll((els) => els.map((e) => e.getAttribute("data-testid")));
  if (head.join(" ") !== "agents-search-open agents-tools-toggle agents-more") bad.push(`the Tasks head holds ${head.join(" ")}`);
  if ((await page.getByTestId("agents-head").locator("h2").textContent())?.trim() !== "Tasks") bad.push("the Tasks head does not say Tasks");
  const foot = await page.getByTestId("agent-foot").locator("[data-testid]").evaluateAll((els) => els.map((e) => e.getAttribute("data-testid")));
  for (const id of ["overflow-menu", "core-status", "open-help", "open-settings"]) if (!foot.includes(id)) bad.push(`${id} is not in the person's row`);
  for (const id of REMOVED) if ((await page.getByTestId(id).count()) !== 0) bad.push(`removed surface ${id} is back`);
  if ((await page.locator('[data-testid="agents-section"][data-section="projects"]').count()) !== 0) bad.push("the Projects heading is back");
  if ((await page.getByRole("heading", { name: "Repositories" }).count()) !== 0) bad.push("a Repositories heading is back");
  // Nothing is drawn that does nothing: no Review row until its page exists, and the state views sit above the groups.
  if ((await page.getByTestId("agent-nav").getByText("Review", { exact: true }).count()) !== 0) bad.push("a Review navigation row is drawn");
  const foot_ = await box(page, "agent-foot");
  if (Math.abs(foot_.y + foot_.height - (await page.evaluate(() => innerHeight))) > 1) bad.push("the person's row is not at the foot of the list");
  if ((await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)) === false) bad.push("the window scrolls sideways");
  return bad;
}

const sections = (page: Page) => page.getByTestId("agents-section").evaluateAll((els) => els.map((e) => e.getAttribute("data-section")!));
const mutate = (page: Page, css: string) => page.evaluate((c) => {
  const s = document.createElement("style");
  s.id = "list-mutation";
  s.textContent = c;
  document.head.appendChild(s);
}, css);
const restore = (page: Page) => page.evaluate(() => document.getElementById("list-mutation")?.remove());
const setTheme = async (page: Page, theme: "light" | "dark" | "high-contrast") => {
  await page.evaluate((t) => document.documentElement.setAttribute("data-theme", t), theme);
  await page.evaluate(() => new Promise<void>((r) => requestAnimationFrame(() => requestAnimationFrame(() => r()))));
};

/** Records the channel of every bridge call the renderer makes from now on, in the main process: each IPC handler is wrapped in place, and no product code is involved. */
async function recordBridgeCalls(app: ElectronApplication): Promise<void> {
  await app.evaluate(({ ipcMain }) => {
    const g = globalThis as unknown as { __bridgeLog?: string[] };
    if (g.__bridgeLog) return;
    const handlers = (ipcMain as unknown as { _invokeHandlers?: Map<string, (...a: unknown[]) => unknown> })._invokeHandlers;
    if (!(handlers instanceof Map) || handlers.size === 0) throw new Error("this Electron has no invoke-handler table to record the bridge on");
    const log: string[] = (g.__bridgeLog = []);
    for (const [channel, fn] of [...handlers]) {
      handlers.set(channel, (...a: unknown[]) => {
        log.push(channel);
        return fn(...a);
      });
    }
  });
}
const bridgeCalls = (app: ElectronApplication, since: number): Promise<string[]> => app.evaluate((_e, n) => ((globalThis as unknown as { __bridgeLog?: string[] }).__bridgeLog ?? []).slice(n), since);
const bridgeMark = async (app: ElectronApplication): Promise<number> => (await bridgeCalls(app, 0)).length;

const focusedTask = (page: Page) => page.evaluate(() => document.activeElement?.closest('[data-testid="agent-row"]')?.getAttribute("data-task-id") ?? "");
const expectFocusedTask = (page: Page, id: string, why?: string) => expect.poll(() => focusedTask(page), { message: why ?? `focused task ${id}`, timeout: 5_000 }).toBe(id);

test("PX-199: the task list reads in the order of MWS-04 at three widths; rows are one line with the state at the right and keep a reason only where the task needs the person; status, environment and date regroup in place; both densities; the gate fails on each mutation", async () => {
  test.setTimeout(300_000);
  const p = await populate("order");
  const { app, page, ids } = p;
  try {
    // ---- The order of MWS-04, at three widths, with nothing removed drawn again.
    for (const [w, h] of [[1280, 800], [1000, 700], [900, 600]] as const) {
      await setContentSize(app, page, w, h);
      expect(await orderDrift(page), `the list's order at ${w} x ${h}`).toEqual([]);
      expect(await listDrift(page), `the rows at ${w} x ${h}`).toEqual([]);
    }
    await setContentSize(app, page, 1280, 800);
    // Inbox is the Approvals page under its new name: its count is the Core's attention count, and its row opens the same page.
    const sid = (await page.evaluate(() => window.modbit.localState())).sessionId!;
    const attentionTasks = new Set((await page.evaluate((s) => window.modbit.attention(s), sid)).items.map((i) => i.taskId));
    expect([...attentionTasks], "the Core's attention view holds the one task that waits on an approval").toEqual([ids.attention]);
    await expect(page.getByTestId("open-inbox-count")).toHaveText(String(attentionTasks.size));

    // ---- Groups are projects by default: one per repository here, each a place whose own menu starts a task there.
    await expect(page.getByTestId("agent-list")).toHaveAttribute("data-grouping", "repository");
    const groups = await page.locator(".agents-section-label").allTextContents();
    expect(groups).toEqual([basename(p.repoA), basename(p.repoB)]);
    const rows = await rowsOf(page);
    expect(rows.map((r) => r.group)).toEqual([basename(p.repoA), basename(p.repoA), basename(p.repoA), basename(p.repoA), basename(p.repoB), basename(p.repoB)]);
    // The rows, as the Core classed them.
    const byId = new Map(rows.map((r) => [r.id, r]));
    expect(byId.get(ids.attention)!.lines, "a task that needs an approval keeps its second line").toBe(2);
    expect(byId.get(ids.attention)!.facts, "with the Core's reason on it").toContain("asks for a");
    expect(byId.get(ids.attention)!.status).toBe("Needs attention");
    expect(byId.get(ids.attention)!.end[0], "the state's glyph is the mark of the pending approval").toBe("agent-attention-dot");
    expect(byId.get(ids.ready)!.lines).toBe(1);
    expect(byId.get(ids.ready)!.end, "ready and unseen: the glyph, the changed files, the unread dot").toEqual(["agent-glyph", "agent-changes", "agent-unread-dot"]);
    const header = (await page.evaluate((s) => window.modbit.agentHeaders(s, false), sid)).headers.find((x) => x.taskId === ids.ready)!;
    expect(header.filesChanged, "the Core's own count").toBe(2);
    expect(byId.get(ids.ready)!.changes).toBe(String(header.filesChanged));
    expect(byId.get(ids.ready)!.name).toContain("2 files changed");
    for (const id of [ids.parent, ids.child, ids.draft1, ids.draft2]) expect(byId.get(id)!.lines, `${byId.get(id)!.cls} is one line`).toBe(1);
    expect(byId.get(ids.draft1)!.end).toEqual(["agent-glyph"]);
    // The child sits directly under its parent, indented, with its mark; its name says whose child it is.
    const at = rows.findIndex((r) => r.id === ids.parent);
    expect(rows[at + 1]!.id, "the child follows its parent").toBe(ids.child);
    expect([byId.get(ids.parent)!.depth, byId.get(ids.child)!.depth]).toEqual([0, 1]);
    expect(byId.get(ids.child)!.titleX).toBeGreaterThan(byId.get(ids.parent)!.titleX + 12);
    await expect(agentRow(page, ids.child).getByTestId("agent-child-mark")).toBeVisible();
    await expect(agentRow(page, ids.parent).getByTestId("agent-child-mark")).toHaveCount(0);
    expect(byId.get(ids.child)!.name).toContain("child task of Add glm-5.3-flash to the price catalog");
    // Status is never colour alone: every row has one glyph and its words.
    for (const row of await page.getByTestId("agent-row").all()) expect(await row.locator(".mb-dot-glyph svg").count()).toBe(1);

    // ---- Pin and archive show under the pointer and on keyboard focus, in place of the row's marks.
    const draft = agentRow(page, ids.draft1);
    await expect(draft.getByTestId("agent-pin")).toBeHidden();
    await expect(draft.getByTestId("agent-archive")).toBeHidden();
    await draft.hover();
    await expect(draft.getByTestId("agent-pin")).toBeVisible();
    await expect(draft.getByTestId("agent-archive")).toBeVisible();
    await expect(draft.locator(".agents-end")).toBeHidden();
    await page.mouse.move(900, 400);
    await expect(draft.getByTestId("agent-pin")).toBeHidden();
    await draft.getByTestId("agent-open").focus();
    await expect(draft.getByTestId("agent-pin"), "on keyboard focus too").toBeVisible();
    await expect(draft.getByTestId("agent-archive")).toBeVisible();
    await page.evaluate(() => (document.activeElement as HTMLElement | null)?.blur());

    // ---- A project group's own menu starts a task there (the Repositories section did this).
    const groupA = page.locator(".agents-group").filter({ hasText: basename(p.repoA) });
    await groupA.hover();
    await groupA.getByTestId("group-menu").click();
    await expect(page.getByTestId("menu-item-new-task-here")).toHaveText(`New task in ${basename(p.repoA)}`);
    await page.getByTestId("menu-item-new-task-here").click();
    await expect.poll(() => focusedTestId(page)).toBe("goal");
    expect(basename(await page.getByTestId("workspace").inputValue())).toBe(basename(p.repoA));
    const groupB = page.locator(".agents-group").filter({ hasText: basename(p.repoB) });
    await groupB.hover();
    await groupB.getByTestId("group-menu").click();
    await page.getByTestId("menu-item-new-task-here").click();
    expect(basename(await page.getByTestId("workspace").inputValue())).toBe(basename(p.repoB));
    await page.mouse.move(900, 400);

    // ---- Grouping by status, environment and date regroups in place: no reload, no page change, the same six tasks.
    await page.evaluate(() => ((window as unknown as { __sameDocument: boolean }).__sameDocument = true));
    await openListTools(page);
    const options = await page.getByTestId("agents-grouping").locator("option").evaluateAll((els) => els.map((e) => `${(e as HTMLOptionElement).value}:${e.textContent}`));
    expect(options.slice(0, 4), "projects first, then the record's three options").toEqual(["repository:Group: Project", "status:Group: Status", "location:Group: Environment", "time:Group: Date"]);
    await page.getByTestId("agents-grouping").selectOption("status");
    expect(await sections(page)).toEqual(["status:NEEDS_ATTENTION", "status:READY_FOR_REVIEW_UNSEEN", "status:RUNNING", "status:DRAFT"]);
    expect(await page.locator(".agents-section-label").allTextContents()).toEqual(["Needs attention", "Ready for review", "Running", "Draft"]);
    // A status always says how many tasks it holds; the child is under its parent among the running ones.
    expect(await page.locator(".agents-section-count").allTextContents()).toEqual(["1", "1", "2", "2"]);
    const byStatus = await rowsOf(page);
    expect(byStatus.filter((r) => r.group === "Running").map((r) => [r.id, r.depth])).toEqual([[ids.parent, 0], [ids.child, 1]]);
    expect(await listDrift(page), "grouped by status").toEqual([]);
    await expect(page.locator(".agents-group").first().getByTestId("group-menu"), "a status is not a place: it has no menu").toHaveCount(0);
    await page.getByTestId("agents-grouping").selectOption("location");
    expect(await sections(page)).toEqual(["location:local"]);
    expect(await page.locator(".agents-section-label").allTextContents()).toEqual(["Local"]);
    await expect(page.getByTestId("agent-row")).toHaveCount(6);
    expect(await listDrift(page), "grouped by environment").toEqual([]);
    await page.getByTestId("agents-grouping").selectOption("time");
    expect(await sections(page)).toEqual(["time:today"]);
    await expect(page.getByTestId("agent-row")).toHaveCount(6);
    expect(await listDrift(page), "grouped by date").toEqual([]);
    // Collapsing a group hides its rows and the heading says how many.
    await page.locator('[data-testid="agents-section"][data-section="time:today"] button').click();
    await expect(page.getByTestId("agent-row")).toHaveCount(0);
    await expect(page.locator('[data-testid="agents-section"][data-section="time:today"] button')).toHaveAttribute("aria-expanded", "false");
    await page.locator('[data-testid="agents-section"][data-section="time:today"] button').click();
    await expect(page.getByTestId("agent-row")).toHaveCount(6);
    await page.getByTestId("agents-grouping").selectOption("repository");
    expect(await page.locator(".agents-section-label").allTextContents()).toEqual([basename(p.repoA), basename(p.repoB)]);
    await page.getByTestId("agents-tools-toggle").click();
    await expect(page.getByTestId("agents-controls")).toHaveCount(0);
    expect(await page.evaluate(() => (window as unknown as { __sameDocument?: boolean }).__sameDocument), "the page was never reloaded").toBe(true);
    await expect(page.getByTestId("goal"), "and home is still what the centre shows").toBeVisible();

    // ---- Row details: when needed (the default), always, never. The reason stays in the name and the tooltip whatever the row shows.
    await expect(page.getByTestId("agent-list")).toHaveAttribute("data-row-details", "auto");
    await openListMore(page);
    await expect(page.getByTestId("menu-item-row-details-auto")).toHaveAttribute("aria-checked", "true");
    await page.getByTestId("menu-item-row-details-always").click();
    await expect(page.getByTestId("agent-list")).toHaveAttribute("data-row-details", "always");
    expect((await rowsOf(page)).map((r) => r.lines), "every row keeps its second line").toEqual([2, 2, 2, 2, 2, 2]);
    // The retained mutation: a quiet row with a second line fails the gate (five quiet rows here).
    expect((await listDrift(page)).filter((d) => d.startsWith("quiet row")).length, "quiet rows with a second line fail").toBe(5);
    expect((await rowsOf(page)).every((r) => Math.abs(r.h - GEOMETRY.rowTwoLine) <= 0.5), "and the virtual list gave each the two-line height").toBe(true);
    await openListMore(page);
    await page.getByTestId("menu-item-row-details-never").click();
    await expect(page.getByTestId("agent-list")).toHaveAttribute("data-row-details", "never");
    const never = await rowsOf(page);
    expect(never.map((r) => r.lines)).toEqual([1, 1, 1, 1, 1, 1]);
    expect(never.find((r) => r.id === ids.attention)!.name, "the reason is still in the row's name").toContain("asks for a");
    expect(await agentRow(page, ids.attention).getByTestId("agent-open").getAttribute("title")).toContain("Next: ");
    await expect(agentRow(page, ids.attention).getByTestId("agent-attention-dot"), "and the mark of the pending approval stays").toBeVisible();
    expect((await listDrift(page)).some((d) => d.includes("needs the person but is one line")), "a row that needs the person on one line fails the default gate").toBe(true);
    // The choice is a stored preference of this viewer.
    await page.reload();
    await expect(page.getByTestId("core-status")).toContainText("Core connected", { timeout: 60_000 });
    await expect(page.getByTestId("agent-row")).toHaveCount(6, { timeout: 30_000 });
    await expect(page.getByTestId("agent-list")).toHaveAttribute("data-row-details", "never");
    await openListMore(page);
    await page.getByTestId("menu-item-row-details-auto").click();
    await expect(page.getByTestId("agent-list")).toHaveAttribute("data-row-details", "auto");
    await page.mouse.move(900, 400);
    // After the reload the reason is the Core's again, not this window's memory.
    await expect(agentRow(page, ids.attention).getByTestId("agent-facts")).toContainText("asks for a", { timeout: 30_000 });
    expect(await listDrift(page), "the default again").toEqual([]);

    // ---- The retained mutations on the default list: each breaks one rule and the gate says which.
    const failing = async (css: string) => {
      await mutate(page, css);
      const drift = await listDrift(page);
      await restore(page);
      return drift;
    };
    expect((await failing('[data-testid="agent-facts"] { display: none !important; }')).filter((d) => d.includes("needs the person but shows no reason")).length, "a missing reason fails").toBe(1);
    expect((await failing('.agents-row[data-lines="1"] { height: 50px !important; }')).filter((d) => d.includes("is 50 high, not 32")).length, "the old 50 px row fails, every one").toBe(5);
    expect((await failing(".agents-title { white-space: normal !important; -webkit-mask-image: none !important; mask-image: none !important; }")).some((d) => d.includes("title")), "a wrapped title without the fade fails").toBe(true);
    expect((await failing(".agents-end { flex-direction: row-reverse !important; }")).some((d) => d.includes("marks are")), "the unread dot before the glyph fails").toBe(true);
    expect((await failing(".agents-open { padding-left: 10px !important; }")).some((d) => d.startsWith("child")), "a child that is not indented fails").toBe(true);
    expect((await failing(":root { --mb-row-inset: 0px; }")).some((d) => d.includes("inset")), "rows flush with the list's edge fail").toBe(true);
    expect(await listDrift(page), "clean again").toEqual([]);

    // ---- Both densities: the same list at the Compact tokens, and axe on the list and on home in three themes.
    for (const density of ["compact", "comfortable"] as const) {
      await setDensity(page, density);
      await page.mouse.move(900, 400);
      await expect(page.getByTestId("agent-row")).toHaveCount(6);
      expect(await listDrift(page, density), `the rows in ${density}`).toEqual([]);
      expect(await orderDrift(page), `the order in ${density}`).toEqual([]);
      for (const theme of ["light", "dark", "high-contrast"] as const) {
        await setTheme(page, theme);
        await accessible(page, `the task list in ${theme}, ${density}`, '[data-testid="region-agents"]');
        await accessible(page, `home with the list in ${theme}, ${density}`);
      }
      await setTheme(page, "light");
    }
    expect((await listDrift(page, "compact")).length, "Comfortable measured against the Compact numbers").toBeGreaterThan(0);
  } finally {
    await done(p);
  }
});

test("PX-199: the keyboard alone reaches every row, pins, archives with undo and opens the preview card; the card opens on rest too, reads only what the list holds, and opens its task", async () => {
  test.setTimeout(300_000);
  const p = await populate("keys");
  const { app, page, ids } = p;
  try {
    await recordBridgeCalls(app);
    const order = (await rowsOf(page)).map((r) => r.id);
    expect(order).toEqual([ids.parent, ids.child, ids.attention, ids.ready, ids.draft2, ids.draft1]);

    // ---- Every row by the keyboard: the arrows from nowhere, then j and k; the ends hold.
    await page.evaluate(() => (document.activeElement as HTMLElement | null)?.blur());
    await page.keyboard.press("ArrowDown");
    await expectFocusedTask(page, order[0]!, "the first row from nowhere");
    for (const id of order.slice(1)) {
      await page.keyboard.press("ArrowDown");
      await expectFocusedTask(page, id);
    }
    await page.keyboard.press("ArrowDown");
    await expectFocusedTask(page, order[5]!, "the last stays the last");
    await page.keyboard.press("k");
    await expectFocusedTask(page, order[4]!);
    await page.keyboard.press("j");
    await expectFocusedTask(page, order[5]!);
    await page.keyboard.press("Home");
    await expectFocusedTask(page, order[0]!);
    await page.keyboard.press("ArrowUp");
    await expectFocusedTask(page, order[0]!, "the top stays the top");
    await page.keyboard.press("End");
    await expectFocusedTask(page, order[5]!);
    // r goes to a running task, a to Inbox on its first item, and Escape comes back.
    await page.keyboard.press("r");
    await expect.poll(() => page.evaluate(() => document.activeElement?.closest('[data-testid="agent-row"]')?.getAttribute("data-class") ?? "")).toBe("RUNNING");
    await page.keyboard.press("a");
    await expect(page.getByTestId("inbox")).toBeVisible();
    await expect(page.getByTestId("top-bar-title")).toHaveText("Inbox");
    await expect.poll(() => focusedTestId(page)).toBe("attention-item");
    await page.getByTestId("open-inbox").focus();
    await page.keyboard.press("Enter");
    await expect(page.getByTestId("inbox")).toHaveCount(0);

    // ---- p pins and unpins from the row.
    await agentRow(page, ids.draft1).getByTestId("agent-open").focus();
    await page.keyboard.press("p");
    await expect(agentRow(page, ids.draft1)).toHaveAttribute("data-pinned", "true");
    expect((await sections(page))[0]).toBe("pinned");
    expect((await rowsOf(page))[0]!.id, "pinned tasks come first").toBe(ids.draft1);
    await expectFocusedTask(page, ids.draft1, "focus stays on the row it pinned");
    await page.keyboard.press("p");
    await expect(agentRow(page, ids.draft1)).toHaveAttribute("data-pinned", "false");
    expect(await sections(page)).not.toContain("pinned");

    // ---- e archives from the row, focus goes to the nearest remaining row, and the toast's Undo brings it back.
    await agentRow(page, ids.draft2).getByTestId("agent-open").focus();
    await page.keyboard.press("e");
    await expect(agentRow(page, ids.draft2)).toHaveCount(0, { timeout: 15_000 });
    await expectFocusedTask(page, ids.draft1, "focus moved to the nearest remaining row");
    const sid = (await page.evaluate(() => window.modbit.localState())).sessionId!;
    expect((await page.evaluate((s) => window.modbit.agentHeaders(s, true), sid)).headers.find((h) => h.taskId === ids.draft2)?.archived, "archived in the Core").toBe(true);
    const undo = page.getByTestId("tray").filter({ hasText: "Archived" });
    await expect(undo).toBeVisible();
    await undo.getByRole("button", { name: "Undo" }).focus();
    await page.keyboard.press("Enter");
    await expect(agentRow(page, ids.draft2)).toBeVisible({ timeout: 15_000 });
    await expect(page.getByTestId("tray").filter({ hasText: "Archived" })).toHaveCount(0);
    expect((await page.evaluate((s) => window.modbit.agentHeaders(s, false), sid)).headers.some((h) => h.taskId === ids.draft2), "restored in the Core").toBe(true);
    await expectFocusedTask(page, ids.draft2, "focus returns to the restored row");

    // ---- Space opens the preview card and Enter the task. From here on the bridge is watched: the card must read nothing.
    await expect(page.getByTestId("goal"), "home is open: no conversation is reading anything").toBeVisible();
    await expect(page.getByTestId("conversation")).toHaveCount(0);
    const mark = await bridgeMark(app);
    await agentRow(page, ids.ready).getByTestId("agent-open").focus();
    await page.keyboard.press("Space");
    const card = page.getByTestId("agent-preview");
    await expect(card).toBeVisible();
    await expect(card).toHaveAttribute("data-task-id", ids.ready);
    await expect(card).toHaveAttribute("data-by", "keyboard");
    await expect.poll(() => focusedTestId(page), { message: "focus moves to the card's Open" }).toBe("agent-preview-open");
    await expect(page.getByTestId("conversation"), "Space did not open the task").toHaveCount(0);
    await expect(agentRow(page, ids.ready)).toHaveAttribute("data-preview", "true");
    // What it says: the state, the size of the change from the header, and Open. Nothing it would have to read for.
    await expect(card.getByTestId("agent-preview-state")).toHaveText("Ready for review");
    await expect(card.getByTestId("agent-preview-title")).toHaveText("Fix accounting reconciliation in the px133 rerun");
    const header = (await page.evaluate((s) => window.modbit.agentHeaders(s, false), sid)).headers.find((h) => h.taskId === ids.ready)!;
    await expect(card.getByTestId("agent-preview-changes")).toHaveText(`+${header.linesAdded} −${header.linesRemoved} · ${header.filesChanged} files`);
    await expect(card.getByTestId("agent-preview-open")).toHaveText("Open");
    // The header's later fields and split view belong to later rows: none of them is drawn here.
    await expect(card.getByRole("button")).toHaveCount(1);
    await expect(card).not.toContainText(/pull request|split view/i);
    // The card sits beside the list, level with its row.
    const rowBox = (await agentRow(page, ids.ready).boundingBox())!;
    const cardBox = (await card.boundingBox())!;
    const listBox = await box(page, "region-agents");
    expect(Math.abs(cardBox.y - rowBox.y), "level with its row").toBeLessThanOrEqual(1);
    expect(cardBox.x, "beside the list, over the centre").toBeGreaterThanOrEqual(listBox.x + listBox.width - GEOMETRY.rowInset - 1);
    expect(cardBox.x).toBeLessThanOrEqual(listBox.x + listBox.width);
    await accessible(page, "the preview card", '[data-testid="agent-preview"]');
    await accessible(page, "home with the preview card open");
    // Escape closes the card and nothing else, and focus returns to the row.
    await page.keyboard.press("Escape");
    await expect(card).toHaveCount(0);
    await expectFocusedTask(page, ids.ready, "focus returns to the row");
    await expect(page.getByTestId("goal")).toBeVisible();
    // Space again closes it from the row as well; leaving the card with Tab closes it.
    await page.keyboard.press("Space");
    await expect(card).toBeVisible();
    await page.keyboard.press("Tab");
    await expect(card).toHaveCount(0);

    // ---- The pointer: resting on a row opens its card; passing over one does not; the card can be entered and left.
    await page.mouse.move(900, 400);
    const attentionRow = agentRow(page, ids.attention);
    const ab = (await attentionRow.boundingBox())!;
    await page.mouse.move(ab.x + 60, ab.y + ab.height / 2);
    await page.mouse.move(900, 400, { steps: 3 });
    await page.waitForTimeout(900);
    await expect(card, "a pointer that only passed opened nothing").toHaveCount(0);
    await page.mouse.move(ab.x + 60, ab.y + ab.height / 2);
    await expect(card).toBeVisible({ timeout: 5_000 });
    await expect(card).toHaveAttribute("data-task-id", ids.attention);
    await expect(card).toHaveAttribute("data-by", "pointer");
    expect(await focusedTestId(page), "resting takes no focus").not.toBe("agent-preview-open");
    await expect(card.getByTestId("agent-preview-state")).toHaveText("Needs attention");
    await expect(card.getByTestId("agent-preview-reason"), "the Core's reason").toContainText("asks for a");
    await expect(card.getByTestId("agent-preview-changes"), "nothing changed: no line for it").toHaveCount(0);
    const cb = (await card.boundingBox())!;
    await page.mouse.move(cb.x + cb.width / 2, cb.y + cb.height / 2, { steps: 4 });
    await page.waitForTimeout(500);
    await expect(card, "the card stays while the pointer is on it").toBeVisible();
    await page.mouse.move(900, 700, { steps: 4 });
    await expect(card).toHaveCount(0);
    // Scrolling the list closes it too (it belongs to a row that moved).
    await page.mouse.move(ab.x + 60, ab.y + ab.height / 2);
    await expect(card).toBeVisible({ timeout: 5_000 });
    await page.getByTestId("agents-scroll").dispatchEvent("scroll");
    await expect(card).toHaveCount(0);
    await page.mouse.move(900, 700);

    // ---- The bridge log: every one of those cards opened and closed without a transcript read (or any conversation read).
    const whileCards = await bridgeCalls(app, mark);
    expect(whileCards.filter((c) => c.startsWith("conversation:")), `no conversation read while the cards were open; the log held ${whileCards.length} calls`).toEqual([]);
    expect(whileCards.length, "the log was recording (the list's own reads are in it)").toBeGreaterThan(0);

    // ---- Open, from the card, opens the task: and that does read the transcript, so the log could have caught it.
    const rb = (await agentRow(page, ids.ready).boundingBox())!;
    await page.mouse.move(rb.x + 60, rb.y + rb.height / 2);
    await expect(card).toBeVisible({ timeout: 5_000 });
    await expect(card).toHaveAttribute("data-task-id", ids.ready);
    const beforeOpen = await bridgeMark(app);
    await card.getByTestId("agent-preview-open").click();
    await expect(page.getByTestId("conversation")).toHaveAttribute("data-task-id", ids.ready);
    await expect(card).toHaveCount(0);
    await expect(page.getByTestId("conversation")).toHaveAttribute("data-loaded", "true", { timeout: 30_000 });
    expect(await bridgeCalls(app, beforeOpen), "opening the task reads its transcript").toContain("conversation:transcript");
    // And from the keyboard: Space, then Enter on Open.
    await toNewTask(page);
    await agentRow(page, ids.attention).getByTestId("agent-open").focus();
    await page.keyboard.press("Space");
    await expect.poll(() => focusedTestId(page)).toBe("agent-preview-open");
    await page.keyboard.press("Enter");
    await expect(page.getByTestId("conversation")).toHaveAttribute("data-task-id", ids.attention);
    await expect(page.getByTestId("approval-card")).toBeVisible({ timeout: 30_000 });
    // Enter on a row still opens the task itself.
    await agentRow(page, ids.draft1).getByTestId("agent-open").focus();
    await page.keyboard.press("Enter");
    await expect(page.getByTestId("conversation")).toHaveAttribute("data-task-id", ids.draft1);
  } finally {
    await done(p);
  }
});
