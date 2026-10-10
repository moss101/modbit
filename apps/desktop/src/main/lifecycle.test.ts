import assert from "node:assert/strict";
import { test } from "node:test";
import { noWork, quitPrompt, recoverySummary, runShutdown, WINDOW_DEFAULT, WINDOW_MIN } from "./lifecycle.ts";

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
  assert.equal(p.buttons[0], "Keep Modbit open", "the default answer keeps the app open");
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
