import { defineConfig } from "@playwright/test";

// A terminal broker left by a Core that a test killed waits out its orphan grace (60 s by default) before it exits. The
// suite's Cores are killed on purpose, so the grace is short here; every worker inherits the variable, and the
// restart specs reattach well within it.
process.env.MODBIT_EXECD_ORPHAN_GRACE_SECS ??= "5";
// REQ-PX-210: the composer asks the Core for a suggested next message when a turn ends, which is one more model request per
// turn. The suite's scripted models answer by request order or count, so the Core's own switch turns the call off for
// every spec (the Core then refuses the request and calls no model); px-210-suggestion.spec.ts turns it back on.
process.env.MODBIT_NEXT_PROMPT_SUGGESTIONS ??= "off";

export default defineConfig({
  testDir: ".",
  testIgnore: /packaged.*\.spec\.ts/,
  timeout: 120_000,
  // An assertion without its own timeout waited Playwright's 5 s default, which the Windows and macOS CI runners
  // regularly overran (a task card appearing, a mode acknowledgement): the Core had answered, the wait had not allowed it.
  // Waiting longer changes nothing about what is asserted; an assertion that needs less still passes at once.
  expect: { timeout: 15_000 },
  retries: 0,
  workers: 1,
  reporter: [["list"], ["json", { outputFile: "../../target/desktop-e2e-report.json" }]],
  use: { trace: "retain-on-failure" },
});
