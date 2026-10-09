import { defineConfig } from "@playwright/test";

// A terminal broker left by a Core that a test killed waits out its orphan grace (60 s by default) before it exits. The
// suite's Cores are killed on purpose, so the grace is short here; every worker inherits the variable, and the
// restart specs reattach well within it.
process.env.MODBIT_EXECD_ORPHAN_GRACE_SECS ??= "5";

export default defineConfig({
  testDir: ".",
  timeout: 120_000,
  retries: 0,
  workers: 1,
  reporter: [["list"], ["json", { outputFile: "../../target/desktop-e2e-report.json" }]],
  use: { trace: "retain-on-failure" },
});
