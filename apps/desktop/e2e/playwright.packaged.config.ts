import { defineConfig } from "@playwright/test";

// The packaged-build suites (M10.2): they need `pnpm package:dir` (or `package`) first and are kept apart from the
// checkout E2E so that run does not depend on packaging. The package under test is UNSIGNED (no code-signing
// identity, no notarization; both are the owner's), so this leg proves the package runs, not that it is signed.
//
// Besides the packaged-only specs, a representative subset of the checkout suite runs against the packaged executable:
// ui-harness `launch` reads MODBIT_E2E_PACKAGED and starts the package's own executable with no MODBIT_CORE_BIN, so the
// Core that connects is the one the package carries. The subset covers the shell and its accessibility pass, a scripted
// task run end to end, approvals across a Core restart, supervision (Core kill and recovery) and the render budgets.
process.env.MODBIT_E2E_PACKAGED = "1";
process.env.MODBIT_EXECD_ORPHAN_GRACE_SECS ??= "5";

export const PACKAGED_SUBSET = ["conversation", "approvals", "supervision", "perf-budgets"];

export default defineConfig({
  testDir: ".",
  testMatch: [/packaged.*\.spec\.ts/, ...PACKAGED_SUBSET.map((n) => new RegExp(`(^|[\\\\/])${n}\\.spec\\.ts$`))],
  timeout: 180_000,
  expect: { timeout: 15_000 },
  retries: 0,
  workers: 1,
  reporter: [["list"], ["json", { outputFile: "../../target/desktop-packaged-report.json" }]],
  use: { trace: "retain-on-failure" },
});
