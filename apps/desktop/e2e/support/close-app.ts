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

export async function closeApp(app: ElectronApplication): Promise<void> {
  const proc = app.process();
  const pid = proc.pid;
  await Promise.race([app.close().catch(() => {}), new Promise<void>((r) => setTimeout(r, GRACE_MS))]);
  if (proc.exitCode === null && proc.signalCode === null && pid !== undefined) killTree(pid);
  await Promise.race([exited(proc), new Promise<void>((r) => setTimeout(r, REAP_MS))]);
}
