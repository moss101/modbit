/**
 * The interaction budgets of docs/65 AFW-K01 and docs/39, measured (not merely
 * bounded) at the real boundary: a real Core, a real Electron renderer and a
 * scripted OpenAI-compatible model. Each test records the distribution
 * (n, p50, p95, p99), retains it as a result file (`MODBIT_PERF_OUT`, see
 * support/perf.ts) and asserts the numeric budget the QUAL names.
 *
 *  - QUAL-PX-041 / QUAL-PX-047: event timestamp to render of a streamed delta,
 *    p95 <= 100 ms (every delta, and the first delta of each message);
 *  - QUAL-PX-046: the agent list reflects a Core event within 150 ms at p95,
 *    with a 200-task list;
 *  - QUAL-PX-047: "1,000 rows scroll without dropped frames beyond the budget":
 *    the dossier names no frame budget, so the frame distribution is recorded
 *    and only a stated reading is asserted (see the test).
 *
 * Time base: the Core's event timestamp (`recordedAt`, ms) and the renderer's
 * `performance.timeOrigin + performance.now()` are the same host's wall clock.
 * "Render" is the first animation frame after the DOM shows the event.
 *
 * Profiles (`MODBIT_PERF_PROFILE`): docs/39 states its budgets "on the reference
 * hardware". `reference` (a release-built Core on a quiet machine) asserts every
 * budget. Any other profile (the default, and what CI runs: a debug-built Core on
 * a shared runner) records the distribution and the verdict, which says whether
 * the budget was met on that profile, and asserts only that the measurement is
 * sound. A budget is never changed to pass: a profile that misses it says "NOT MET".
 */
import { expect, test, type Page } from "@playwright/test";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { closeApp, launch, makeRepo, setContentSize } from "./support/ui-harness.ts";
import { dist, pacedModel, retain, type Dist, type Part } from "./support/perf.ts";
import { Gate } from "./support/stream-model.ts";
import { createTask, openConversation, sessionOf, startAndOpen } from "./support/conversation-harness.ts";

const PROFILE = process.env.MODBIT_PERF_PROFILE ?? "ci";
const ENFORCE = PROFILE === "reference";
const num = (k: string, d: number): number => {
  const v = Number(process.env[k]);
  return Number.isFinite(v) && v > 0 ? Math.floor(v) : d;
};

interface Stamp {
  token: string;
  first: boolean;
  seq: number;
  recorded: number;
  main: number;
  ipc: number;
  mut: number;
  frame: number;
}

test("PERF PX-041/PX-047: a streamed delta renders within 100 ms of its event timestamp at p95, every delta and the first of each message", async ({}, info) => {
  test.setTimeout(900_000);
  const TASKS = num("MODBIT_PERF_TASKS", 4);
  const TURNS = num("MODBIT_PERF_TURNS", 50);
  const CHUNKS = 4; // deltas per message
  const GAP_MS = 60; // longer than the Core's 50 ms coalescing window: one event per piece
  const repo = makeRepo(mkdtempSync(join(tmpdir(), "modbit-perf-repo-")));
  const pad = (n: number, w: number): string => String(n).padStart(w, "0");
  // Tokens are fixed width, so none is the prefix of another: `k1t007w03`.
  const tokenOf = (k: number, t: number, w: number): string => `k${k}t${pad(t, 3)}w${pad(w, 2)}`;
  const model = await pacedModel((goal, results) => {
    const k = Number(/^perf stream (\d+)/.exec(goal)?.[1] ?? "0");
    const parts: Part[] = [];
    for (let c = 0; c < CHUNKS; c++) {
      if (c > 0) parts.push({ pauseMs: GAP_MS });
      parts.push(`${[0, 1, 2, 3, 4].map((j) => tokenOf(k, results, c * 5 + j)).join(" ")} `);
    }
    if (results === 0) parts.push({ call: { name: "plan.update", args: { outcome: "read the notes", expected_files: [] } } });
    else if (results < TURNS) parts.push({ call: { name: "fs.read", args: { path: "notes.txt" } } });
    else parts.push({ call: { name: "task.complete", args: { summary: "done", self_review: { findings: [] } } } });
    return parts;
  });
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-perf-stream-"));
  const { app, page } = await launch(dataDir, { env: { MODBIT_OPENAI_BASE_URL: model.url } });
  try {
    await setContentSize(app, page, 1280, 800);
    await page.evaluate(() => {
      const w = window as unknown as { __stamps: Stamp[]; __long: { start: number; duration: number }[] };
      const epoch = (): number => performance.timeOrigin + performance.now();
      const stamps: Stamp[] = [];
      w.__stamps = stamps;
      // Long tasks (>50 ms of main-thread work): an IPC message that arrives during one waits for it.
      w.__long = [];
      new PerformanceObserver((list) => {
        for (const e of list.getEntries()) w.__long.push({ start: performance.timeOrigin + e.startTime, duration: e.duration });
      }).observe({ entryTypes: ["longtask"] });
      const check = (): void => {
        const root = document.querySelector('[data-testid="conversation"]');
        if (!root) return;
        const text = root.textContent ?? "";
        const now = epoch();
        for (const s of stamps) {
          if (s.mut !== 0 || !text.includes(s.token)) continue;
          s.mut = now;
          requestAnimationFrame(() => (s.frame = epoch()));
        }
      };
      new MutationObserver(check).observe(document.body, { childList: true, subtree: true, characterData: true });
      window.modbit.onEvent((raw) => {
        const e = raw as { eventType?: string; occurredAtMs?: number; receivedAtMs?: number; payload?: { kind?: string; sequence?: number; text?: string } };
        if (e.eventType !== "AssistantTextDelta" || e.payload?.kind !== "TEXT") return;
        const tokens = (e.payload.text ?? "").match(/k\d+t\d{3}w\d{2}/g);
        if (!tokens) return;
        stamps.push({ token: tokens[tokens.length - 1]!, first: e.payload.sequence === 1, seq: e.payload.sequence ?? 0, recorded: e.occurredAtMs ?? 0, main: e.receivedAtMs ?? 0, ipc: epoch(), mut: 0, frame: 0 });
        // The DOM may already show it (the event is also what drives the render): look again after this task.
        queueMicrotask(check);
      });
    });
    // The drafts are made on the Fleet first; each is then started and its conversation opened in turn (one stream is watched at a time).
    const ids: string[] = [];
    for (let k = 0; k < TASKS; k++) ids.push(await createTask(page, `perf stream ${k}`, repo));
    const sid = await sessionOf(page);
    for (const id of ids) {
      await page.evaluate(([s, t]) => window.modbit.startTask(s!, t!), [sid, id]);
      await openConversation(page, id);
      await expect(page.getByTestId("review")).toBeVisible({ timeout: 600_000 });
      await page.keyboard.press("Escape");
    }
    // Every stamp is rendered once its frame callback ran.
    await expect.poll(() => page.evaluate(() => (window as unknown as { __stamps: Stamp[] }).__stamps.filter((s) => s.frame === 0).length), { timeout: 30_000 }).toBe(0);
    const stamps = await page.evaluate(() => (window as unknown as { __stamps: Stamp[] }).__stamps);
    // What a read of the finished task's log costs the Core: the app makes these on every non-delta event, and each holds the store while it folds the log.
    const reads = await page.evaluate(
      async ([s, t]) => {
        const out: { transcript: number[]; headers: number[] } = { transcript: [], headers: [] };
        for (let i = 0; i < 20; i++) {
          let a = performance.now();
          await window.modbit.transcript(t!, {});
          out.transcript.push(performance.now() - a);
          a = performance.now();
          await window.modbit.agentHeaders(s!);
          out.headers.push(performance.now() - a);
        }
        return out;
      },
      [sid, ids[ids.length - 1]],
    );
    const long = await page.evaluate(() => (window as unknown as { __long: { start: number; duration: number }[] }).__long);
    // A delta is "held up" when it took over 100 ms; it is explained by a long task when one was running at some point between its event timestamp and its receipt.
    const slow = stamps.filter((s) => s.frame - s.recorded > 100);
    const explained = slow.filter((s) => long.some((l) => l.start < s.ipc && l.start + l.duration > s.recorded));
    const all = stamps.map((s) => s.frame - s.recorded);
    const first = stamps.filter((s) => s.first).map((s) => s.frame - s.recorded);
    const distributions: Record<string, Dist> = {
      "event timestamp to rendered frame, every delta": dist(all),
      "event timestamp to rendered frame, first delta of a message": dist(first),
      "event timestamp to main-process receipt (Core append, its connection, socket)": dist(stamps.map((s) => s.main - s.recorded)),
      "main-process receipt to renderer receipt (main thread, IPC)": dist(stamps.map((s) => s.ipc - s.main)),
      "event timestamp to rendered frame, 2nd delta of a message": dist(stamps.filter((s) => s.seq === 2).map((s) => s.frame - s.recorded)),
      "event timestamp to rendered frame, 3rd delta of a message": dist(stamps.filter((s) => s.seq === 3).map((s) => s.frame - s.recorded)),
      "event timestamp to rendered frame, 4th delta of a message": dist(stamps.filter((s) => s.seq === 4).map((s) => s.frame - s.recorded)),
      "diagnostic: one transcript read of the last task, IPC round trip": dist(reads.transcript),
      "diagnostic: one agent-headers read, IPC round trip": dist(reads.headers),
      "diagnostic: renderer long tasks (>50 ms main-thread work)": dist(long.map((l) => l.duration)),
      "renderer receipt to DOM showing it": dist(stamps.map((s) => s.mut - s.ipc)),
      "DOM showing it to the next frame": dist(stamps.map((s) => s.frame - s.mut)),
    };
    const met = distributions["event timestamp to rendered frame, every delta"]!.p95 <= 100 && distributions["event timestamp to rendered frame, first delta of a message"]!.p95 <= 100;
    retain(info, "px041-px047-stream-render", {
      budgets: [
        { statement: "QUAL-PX-041: event timestamp to render is at most 100 ms at p95", limitMs: 100, percentile: "p95" },
        { statement: "QUAL-PX-047: first-delta render is within 100 ms of the event timestamp at p95", limitMs: 100, percentile: "p95" },
      ],
      method: { tasks: TASKS, turnsPerTask: TURNS, deltasPerMessage: CHUNKS, gapMs: GAP_MS, model: "scripted OpenAI-compatible endpoint (stream pieces 60 ms apart)", profile: PROFILE, enforced: ENFORCE },
      distributions,
      diagnostics: { deltasOver100ms: slow.length, ofThoseDuringAnRendererLongTask: explained.length },
      verdict: { met, text: ENFORCE ? "asserted (reference profile)" : `recorded on the "${PROFILE}" profile; asserted only on the reference profile` },
    });
    expect(stamps.length, "every delta was seen by the renderer").toBeGreaterThanOrEqual(TASKS * TURNS);
    if (ENFORCE) {
      expect(distributions["event timestamp to rendered frame, every delta"]!.p95, "p95 of every delta, ms").toBeLessThanOrEqual(100);
      expect(distributions["event timestamp to rendered frame, first delta of a message"]!.p95, "p95 of first deltas, ms").toBeLessThanOrEqual(100);
    }
  } finally {
    await closeApp(app);
    model.server.close();
  }
});

interface ListStamp {
  taskId: string;
  kind: "appear" | "vanish";
  recorded: number;
  main: number;
  ipc: number;
  mut: number;
  frame: number;
}

test("PERF PX-046: the agent list reflects a Core event within 150 ms at p95 with a 200-task list", async ({}, info) => {
  test.setTimeout(900_000);
  const FIXTURE = num("MODBIT_PERF_FIXTURE", 200);
  const SAMPLES = num("MODBIT_PERF_SAMPLES", 220);
  const repo = makeRepo(mkdtempSync(join(tmpdir(), "modbit-perf-repo-")));
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-perf-list-"));
  const { app, page } = await launch(dataDir);
  try {
    await setContentSize(app, page, 1280, 800);
    // The first task is made on the Fleet, which opens the session; the rest of the fixture goes through the same preload command.
    await createTask(page, "fixture task 0", repo);
    const sid = await sessionOf(page);
    // The fixture: 200 tasks the list already holds.
    await page.evaluate(
      async ([s, r, n]) => {
        const fresh = (): string => Array.from({ length: 32 }, () => Math.floor(Math.random() * 16).toString(16)).join("");
        for (let i = 1; i < Number(n); i++) await window.modbit.createTask(s as string, `fixture task ${i}`, fresh(), r as string);
      },
      [sid, repo, FIXTURE],
    );
    await expect.poll(() => page.evaluate((s) => window.modbit.agentHeaders(s).then((v) => v.headers.length), sid), { timeout: 60_000 }).toBeGreaterThanOrEqual(FIXTURE);
    await expect(page.getByTestId("agent-row").first()).toBeVisible({ timeout: 30_000 });

    await page.evaluate(() => {
      const w = window as unknown as { __list: ListStamp[] };
      const epoch = (): number => performance.timeOrigin + performance.now();
      const stamps: ListStamp[] = [];
      w.__list = stamps;
      const present = (id: string): boolean => document.querySelector(`[data-testid="agent-row"][data-agent-id="${id}"]`) !== null;
      const check = (): void => {
        const now = epoch();
        for (const s of stamps) {
          if (s.mut !== 0) continue;
          if (s.kind === "appear" ? !present(s.taskId) : present(s.taskId)) continue;
          s.mut = now;
          requestAnimationFrame(() => (s.frame = epoch()));
        }
      };
      new MutationObserver(check).observe(document.body, { childList: true, subtree: true, attributes: true });
      window.modbit.onEvent((raw) => {
        const e = raw as { eventType?: string; taskId?: string | null; occurredAtMs?: number; receivedAtMs?: number };
        if (!e.taskId || (e.eventType !== "TaskCreated" && e.eventType !== "ConversationArchived")) return;
        stamps.push({ taskId: e.taskId, kind: e.eventType === "TaskCreated" ? "appear" : "vanish", recorded: e.occurredAtMs ?? 0, main: e.receivedAtMs ?? 0, ipc: epoch(), mut: 0, frame: 0 });
        queueMicrotask(check);
      });
    });
    const settled = (n: number) =>
      page.evaluate((count) => new Promise<boolean>((resolve) => {
        const w = window as unknown as { __list: ListStamp[] };
        const t0 = performance.now();
        const tick = (): void => {
          if (w.__list.length >= count && w.__list.every((s) => s.frame !== 0)) resolve(true);
          else if (performance.now() - t0 > 20_000) resolve(false);
          else setTimeout(tick, 5);
        };
        tick();
      }), n);
    // New tasks arrive one at a time; each is waited for before the next so the samples do not queue behind each other.
    const made: string[] = [];
    for (let i = 0; i < SAMPLES; i++) {
      const id = await page.evaluate(([s, r, k]) => window.modbit.createTask(s as string, `sample task ${k}`, Array.from({ length: 32 }, () => Math.floor(Math.random() * 16).toString(16)).join(""), r as string).then((t) => t.taskId), [sid, repo, i]);
      made.push(id);
      expect(await settled(i + 1), `the list never showed task ${i}`).toBe(true);
    }
    // And the other way: archiving removes the row from the default list.
    for (let i = 0; i < SAMPLES; i++) {
      await page.evaluate(([s, t]) => window.modbit.archiveTask(s!, t!, true), [sid, made[i]]);
      expect(await settled(SAMPLES + i + 1), `the list never dropped task ${i}`).toBe(true);
    }
    const stamps = await page.evaluate(() => (window as unknown as { __list: ListStamp[] }).__list);
    const of = (k: ListStamp["kind"]) => stamps.filter((s) => s.kind === k);
    const distributions: Record<string, Dist> = {
      "event timestamp to rendered frame, a task appears": dist(of("appear").map((s) => s.frame - s.recorded)),
      "event timestamp to rendered frame, a task is archived": dist(of("vanish").map((s) => s.frame - s.recorded)),
      "event timestamp to rendered frame, both": dist(stamps.map((s) => s.frame - s.recorded)),
      "event timestamp to main-process receipt (Core append, its connection, socket)": dist(stamps.map((s) => s.main - s.recorded)),
      "main-process receipt to renderer receipt (main thread, IPC)": dist(stamps.map((s) => s.ipc - s.main)),
      "renderer receipt to the row present or gone": dist(stamps.map((s) => s.mut - s.ipc)),
      "that to the next frame": dist(stamps.map((s) => s.frame - s.mut)),
    };
    const met = distributions["event timestamp to rendered frame, both"]!.p95 <= 150;
    retain(info, "px046-agent-list", {
      budgets: [{ statement: "QUAL-PX-046: the list reflects a Core event within 150 ms at p95 (200-task fixture)", limitMs: 150, percentile: "p95" }],
      method: { fixtureTasks: FIXTURE, samplesPerKind: SAMPLES, events: "TaskCreated (a row appears), ConversationArchived (a row goes)", profile: PROFILE, enforced: ENFORCE },
      distributions,
      verdict: { met, text: ENFORCE ? "asserted (reference profile)" : `recorded on the "${PROFILE}" profile; asserted only on the reference profile` },
    });
    expect(stamps.length).toBe(SAMPLES * 2);
    if (ENFORCE) expect(distributions["event timestamp to rendered frame, both"]!.p95, "p95, ms").toBeLessThanOrEqual(150);
  } finally {
    await closeApp(app);
  }
});

test("PERF PX-047: a transcript of 1,000 rows or more scrolls; the frame distribution is recorded (the dossier names no frame budget)", async ({}, info) => {
  test.setTimeout(900_000);
  const STEERS = num("MODBIT_PERF_STEERS", 800);
  const CALLS = 4; // fs.read calls per turn; the run ends at the Core's default of 60 turns, well under its 300 tool calls
  const repo = makeRepo(mkdtempSync(join(tmpdir(), "modbit-perf-repo-")));
  const held = new Gate();
  const model = await pacedModel((_goal, results) => {
    const turn = Math.floor(Math.max(0, results - 1) / CALLS);
    const parts: Part[] = results === 0 ? [{ wait: held.opened }, `Reading batch ${turn} of the notes.`] : [`Reading batch ${turn} of the notes.`];
    if (results === 0) parts.push({ call: { name: "plan.update", args: { outcome: "read the notes", expected_files: [] } } });
    else for (let i = 0; i < CALLS; i++) parts.push({ call: { name: "fs.read", args: { path: "notes.txt" } } });
    return parts;
  });
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-perf-scroll-"));
  const { app, page } = await launch(dataDir, { env: { MODBIT_OPENAI_BASE_URL: model.url } });
  try {
    await setContentSize(app, page, 1280, 800);
    // The task is made and started from the Fleet and the model is held at its first answer while the person's inputs queue behind it (the
    // conversation stays closed meanwhile, so the app is not re-reading it on every input). A queued input is a user row, and the run takes
    // one up per turn, so the run reads on until the Core's turn budget (60) ends it: no run under the defaults projects 1,000 rows on its
    // own (about 7 per turn), and the desktop cannot raise the budgets.
    const taskId = await createTask(page, "perf scroll", repo);
    const sid = await sessionOf(page);
    await page.evaluate(([s, t]) => window.modbit.startTask(s!, t!), [sid, taskId]);
    const t0 = Date.now();
    for (let i = 0; i < STEERS; i += 100) {
      await page.evaluate(
        async ([s, t, from, to]) => {
          for (let k = Number(from); k < Number(to); k++) await window.modbit.steerTask(s!, t!, `note ${k}`);
        },
        [sid, taskId, String(i), String(Math.min(STEERS, i + 100))],
      );
      console.log(`[perf] queued ${Math.min(STEERS, i + 100)} inputs after ${Date.now() - t0} ms`);
    }
    held.open();
    await expect.poll(() => page.evaluate((t) => window.modbit.taskStatus(t).then((s) => s.failureCode), taskId), { timeout: 600_000, intervals: [1000] }).toBe("BUDGET_EXHAUSTED");
    await openConversation(page, taskId);
    // Nothing folds in the detailed density: the rows the Core projected are the rows on the page.
    await page.getByTestId("conv-density").selectOption("DETAILED");
    await expect(page.getByTestId("conversation")).toHaveAttribute("data-density", "DETAILED");
    const total = await page.evaluate((t) => window.modbit.transcript(t, { density: "DETAILED", limit: 1 }).then((v) => v.totalRows), taskId);
    const scroll = page.getByTestId("conv-scroll");
    await expect.poll(() => page.locator('[data-testid="conversation"] [data-row-id]').count(), { timeout: 60_000 }).toBeGreaterThan(0);
    // Let the pages the view loads in the background arrive.
    await expect.poll(() => page.locator('[data-testid="conversation"] [data-row-id]').count(), { timeout: 60_000, intervals: [1000] }).toBeGreaterThanOrEqual(total - 5); // a few projected rows (a tail status) are not drawn as rows
    const rowsOnPage = await page.locator('[data-testid="conversation"] [data-row-id]').count();
    const frames = await scroll.evaluate(async (el) => {
      // Programmatic scrolling, one step per animation frame, top to bottom and back, twice. Each frame's interval is the time since the previous one.
      const out: number[] = [];
      const max = el.scrollHeight - el.clientHeight;
      const step = Math.max(8, Math.round(el.clientHeight / 30));
      const frame = (): Promise<number> => new Promise((r) => requestAnimationFrame(r));
      let last = await frame();
      for (let pass = 0; pass < 2; pass++) {
        for (const dir of [1, -1]) {
          let y = dir === 1 ? 0 : max;
          el.scrollTop = y;
          last = await frame();
          while (dir === 1 ? y < max : y > 0) {
            y = Math.min(max, Math.max(0, y + dir * step));
            el.scrollTop = y;
            const t = await frame();
            out.push(t - last);
            last = t;
          }
        }
      }
      return out;
    });
    const dropped = frames.filter((f) => f > 2 * (1000 / 60));
    const distributions: Record<string, Dist> = { "frame interval while scrolling (ms)": dist(frames) };
    retain(info, "px047-scroll-frames", {
      budgets: [{ statement: "QUAL-PX-047: 1,000 rows scroll without dropped frames beyond the budget. The dossier gives no frame budget (no count, no interval): this is an owner question.", limitMs: null, percentile: null }],
      method: { queuedInputs: STEERS, toolCallsPerTurn: CALLS, density: "DETAILED", rowsProjectedByTheCore: total, rowsOnThePage: rowsOnPage, framesMeasured: frames.length, droppedFrames: dropped.length, droppedFrameMeansIntervalOver: "33.3 ms (two 60 Hz frames)", profile: PROFILE },
      distributions,
      verdict: { met: null, text: `${dropped.length} of ${frames.length} frames over 33.3 ms (${((100 * dropped.length) / Math.max(1, frames.length)).toFixed(1)} %); no numeric budget exists to judge them against` },
    });
    expect(total, "the Core projected at least 1,000 rows").toBeGreaterThanOrEqual(1000);
    expect(rowsOnPage, "the page holds the rows").toBeGreaterThanOrEqual(1000 - 5);
    expect(frames.length, "a measurable scroll").toBeGreaterThan(100);
  } finally {
    await closeApp(app);
    model.server.close();
  }
});
