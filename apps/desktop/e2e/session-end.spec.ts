/**
 * REQ-PX-049 / AFW-J11 on the real stack: the packaged Electron app and a real Core, with real hook processes.
 *
 * Quitting ends the session in the Core: its session-end hooks run to completion before the process exits and are
 * journaled, a hook that fails (or a Core that dies while one runs) is shown to the person, and the resume choice made at
 * the quit is a Core input: the next start recovers the run, shows the Core's counts and, when resume was chosen,
 * continues it. (The native message box is replaced in the main process by a recorder that writes what it was asked to a
 * file, because a test cannot press a native button; everything it is asked is real.)
 */
import { expect, test, type ElectronApplication, type Page } from "@playwright/test";
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { DatabaseSync } from "node:sqlite";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { closeApp, launch, makeRepo } from "./support/ui-harness.ts";
import { gate, steppedModel, type Step } from "./support/apps-harness.ts";

/** A message-box recorder that appends every question to `file`, so it can be read after the app has gone. */
async function recordDialogToFile(app: ElectronApplication, answer: number, file: string): Promise<void> {
  await app.evaluate(
    ({ dialog }, { response, path }) => {
      const fs = process.getBuiltinModule("node:fs");
      (dialog as unknown as { showMessageBox: unknown }).showMessageBox = async (...args: unknown[]) => {
        const o = (args.find((a) => typeof a === "object" && a !== null && "message" in (a as object)) ?? {}) as { message: string; detail: string; buttons: string[] };
        fs.appendFileSync(path, `${JSON.stringify({ message: o.message, detail: o.detail, buttons: o.buttons })}\n`);
        return { response, checkboxChecked: false };
      };
    },
    { response: answer, path: file },
  );
}
const asksIn = (file: string): { message: string; detail: string; buttons: string[] }[] => (existsSync(file) ? readFileSync(file, "utf8").split("\n").filter(Boolean).map((l) => JSON.parse(l)) : []);
const exited = (app: ElectronApplication) => new Promise<void>((resolve) => (app.process().exitCode !== null ? resolve() : app.process().once("exit", () => resolve())));
const shutdownLog = (dataDir: string): unknown => {
  const f = join(dataDir, "shutdown-log.json");
  return existsSync(f) ? JSON.parse(readFileSync(f, "utf8")) : null;
};

/** The Core's own log, read from the profile after the app has gone: the journal entries the hooks and the quit left. */
function journal(dataDir: string, types: string[]): { type: string; payload: Record<string, unknown> }[] {
  const db = new DatabaseSync(join(dataDir, "core", "core.db"), { readOnly: true });
  try {
    const rows = db.prepare(`SELECT event_type, payload_inline FROM events WHERE event_type IN (${types.map(() => "?").join(",")}) ORDER BY offset`).all(...types) as { event_type: string; payload_inline: string | null }[];
    return rows.map((r) => {
      const p = JSON.parse(r.payload_inline ?? "{}") as Record<string, unknown>;
      return { type: r.event_type, payload: (p.payload as Record<string, unknown> | undefined) ?? p };
    });
  } finally {
    db.close();
  }
}

/**
 * Declares one session-end hook for the profile (the Core reads `admin-config.json` from its data directory) and returns where the
 * hook leaves its marks. The hook is a real process: it records the pid of its parent (the Core), waits `ms`, writes a sentinel and
 * answers with a reason (or exits 1 when `fail`).
 */
function declareSessionEndHook(dataDir: string, o: { ms: number; fail?: boolean }): { started: string; done: string; request: string } {
  const dir = mkdtempSync(join(tmpdir(), "modbit-e2e-hook-"));
  const marks = { started: join(dir, "started.txt"), done: join(dir, "done.txt"), request: join(dir, "request.json") };
  const script = join(dir, "hook.cjs");
  writeFileSync(
    script,
    `const fs = require("fs");
let input = "";
process.stdin.on("data", (d) => (input += d));
process.stdin.on("end", () => {
  fs.writeFileSync(${JSON.stringify(marks.request)}, input);
  fs.writeFileSync(${JSON.stringify(marks.started)}, String(process.ppid));
  setTimeout(() => {
    if (${o.fail === true}) { process.stderr.write("archive store is full"); process.exit(1); }
    fs.writeFileSync(${JSON.stringify(marks.done)}, "done");
    process.stdout.write(JSON.stringify({ decision: "continue", reason: "archived 3 files" }));
  }, ${o.ms});
});
`,
  );
  const hook = { name: "archive", point: "session_end", command: [process.execPath, script], timeout_ms: 20_000, fail_policy: "open" };
  writeFileSync(join(dataDir, "admin-config.json"), JSON.stringify({ hooks: [JSON.stringify(hook)] }));
  return marks;
}

async function createTaskCard(page: Page, repo: string) {
  await page.getByTestId("goal").fill("Edit the notes");
  await page.getByTestId("workspace").fill(repo);
  await page.getByTestId("run").click();
  const card = page.getByTestId("task-card").first();
  await expect(card).toBeVisible({ timeout: 30_000 });
  return card;
}

async function startedCard(page: Page, repo: string) {
  const card = await createTaskCard(page, repo);
  await card.getByTestId("task-start").click();
  await expect(card.getByTestId("task-state")).toHaveText("Running", { timeout: 60_000 });
  return card;
}

/** A task held mid-run by the model, and a count of the model requests it has made. */
async function heldRun(label: string) {
  const repo = makeRepo(mkdtempSync(join(tmpdir(), "modbit-resume-repo-")));
  const first = gate();
  const later = gate();
  const seen = { requests: 0 };
  const script: Step[] = [
    { calls: [{ name: "plan.update", args: { outcome: "edit notes", expected_files: ["notes.txt"] } }] },
    async () => {
      seen.requests += 1;
      await (seen.requests === 1 ? first.wait : later.wait);
      return { text: "Nothing further." };
    },
  ];
  const { server, url } = await steppedModel(script);
  const dataDir = mkdtempSync(join(tmpdir(), `modbit-e2e-${label}-`));
  return { repo, first, later, seen, server, dataDir, env: { MODBIT_OPENAI_BASE_URL: url, MODBIT_QUIT_PROMPT: "" } };
}

test("PX-049: a session-end hook that takes 2 s completes before the app exits, and what it said is journaled", async () => {
  const repo = makeRepo(mkdtempSync(join(tmpdir(), "modbit-hook-repo-")));
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-hook-ok-"));
  const marks = declareSessionEndHook(dataDir, { ms: 2000 });
  const { app, page } = await launch(dataDir);
  await createTaskCard(page, repo);
  const done = exited(app);
  const t0 = Date.now();
  await app.evaluate(({ app: a }) => a.quit());
  await done;
  expect(Date.now() - t0, "the quit waited for the hook").toBeGreaterThanOrEqual(1900);
  expect(existsSync(marks.done), "the hook ran to completion before the process exited").toBe(true);
  const sent = JSON.parse(readFileSync(marks.request, "utf8")) as { point: string; payload: { reason: string } };
  expect(sent.point).toBe("session_end");
  expect(sent.payload.reason).toBe("quit");
  const entries = journal(dataDir, ["HookInvoked", "SessionEnding"]);
  const hook = entries.find((e) => e.type === "HookInvoked" && e.payload.point === "session_end");
  expect(hook, JSON.stringify(entries)).toBeTruthy();
  expect(hook!.payload.outcome).toBe("OK");
  expect(hook!.payload.detail, "its output is in the journal").toBe("archived 3 files");
  expect(entries.find((e) => e.type === "SessionEnding")?.payload.hooks_failed).toBe(0);
  expect(shutdownLog(dataDir), "a hook that finished leaves no failure").toBeNull();
});

test("PX-049: a session-end hook that fails is shown to the person and journaled", async () => {
  const repo = makeRepo(mkdtempSync(join(tmpdir(), "modbit-hook-repo-")));
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-hook-fail-"));
  declareSessionEndHook(dataDir, { ms: 100, fail: true });
  const { app, page } = await launch(dataDir, { env: { MODBIT_QUIT_PROMPT: "" } });
  const asks = join(dataDir, "asked.jsonl");
  await recordDialogToFile(app, 0, asks);
  await createTaskCard(page, repo);
  const done = exited(app);
  await app.evaluate(({ app: a }) => a.quit());
  await done;
  const shown = asksIn(asks);
  expect(shown, "the failure was put in front of the person").toHaveLength(1);
  expect(shown[0]!.message).toBe("Modbit did not finish closing cleanly.");
  expect(shown[0]!.detail).toContain("1 session-end hook did not finish cleanly");
  expect(shown[0]!.detail).toContain("archive");
  expect(shown[0]!.detail).toContain("failed");
  const log = shutdownLog(dataDir) as { failed: { name: string; reason: string }[]; hooks: { outcome: string }[] };
  expect(log.failed[0]!.name).toBe("session-end");
  expect(log.hooks[0]!.outcome).toBe("FAILED");
  const entries = journal(dataDir, ["HookInvoked", "SessionEnding"]);
  expect(entries.find((e) => e.type === "HookInvoked" && e.payload.point === "session_end")?.payload.outcome).toBe("FAILED");
  expect(entries.find((e) => e.type === "SessionEnding")?.payload.hooks_failed).toBe(1);
});

test("PX-049: choosing resume at the quit records it in the Core, and the next start recovers the run, shows the counts and continues it", async () => {
  const h = await heldRun("resume");
  let { app, page } = await launch(h.dataDir, { env: h.env });
  try {
    await startedCard(page, h.repo);
    await expect.poll(() => h.seen.requests, { timeout: 60_000, message: "the run is waiting on the model" }).toBe(1);
    const asks = join(h.dataDir, "asked.jsonl");
    await recordDialogToFile(app, 1, asks);
    const done = exited(app);
    await app.evaluate(({ app: a }) => a.quit());
    await done;
    const question = asksIn(asks)[0]!;
    expect(question.buttons).toEqual(["Keep Modbit open", "Quit and resume on reopen", "Quit and leave suspended"]);
    const ending = journal(h.dataDir, ["SessionEnding"]);
    expect(ending, "the choice is on the Core's log").toHaveLength(1);
    expect(ending[0]!.payload.resume_on_reopen).toBe(true);
    expect((ending[0]!.payload.running_tasks as unknown[]).length).toBe(1);

    // The next start: nobody clicks anything. The Core's recovery report names the task, the desktop resumes it, and the model is asked again.
    ({ app, page } = await launch(h.dataDir, { env: h.env }));
    await expect.poll(() => h.seen.requests, { timeout: 60_000, message: "the run continued without anyone pressing Start" }).toBeGreaterThanOrEqual(2);
    const summary = page.getByTestId("recovery-summary");
    await expect(summary).toBeVisible({ timeout: 30_000 });
    const text = (await summary.textContent())!;
    expect(text).toMatch(/^Recovered at start: /);
    const card = page.getByTestId("task-card").first();
    await expect(card.getByTestId("task-state")).toHaveText("Running", { timeout: 30_000 });
    // The counts are the Core's: what the summary says is resumed is what the Running column shows.
    const running = await page.getByTestId("column-running").getByTestId("task-card").count();
    expect(Number(/(\d+) tasks? resumed/.exec(text)?.[1] ?? 0)).toBe(running);
  } finally {
    h.first.open();
    h.later.open();
    await closeApp(app);
    h.server.close();
  }
});

test("PX-049: quitting and leaving the run suspended keeps it recoverable, and nothing continues on the next start", async () => {
  const h = await heldRun("suspend");
  let { app, page } = await launch(h.dataDir, { env: h.env });
  try {
    await startedCard(page, h.repo);
    await expect.poll(() => h.seen.requests, { timeout: 60_000, message: "the run is waiting on the model" }).toBe(1);
    const asks = join(h.dataDir, "asked.jsonl");
    await recordDialogToFile(app, 2, asks);
    const done = exited(app);
    await app.evaluate(({ app: a }) => a.quit());
    await done;
    expect(journal(h.dataDir, ["SessionEnding"])[0]!.payload.resume_on_reopen).toBe(false);

    ({ app, page } = await launch(h.dataDir, { env: h.env }));
    await expect(page.getByTestId("recovery-summary")).toBeVisible({ timeout: 30_000 });
    await expect(page.getByTestId("column-needsAttention").getByTestId("task-card")).toHaveCount(1, { timeout: 30_000 });
    await page.waitForTimeout(4000);
    expect(h.seen.requests, "no model request: the run waits for the person").toBe(1);
    expect(await page.getByTestId("column-running").getByTestId("task-card").count()).toBe(0);
  } finally {
    h.first.open();
    h.later.open();
    await closeApp(app);
    h.server.close();
  }
});

test("PX-049: killing the Core while the session-end hook runs fails visibly and leaves the task recoverable (the unrecorded choice does not resume it)", async () => {
  const h = await heldRun("kill-in-shutdown");
  const marks = declareSessionEndHook(h.dataDir, { ms: 8000 });
  let { app, page } = await launch(h.dataDir, { env: h.env });
  try {
    await startedCard(page, h.repo);
    await expect.poll(() => h.seen.requests, { timeout: 60_000, message: "the run is waiting on the model" }).toBe(1);
    const asks = join(h.dataDir, "asked.jsonl");
    // Resume is chosen, but the Core dies before it can record the choice.
    await recordDialogToFile(app, 1, asks);
    const done = exited(app);
    await app.evaluate(({ app: a }) => a.quit());
    await expect.poll(() => existsSync(marks.started), { timeout: 30_000 }).toBe(true);
    process.kill(Number(readFileSync(marks.started, "utf8")), "SIGKILL");
    await done;
    const failure = asksIn(asks).find((q) => q.message === "Modbit did not finish closing cleanly.");
    expect(failure, "the failure was shown").toBeTruthy();
    expect(failure!.detail).toContain("session-end hooks may not have run");
    expect(existsSync(marks.done), "the killed hook never completed").toBe(false);
    expect(journal(h.dataDir, ["SessionEnding"]), "the Core died before recording the choice").toHaveLength(0);

    rmSync(join(h.dataDir, "admin-config.json"));
    ({ app, page } = await launch(h.dataDir, { env: h.env }));
    await expect(page.getByTestId("recovery-summary")).toBeVisible({ timeout: 30_000 });
    await expect(page.getByTestId("column-needsAttention").getByTestId("task-card")).toHaveCount(1, { timeout: 30_000 });
    await page.waitForTimeout(3000);
    expect(h.seen.requests, "recoverable, not resumed").toBe(1);
  } finally {
    h.first.open();
    h.later.open();
    await closeApp(app);
    h.server.close();
  }
});
