import { defineConfig } from "@playwright/test";

// The packaged-build suites (M10.2): they need `pnpm package:dir` first and are
// kept apart from the checkout E2E so that run does not depend on packaging.
export default defineConfig({
  testDir: ".",
  testMatch: /packaged.*\.spec\.ts/,
  timeout: 180_000,
  retries: 0,
  workers: 1,
  reporter: [["list"], ["json", { outputFile: "../../target/desktop-packaged-report.json" }]],
  use: { trace: "retain-on-failure" },
});
