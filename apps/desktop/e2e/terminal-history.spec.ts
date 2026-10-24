/**
 * REQ-PX-048 / PX-043 on the real stack: a build that streams live into the
 * Terminal tab, and older output read as OutputRef ranges. The packaged
 * Electron app, a real Core, a real `modbit-execd` with a 1 MiB replay window
 * (so a 3 MiB build drops its head), real PTYs; the scripted model holds the
 * task Running at a gate while the specs use the panel.
 *
 * The "build" is a Node program on a PTY that prints numbered lines (paced for
 * the live proof, in bulk for the history proof): the lines are numbered so a
 * gap, a repeat or a reordering is visible in the bytes, whatever the platform.
 * Nothing waits for a fixed time: every wait is for a condition the Core or the
 * page reports.
 */
import { expect, test, type ElectronApplication, type Page } from "@playwright/test";
import type { Server } from "node:http";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { ECHO_LOOP, expectTerminals, gate, nodeTerminal, readTerminal, steppedModel, type Step } from "./support/apps-harness.ts";
import { accessible, closeApp, launch, makeRepo, MOD, setContentSize } from "./support/ui-harness.ts";

test.describe.configure({ mode: "serial" });

/** One numbered line: 7 digits, a space, 120 x, a line feed (the PTY adds the carriage return). */
const WIDTH = Number(process.env.TERM_HISTORY_WIDTH ?? 120);
const NUMBERED = `String(k).padStart(7,'0')+' '+'x'.repeat(${WIDTH})+'\\n'`;
const lineOf = (k: number) => `${String(k).padStart(7, "0")} ${"x".repeat(WIDTH)}`;
/** Prints `n` numbered lines in bulk, then exits (or, with `stay`, keeps running). */
const flood = (tag: string, n: number, stay: boolean) =>
  `const tag='${tag}'; const N=${n}; let i=0; const go=()=>{ let s=''; for(let c=0;c<500&&i<N;c++,i++){ const k=i; s+=${NUMBERED}; } process.stdout.write(s,()=>{ if(i<N) setImmediate(go); else ${stay ? "setInterval(()=>{},1000)" : "process.exit(0)"}; }); }; go();`;
const ENDED_LINES = 24_000; // about 3.1 MB through a 1 MiB replay window
const RUNNING_LINES = 16_000; // about 2.1 MB
/** A build that reports progress for about 24 s, then ends with code 0. */
const PACED = "const tag='MARK_PACED'; let i=0; const t=setInterval(()=>{ i++; console.log('step '+i+' of 60'); if(i===60){ clearInterval(t); console.log('build done'); process.exit(0); } },400);";

let app: ElectronApplication;
let page: Page;
let server: Server;
let sessionId = "";
let taskId = "";
const modelGate = gate();
const handle: { paced: string; ended: string; running: string; echo: string } = { paced: "", ended: "", running: "", echo: "" };

const terminalView = () => page.getByTestId("terminal-view");
const screen = () => page.getByTestId("terminal-screen");
const history = () => page.getByTestId("terminal-history");
const listTerminals = () => page.evaluate((t) => window.modbit.terminalList(t).then((l) => l.terminals), taskId);
const viewOf = async (id: string) => (await listTerminals()).find((t) => t.terminalId === id)!;
const pickTerminal = async (id: string) => {
  await page.locator(`[data-testid="terminal-row"][data-terminal-id="${id}"] [data-testid="terminal-pick"]`).click();
  await expect(terminalView()).toHaveAttribute("data-terminal-id", id);
};

test.beforeAll(async () => {
  const repo = makeRepo(mkdtempSync(join(tmpdir(), "modbit-term-history-repo-")));
  const script: Step[] = [
    { calls: [{ name: "plan.update", args: { outcome: "run builds", expected_files: ["notes.txt"] } }] },
    { calls: [nodeTerminal(PACED), nodeTerminal(flood("MARK_ENDED", ENDED_LINES, false)), nodeTerminal(flood("MARK_RUNNING", RUNNING_LINES, true)), nodeTerminal(ECHO_LOOP)] },
    async () => {
      await modelGate.wait;
      return { calls: [{ name: "task.complete", args: { summary: "builds ran", self_review: { findings: [] } } }] };
    },
  ];
  const m = await steppedModel(script);
  server = m.server;
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-term-history-"));
  ({ app, page } = await launch(dataDir, { env: { MODBIT_OPENAI_BASE_URL: m.url, MODBIT_EXECD_REPLAY_WINDOW_BYTES: "1048576", MODBIT_EXECD_SEGMENT_BYTES: "65536" } }));
  await setContentSize(app, page, 1280, 800);
  await page.getByTestId("goal").fill("Run the builds");
  await page.getByTestId("workspace").fill(repo);
  await page.getByTestId("run").click();
  const card = page.getByTestId("task-card").first();
  taskId = (await card.getAttribute("data-task-id"))!;
  sessionId = (await page.evaluate(() => window.modbit.localState()).then((s) => s.sessionId))!;
  await card.getByTestId("task-start").click();
  await expect(card.getByTestId("task-state")).toHaveText("Running", { timeout: 60_000 });
  await expectTerminals(page, taskId, 4);
  const all = await listTerminals();
  const by = (needle: string) => all.find((t) => t.title.includes(needle))!.terminalId;
  handle.paced = by("MARK_PACED");
  handle.ended = by("MARK_ENDED");
  handle.running = by("MARK_RUNNING");
  handle.echo = by("READY");
  await card.focus();
  await page.keyboard.press(`${MOD}+e`);
  await expect(page.getByTestId("region-apps")).toBeVisible();
  await page.getByTestId("tab-terminal").click();
});

test.afterAll(async () => {
  modelGate.open();
  if (app) await closeApp(app);
  server?.close();
});

test("a build streams live into the Terminal tab: output arrives while the process runs, on one attachment, and the exit is shown when it ends", async () => {
  await pickTerminal(handle.paced);
  await expect(screen()).toContainText("step 3 of 60", { timeout: 30_000 });
  expect((await viewOf(handle.paced)).state, "the build is still running while its output is on screen").toBe("RUNNING");
  await expect(screen()).not.toContainText("build done");
  const first = BigInt((await terminalView().getAttribute("data-cursor"))!);
  await expect(screen()).toContainText("step 9 of 60", { timeout: 30_000 });
  const second = BigInt((await terminalView().getAttribute("data-cursor"))!);
  expect(second, "the stream moved on without a reload or a reattach").toBeGreaterThan(first);
  await expect(terminalView()).toHaveAttribute("data-phase", "live");
  expect((await viewOf(handle.paced)).state, "still running at step 9").toBe("RUNNING");
  expect(await terminalView().getAttribute("data-lease"), "watching a build takes no input lease").toBe("free");
  // The end of the build reaches the same view as a frame: the Core's exit, with its code.
  await expect(screen()).toContainText("build done", { timeout: 60_000 });
  await expect(page.getByTestId("terminal-exit")).toContainText("Exited with code 0", { timeout: 30_000 });
  await expect(terminalView()).toHaveAttribute("data-phase", "exited");
  const done = await viewOf(handle.paced);
  expect(done.state).toBe("EXITED");
  expect(done.exitCode).toBe(0);
  expect(await terminalView().getAttribute("data-cursor"), "the view stands at the Core's head").toBe(done.bytesSoFar);
});

test("scrolling above the view reads the older output as OutputRef ranges: in order, once, from the Core's retained start to the view", async () => {
  test.setTimeout(240_000);
  const row = (id: string) => page.locator(`[data-terminal-id="${id}"] [data-testid="terminal-state"]`);
  await expect(row(handle.ended)).toContainText("exited 0", { timeout: 150_000 });
  const ended = await viewOf(handle.ended);
  expect(ended.hasOutputRef, "the Core sealed the output when the process ended").toBe(true);
  const retainedFrom = BigInt(ended.oldestCursor);
  const head = BigInt(ended.bytesSoFar);
  expect(retainedFrom, "the 1 MiB replay window dropped the head of a 3 MB build").toBeGreaterThan(0n);
  expect(head - retainedFrom, "what the Core kept is larger than the view's tail").toBeGreaterThan(262_144n * 2n);

  await pickTerminal(handle.ended);
  // The view replays the tail of the ended build, not all of it.
  const origin = head - 262_144n;
  await expect(terminalView()).toHaveAttribute("data-cursor", head.toString(), { timeout: 60_000 });
  await expect(screen()).toContainText(String(ENDED_LINES - 1).padStart(7, "0"));
  await expect(history()).toHaveAttribute("data-origin", origin.toString());
  await expect(history(), "nothing older is read until the person scrolls there").toHaveAttribute("data-pages", "0");
  await expect(page.getByTestId("terminal-history-pane")).toHaveCount(0);

  // Scroll up in the terminal itself, past its first line: the first range arrives.
  await screen().hover();
  await expect
    .poll(async () => {
      await page.mouse.wheel(0, -100_000);
      return Number(await history().getAttribute("data-pages"));
    }, { timeout: 60_000, intervals: [100] })
    .toBeGreaterThanOrEqual(1);
  await expect(history()).toHaveAttribute("data-loading", "false");
  // A burst of wheel events can pass the top more than once: the ranges read are whole pages, each below the last.
  const snap = await history().evaluate((el) => ({ pages: Number(el.getAttribute("data-pages")), bytes: Number(el.getAttribute("data-bytes")), from: el.getAttribute("data-loaded-from") }));
  expect(snap.bytes, "ranges are whole pages").toBe(snap.pages * 65_536);
  expect(snap.from).toBe((origin - BigInt(snap.bytes)).toString());
  await accessible(page, "the earlier-output pane");

  // Scroll up through the pane: each time its top is passed, the range before it is read. Record every state seen.
  const pane = page.getByTestId("terminal-history-pane");
  await pane.hover();
  const seen: { pages: number; from: bigint }[] = [];
  await expect
    .poll(async () => {
      await page.mouse.wheel(0, -30_000);
      // One read of the attributes, so the pair belongs to one state of the pane.
      const now = await history().evaluate((el) => ({ pages: Number(el.getAttribute("data-pages")), from: el.getAttribute("data-loaded-from")!, exhausted: el.getAttribute("data-exhausted") }));
      if (seen.length === 0 || seen[seen.length - 1]!.pages !== now.pages) seen.push({ pages: now.pages, from: BigInt(now.from) });
      return now.exhausted;
    }, { timeout: 150_000, intervals: [100] })
    .toBe("true");
  await expect(history()).toHaveAttribute("data-loading", "false");

  // Each range was taken once, one after another, each ending where the loaded text began.
  for (let i = 1; i < seen.length; i++) {
    // The poll can miss a state between two samples, so a step is one or more ranges, never a range of more than a page.
    const added = seen[i]!.pages - seen[i - 1]!.pages;
    expect(added).toBeGreaterThan(0);
    expect(seen[i]!.from, "each range lies below the previous one").toBeLessThan(seen[i - 1]!.from);
    expect(seen[i - 1]!.from - seen[i]!.from <= BigInt(added) * 65_536n, "and is at most a page").toBe(true);
  }
  // Everything the Core retained before the view is in, and nothing else.
  const bytes = BigInt((await history().getAttribute("data-bytes"))!);
  expect(await history().getAttribute("data-loaded-from")).toBe(retainedFrom.toString());
  expect(bytes, "the pane holds exactly the retained bytes before the view").toBe(origin - retainedFrom);
  expect(Number(await history().getAttribute("data-pages"))).toBe(Math.ceil(Number(bytes) / 65_536));
  await expect(page.getByTestId("terminal-history-load")).toBeDisabled();
  await expect(page.getByTestId("terminal-history-status")).toContainText("all the output the Core retained");

  // The bytes: the pane's text followed by the Core's own replay from where the view begins is the build's output, line after line.
  const paneText = (await page.getByTestId("terminal-history-text").textContent()) ?? "";
  // The view's own bytes, from the sealed output too (the same object the pane was read from), so the junction is exact.
  const listed = await viewOf(handle.ended);
  const tail = await page.evaluate(({ taskId, id, from }) => window.modbit.terminalHistory(taskId, id, from, String(BigInt(from) + 262_144n)).then((r) => ({ from: r.from, startCursor: r.startCursor, bytes: Array.from(r.data) })), { taskId, id: handle.ended, from: origin.toString() });
  const replay = await readTerminal(page, { sessionId, taskId, terminalId: handle.ended, after: origin.toString(), windowBytes: 65_536 });
  const head16 = (u: number[]) => JSON.stringify(u.slice(0, 16));
  const cursors = `listing: oldestCursor=${listed.oldestCursor} bytesSoFar=${listed.bytesSoFar}; view origin=${origin}; object range starts at cursor ${tail.from} (retained from ${tail.startCursor}) and holds ${tail.bytes.length} bytes from there; stream from ${origin} holds ${replay.bytes.length} bytes (head ${replay.head}); object first bytes ${head16(tail.bytes)}, stream first bytes ${head16(replay.bytes)}`;
  expect(tail.from, `the object range starts at the view's cursor. ${cursors}`).toBe(origin.toString());
  // The Core's live stream from the same cursor is the same bytes as the sealed output, compared by cursor, not by the end.
  const common = Math.min(tail.bytes.length, replay.bytes.length);
  const firstDiff = replay.bytes.slice(0, common).findIndex((b, i) => b !== tail.bytes[i]);
  expect(firstDiff, `the stream and the sealed output differ at view byte ${firstDiff} (cursor ${origin + BigInt(firstDiff)}). ${cursors}`).toBe(-1);
  expect(tail.bytes.length, `the stream and the sealed output are the same length. ${cursors}`).toBe(replay.bytes.length);
  // A PTY's line end is CR LF; a doubled CR is seen at write boundaries on some platforms. Both are one line end, as in the pane.
  const norm = (u: number[]) => new TextDecoder().decode(Uint8Array.from(u)).replace(/\u001b\[[0-?]*[ -/]*[@-~]/g, "").replace(/\r+\n/g, "\n").replace(/\r/g, "\n");
  const whole = paneText + norm(tail.bytes);
  const lines = whole.split("\n");
  const last = lines.pop()!;
  expect(JSON.stringify(last), `the build ended with a line feed; what follows it is ${JSON.stringify(last)} (codes ${JSON.stringify(Array.from(last).map((ch) => ch.charCodeAt(0)))}). ${cursors}`).toBe('""');
  // On every platform: the pane followed by the view is exactly what the Core's live stream carries from the oldest retained cursor
  // to the head (a different path from the sealed object the pane was read from), whatever the platform's PTY does to the lines.
  const everything = await readTerminal(page, { sessionId, taskId, terminalId: handle.ended, after: retainedFrom.toString(), windowBytes: 65_536 });
  const streamText = norm(everything.bytes);
  const joined = lines.join("\n") + "\n";
  const at = joined.length === streamText.length && joined === streamText ? -1 : Array.from(joined).findIndex((ch, i) => ch !== streamText[i]);
  expect(at, `the pane and the view differ from the Core's stream at text offset ${at} (pane+view ${joined.length} chars, stream ${streamText.length}): pane+view ${JSON.stringify(joined.slice(Math.max(0, at - 20), at + 40))}, stream ${JSON.stringify(streamText.slice(Math.max(0, at - 20), at + 40))}`).toBe(-1);
  if (process.platform === "win32") {
    // A Windows pseudo-console re-renders lines (wrapping, cursor sequences): the numbers are what stays. Each line number appears once, in order, up to the last.
    const numbers = [...joined.matchAll(/(?<![0-9])(\d{7}) x/g)].map((m) => Number(m[1]));
    expect(numbers.length, `line numbers found in ${JSON.stringify(joined.slice(0, 300))}`).toBeGreaterThan(1000);
    const gap = numbers.findIndex((n, i) => i > 0 && n !== numbers[i - 1]! + 1);
    expect(gap, `line numbers run on without a gap or a repeat; around ${gap}: ${JSON.stringify(numbers.slice(Math.max(0, gap - 2), gap + 3))}`).toBe(-1);
    expect(numbers.at(-1)).toBe(ENDED_LINES - 1);
    return;
  }
  const firstFull = Number(/^(\d{7}) /.exec(lines[1]!)![1]);
  // The Core's retained start can fall inside a line: the first line is then the tail of the line before it.
  expect(lineOf(firstFull - 1).endsWith(lines[0]!), `first line ${JSON.stringify(lines[0])} is the tail of line ${firstFull - 1}`).toBe(true);
  let bad = -1;
  for (let i = 1; i < lines.length; i++) {
    if (lines[i] !== lineOf(firstFull + i - 1)) {
      bad = i;
      break;
    }
  }
  expect(bad, bad < 0 ? "" : `line ${bad} is ${JSON.stringify(lines[bad])}, expected ${lineOf(firstFull + bad - 1)}`).toBe(-1);
  expect(firstFull + lines.length - 2, "and it ends at the last line the build printed").toBe(ENDED_LINES - 1);
});

test("a build that is still running has no sealed output: the Core refuses a history read, and the view offers none", async () => {
  const running = handle.running;
  await expect.poll(async () => BigInt((await viewOf(running)).oldestCursor), { timeout: 150_000 }).toBeGreaterThan(0n);
  const v = await viewOf(running);
  expect(v.state).toBe("RUNNING");
  expect(v.hasOutputRef).toBe(false);
  await pickTerminal(running);
  await expect(terminalView()).toHaveAttribute("data-phase", "live");
  await expect(history()).toHaveCount(0);
  const refusals = await page.evaluate(
    async ({ taskId, running, ended }) => {
      const ask = (t: string, id: string) => window.modbit.terminalHistory(t, id, "0", "65536").then(() => "ALLOWED", (e: Error) => e.message);
      return [await ask(taskId, running), await ask("f".repeat(32), ended), await ask(taskId, "0123456789abcdef")];
    },
    { taskId, running, ended: handle.ended },
  );
  expect(refusals[0]).toMatch(/NO_OUTPUT_REF/);
  expect(refusals[1], "a task that does not own the terminal reads nothing").toMatch(/UNKNOWN_TERMINAL/);
  expect(refusals[2]).toMatch(/UNKNOWN_TERMINAL/);
  // Malformed ranges never leave main.
  const bad = await page.evaluate(async ({ taskId, ended }) => {
    const ask = (...a: [string, string, string, string]) => window.modbit.terminalHistory(...a).then(() => "ALLOWED", (e: Error) => e.message);
    return [await ask("short", ended, "0", "1"), await ask(taskId, "../x", "0", "1"), await ask(taskId, ended, "-1", "1"), await ask(taskId, ended, "0", "x")];
  }, { taskId, ended: handle.ended });
  for (const m of bad) expect(m).toMatch(/BAD_ARGUMENT/);
  // A range past the head, or wholly before what the Core kept, is empty rather than an error or someone else's bytes.
  const ranges = await page.evaluate(async ({ taskId, ended }) => {
    const before = await window.modbit.terminalHistory(taskId, ended, "0", "4096");
    return { before: before.data.length, exhausted: before.exhausted };
  }, { taskId, ended: handle.ended });
  expect(ranges).toEqual({ before: 0, exhausted: true });
});

test("input policy: a finished terminal takes no keystroke; a write without the input lease is refused by the Core and reaches nothing", async () => {
  // A finished build: the view types nothing and holds no lease.
  await pickTerminal(handle.ended);
  const endedBefore = await viewOf(handle.ended);
  await screen().click();
  await page.keyboard.type("zzz");
  await page.keyboard.press("Enter");
  const endedAfter = await viewOf(handle.ended);
  expect(endedAfter.inputLeaseHolder).toBe("");
  expect(endedAfter.bytesSoFar, "nothing was written to a process that ended").toBe(endedBefore.bytesSoFar);
  expect(await terminalView().getAttribute("data-lease")).toBe("free");

  // A running terminal, written to without the lease (no keystroke from the view took it): the Core refuses and the process never sees it.
  const echo = handle.echo;
  const refusal = await page.evaluate(
    async ({ sessionId, taskId, echo }) => {
      const att = await window.modbit.terminalAttach(sessionId, taskId, echo, "0", { windowBytes: 4096 });
      try {
        await window.modbit.terminalWrite(att.attachId, "unleased\n");
        return "ALLOWED";
      } catch (e) {
        return (e as Error).message;
      } finally {
        await window.modbit.terminalDetach(att.attachId).catch(() => undefined);
      }
    },
    { sessionId, taskId, echo },
  );
  expect(refusal).toMatch(/LEASE_REQUIRED/);
  const log = await readTerminal(page, { sessionId, taskId, terminalId: echo, after: "0", windowBytes: 65_536 });
  expect(new TextDecoder().decode(Uint8Array.from(log.bytes))).not.toContain("unleased");
  expect((await viewOf(echo)).inputLeaseHolder).toBe("");

  // With the lease, taken by typing in the view, the same terminal accepts input and the process answers.
  await pickTerminal(echo);
  await screen().click();
  await page.keyboard.type("with-lease");
  await page.keyboard.press("Enter");
  await expect(screen()).toContainText("got:with-lease", { timeout: 30_000 });
  await expect.poll(async () => (await viewOf(echo)).inputLeaseHolder).toMatch(/^user:/);
  await page.getByTestId("terminal-leave").focus();
  await expect.poll(async () => (await viewOf(echo)).inputLeaseHolder).toBe("");
});
