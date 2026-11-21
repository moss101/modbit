/**
 * Closing a test's Electron app, for every spec. A plain kill of the main
 * process is not enough: on Windows (and on Linux for the GPU, utility and
 * renderer helpers) Chromium's child processes outlive a killed parent and
 * keep the test worker's stdout/stderr pipes open, so the worker's teardown
 * waits for them (120 s, then "1 error was not a part of any test"). This
 * asks the app to quit, waits a bounded grace, then kills the whole process
 * tree (taskkill /T on Windows; the descendants on POSIX) and waits, bounded,
 * for the main process to be gone.
 */
import { execFileSync } from "node:child_process";
import type { ChildProcess } from "node:child_process";
import { writeFileSync } from "node:fs";
import { test } from "@playwright/test";
import type { ElectronApplication } from "@playwright/test";

const GRACE_MS = 15_000;
const REAP_MS = 5_000;

function children(pid: number): number[] {
  // POSIX: the direct children of pid (recursively, below).
  try {
    const out = execFileSync("ps", ["-A", "-o", "pid=,ppid="], { encoding: "utf8" });
    return out
      .split("\n")
      .map((l) => l.trim().split(/\s+/).map(Number))
      .filter(([, ppid]) => ppid === pid)
      .map(([p]) => p!);
  } catch {
    return [];
  }
}

function killTree(pid: number): void {
  if (process.platform === "win32") {
    try {
      execFileSync("taskkill", ["/PID", String(pid), "/T", "/F"], { stdio: "ignore" });
    } catch {
      // already gone
    }
    return;
  }
  const seen = new Set<number>();
  const walk = (p: number) => {
    if (seen.has(p)) return;
    seen.add(p);
    for (const c of children(p)) walk(c);
  };
  walk(pid);
  for (const p of seen) {
    try {
      process.kill(p, "SIGKILL");
    } catch {
      // already gone
    }
  }
}

function exited(proc: ChildProcess): Promise<void> {
  if (proc.exitCode !== null || proc.signalCode !== null) return Promise.resolve();
  return new Promise<void>((resolve) => proc.once("exit", () => resolve()));
}

const CAPTURE_MS = 3_000;
const CAPTURE_HTML_MAX = 2_000_000;
/** The name the at-close attachments carry; `at-close-reporter.ts` deletes them for a test that passed. */
export const AT_CLOSE = "app-at-close";
let captures = 0;

const bounded = <T>(p: Promise<T>): Promise<T | undefined> => Promise.race([p.catch(() => undefined), new Promise<undefined>((r) => setTimeout(() => r(undefined), CAPTURE_MS))]);

/**
 * What each window showed when the test let go of the app: a screenshot and the document. Playwright's trace of an Electron
 * test holds the steps and no picture of the page, so a failure that is "the page never showed X" said nothing about what the
 * page showed instead (a note, a refusal, another view). A test cannot know in its own `finally` that it failed, so this is
 * taken every time and the reporter keeps it only for a test that did not pass. Every step is bounded and none can fail the
 * test: an app that is already gone simply leaves nothing.
 */
async function captureAtClose(app: ElectronApplication): Promise<void> {
  let info: ReturnType<typeof test.info>;
  try {
    info = test.info();
  } catch {
    return; // not inside a test (a worker-level teardown)
  }
  let pages: ReturnType<ElectronApplication["windows"]>;
  try {
    pages = app.windows();
  } catch {
    return;
  }
  for (const page of pages) {
    const base = info.outputPath(`at-close-${++captures}`);
    const shot = await bounded(page.screenshot({ path: `${base}.jpg`, type: "jpeg", quality: 60, timeout: CAPTURE_MS }));
    if (shot) info.attachments.push({ name: AT_CLOSE, path: `${base}.jpg`, contentType: "image/jpeg" });
    const html = await bounded(page.evaluate(() => document.documentElement.outerHTML));
    if (typeof html === "string") {
      try {
        writeFileSync(`${base}.html`, html.slice(0, CAPTURE_HTML_MAX));
        info.attachments.push({ name: AT_CLOSE, path: `${base}.html`, contentType: "text/html" });
      } catch {
        // the evidence is best effort
      }
    }
  }
}

export async function closeApp(app: ElectronApplication): Promise<void> {
  await captureAtClose(app);
  const proc = app.process();
  const pid = proc.pid;
  await Promise.race([app.close().catch(() => {}), new Promise<void>((r) => setTimeout(r, GRACE_MS))]);
  if (proc.exitCode === null && proc.signalCode === null && pid !== undefined) killTree(pid);
  await Promise.race([exited(proc), new Promise<void>((r) => setTimeout(r, REAP_MS))]);
}
