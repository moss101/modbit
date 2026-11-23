/**
 * `closeApp` records what each window showed when a test let go of the app, every time, because a test cannot know in its own
 * `finally` that it failed. This keeps that record only where it is evidence: for a test that ended as expected, the files are
 * deleted, so the traces a failed CI job uploads hold the failing tests' pages and nothing else.
 */
import { rmSync } from "node:fs";
import type { Reporter, TestCase, TestResult } from "@playwright/test/reporter";
import { AT_CLOSE } from "./close-app.ts";

export default class AtCloseReporter implements Reporter {
  onTestEnd(test: TestCase, result: TestResult): void {
    if (result.status !== test.expectedStatus) return;
    for (const a of result.attachments) if (a.name === AT_CLOSE && a.path) rmSync(a.path, { force: true });
  }

  printsToStdio(): boolean {
    return false;
  }
}
