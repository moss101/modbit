import assert from "node:assert/strict";
import { test } from "node:test";
import { noWork, quitChoice, quitPrompt, recoverySummary, runShutdown, SESSION_END_DEADLINE_MS, sessionEndFailure, WINDOW_DEFAULT, WINDOW_MIN } from "./lifecycle.ts";

test("AFW-A02: the default window is 1280 x 800 and the minimum 900 x 600", () => {
  assert.deepEqual(WINDOW_DEFAULT, { width: 1280, height: 800 });
  assert.deepEqual(WINDOW_MIN, { width: 900, height: 600 });
});

test("PX-049: no running work means no question", () => {
  assert.equal(noWork({ runningTasks: 0, runningTerminals: 0 }), true);
  assert.equal(noWork({ runningTasks: 1, runningTerminals: 0 }), false);
  assert.equal(noWork({ runningTasks: 0, runningTerminals: 2 }), false);
});

test("PX-049: the quit question says what is running and what quitting really does", () => {
  const p = quitPrompt({ runningTasks: 2, runningTerminals: 1 });
  assert.match(p.message, /2 tasks are running and 1 background terminal is running/);
  assert.match(p.detail, /stops the local Core/);
  assert.match(p.detail, /suspended at their last saved step and recovered the next time you open Modbit/);
  assert.match(p.detail, /nothing continues while Modbit is closed/i);
  assert.match(p.detail, /about a minute/);
  assert.match(p.detail, /resume by themselves when you reopen it, or wait for you/, "the resume choice is offered");
  assert.equal(p.buttons[0], "Keep Modbit open", "the default answer keeps the app open");
  assert.deepEqual(p.buttons.slice(1), ["Quit and resume on reopen", "Quit and leave suspended"]);
  assert.doesNotMatch(p.detail, /keeps running in the background|continue running/i, "it must not claim tasks outlive the app");
  const one = quitPrompt({ runningTasks: 1, runningTerminals: 0 });
  assert.match(one.message, /^1 task is running\./);
  assert.doesNotMatch(one.detail, /terminal/);
});

test("PX-049: the recovery counts come from the Core's own states", () => {
  assert.equal(recoverySummary({ recovered: 0, resumed: 0, needAttention: 0 }), "No tasks to recover.");
  assert.equal(recoverySummary({ recovered: 4, resumed: 3, needAttention: 1 }), "3 tasks resumed, 1 needs attention.");
  assert.equal(recoverySummary({ recovered: 3, resumed: 1, needAttention: 2 }), "1 task resumed, 2 need attention.");
  assert.equal(recoverySummary({ recovered: 5, resumed: 1, needAttention: 2 }), "1 task resumed, 2 need attention, 2 recovered idle.");
  assert.equal(recoverySummary({ recovered: 3, resumed: 0, needAttention: 0 }), "3 recovered idle.");
});

test("PX-049: shutdown reports what failed or hung and never waits past its deadline", async () => {
  const order: string[] = [];
  const t0 = Date.now();
  const report = await runShutdown(
    [
      { name: "ok", run: () => void order.push("ok") },
      { name: "boom", run: () => Promise.reject(new Error("hook exited 1")) },
      { name: "hung", run: () => new Promise<void>(() => undefined) },
      { name: "slow-but-fine", run: () => new Promise<void>((r) => setTimeout(r, 20)) },
    ],
    200,
  );
  assert.ok(Date.now() - t0 < 1500, "returned at the deadline, not when the hung step ends");
  assert.deepEqual(report.completed.sort(), ["ok", "slow-but-fine"]);
  assert.deepEqual(report.failed, [{ name: "boom", reason: "hook exited 1" }]);
  assert.deepEqual(report.timedOut, ["hung"]);
});

test("PX-049: a shutdown with nothing to run is clean and immediate", async () => {
  assert.deepEqual(await runShutdown([], 1000), { completed: [], failed: [], timedOut: [] });
});

test("PX-049: the answer to the quit question is a choice: stay, quit, or quit and resume on reopen", () => {
  const tasks = { runningTasks: 2, runningTerminals: 0 };
  assert.equal(quitChoice(tasks, 0), "stay", "the first button keeps the app open");
  assert.equal(quitChoice(tasks, 1), "quit-resume");
  assert.equal(quitChoice(tasks, 2), "quit");
  assert.equal(quitChoice(tasks, -1), "stay", "a dismissed dialog never quits");
  assert.equal(quitChoice(tasks, 7), "stay", "an answer that is no button never quits");
  // Terminals alone have no run to resume: the only way out is a plain quit.
  const terminals = { runningTasks: 0, runningTerminals: 1 };
  assert.deepEqual(quitPrompt(terminals).buttons, ["Keep Modbit open", "Quit and stop"]);
  assert.equal(quitChoice(terminals, 1), "quit");
  assert.equal(quitChoice(terminals, 2), "stay");
});

test("PX-049: the session-end deadline leaves room for a hook's own longest timeout (30 s)", () => {
  assert.ok(SESSION_END_DEADLINE_MS > 30_000);
});

test("PX-049: a session-end hook that did not end well is a visible failure naming it; clean runs are not", () => {
  assert.equal(sessionEndFailure([]), null);
  assert.equal(sessionEndFailure([{ hook: "config:admin/archive", outcome: "OK", detail: "archived 3 files" }]), null);
  const text = sessionEndFailure([
    { hook: "config:admin/archive", outcome: "OK", detail: "" },
    { hook: "config:admin/upload", outcome: "FAILED", detail: "exit 1" },
    { hook: "config:admin/slow", outcome: "TIMEOUT", detail: "no answer within 500 ms" },
  ]);
  assert.equal(text, "2 session-end hooks did not finish cleanly: config:admin/upload: failed (exit 1); config:admin/slow: timeout (no answer within 500 ms)");
  assert.match(sessionEndFailure([{ hook: "h", outcome: "MALFORMED", detail: "" }])!, /^1 session-end hook did not finish cleanly/);
});

test("PX-049: a session-end step that outlives the deadline is reported as timed out, a failing one as failed", async () => {
  const hung = await runShutdown([{ name: "session-end", run: () => new Promise<void>(() => undefined) }], 100);
  assert.deepEqual(hung.timedOut, ["session-end"]);
  const failing = await runShutdown([{ name: "session-end", run: () => Promise.reject(new Error("1 session-end hook did not finish cleanly: h: failed")) }], 1000);
  assert.deepEqual(failing.failed, [{ name: "session-end", reason: "1 session-end hook did not finish cleanly: h: failed" }]);
});
