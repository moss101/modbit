/**
 * REQ-PX-048 on the real stack: the packaged Electron app, a real Core, a real
 * `modbit-execd` broker and real PTYs, a real Git checkout, and a scripted
 * OpenAI-compatible model (the repo's standard model stand-in) that holds the
 * task Running at a gate the test opens. One task is started; the specs below
 * use the panel while it is running, in order, on the same app.
 *
 * Terminals are the test runner's own Node on a PTY, so nothing assumes `sh`
 * or `stty`. Nothing here waits for a fixed time: every wait is for a
 * condition the Core or the page reports.
 */
import { expect, test, type ElectronApplication, type Page } from "@playwright/test";
import { mkdirSync, mkdtempSync, realpathSync, symlinkSync, writeFileSync } from "node:fs";
import type { Server } from "node:http";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createHash } from "node:crypto";
import { BIG_OUTPUT, ECHO_LOOP, EXIT_THREE, expectTerminals, SIZE_REPORTER, gate, handlesIn, nodeTerminal, readTerminal, steppedModel, toolResults, type Step } from "./support/apps-harness.ts";
import { accessible, closeApp, git, launch, makeRepo, MOD, setContentSize } from "./support/ui-harness.ts";

test.describe.configure({ mode: "serial" });

let app: ElectronApplication;
let page: Page;
let server: Server;
let repo: string;
let dataDir: string;
let sessionId = "";
let taskId = "";
const modelGate = gate();
const seen: { shellInput: string | null } = { shellInput: null };
const handles: string[] = [];
const sha = (bytes: number[]) => createHash("sha256").update(Uint8Array.from(bytes)).digest("hex");

const terminalView = () => page.getByTestId("terminal-view");
const screen = () => page.getByTestId("terminal-screen");
const listTerminals = () => page.evaluate((t) => window.modbit.terminalList(t).then((l) => l.terminals), taskId);
const pickTerminal = async (handle: string) => {
  await page.locator(`[data-testid="terminal-row"][data-terminal-id="${handle}"] [data-testid="terminal-pick"]`).click();
  await expect(terminalView()).toHaveAttribute("data-terminal-id", handle);
};

test.beforeAll(async () => {
  repo = mkdtempSync(join(tmpdir(), "modbit-apps-repo-"));
  // Files the Files app must refuse or describe, committed with the base so they are not part of the task's change set.
  writeFileSync(join(repo, "blob.bin"), Buffer.from([0, 159, 146, 150, 0, 1, 2]));
  writeFileSync(join(repo, "big.txt"), "a".repeat(300 * 1024));
  writeFileSync(join(repo, ".env"), "SECRET=1\n");
  mkdirSync(join(repo, "src"), { recursive: true });
  writeFileSync(join(repo, "src", "lib.rs"), "pub fn answer() -> u32 {\n    42\n}\n");
  const outside = mkdtempSync(join(tmpdir(), "modbit-apps-outside-"));
  writeFileSync(join(outside, "secret.txt"), "outside the workspace\n");
  if (process.platform !== "win32") symlinkSync(outside, join(repo, "escape"));
  makeRepo(repo);

  // A worktree for the agent to close later: closing it is a protected effect, which waits for the person's approval and then leaves receipts.
  const wt = join(`${repo}.modbit-worktrees`, "held");
  git(repo, "worktree", "add", "-q", "-b", "task/held", wt);
  const wtReal = realpathSync.native(wt).replace(/^\\\\\?\\/, "");
  // One turn starts all four terminals: a turn that makes no progress counts toward the Core's no-progress guard, and four of them would suspend the task.
  const script: Step[] = [
    { calls: [{ name: "plan.update", args: { outcome: "edit notes", expected_files: ["notes.txt"], protected_effects: ["git.worktree.close"] } }] },
    { calls: [{ name: "fs.read", args: { path: "notes.txt" } }] },
    { calls: [{ name: "change.apply", args: { path: "notes.txt", op: "replace", content: "line 1\nline 2 changed\nline 3\n" } }] },
    { calls: [nodeTerminal(ECHO_LOOP), nodeTerminal(SIZE_REPORTER), nodeTerminal(BIG_OUTPUT), nodeTerminal(EXIT_THREE)] },
  ];
  // The request after the terminals' four results (3 + 4 = 7 tool results) is held until the test opens the gate. Then, in one turn, the agent
  // tries to type into the terminal the person holds and asks to close the worktree (which waits for the person's approval).
  script[7] = async ({ body }) => {
    handles.push(...handlesIn(body));
    await modelGate.wait;
    return { calls: [{ name: "shell.input", args: { session_id: handles[0], text: "agent-typed\n" } }, { name: "git.worktree.close", args: { path: wtReal } }] };
  };
  script[9] = ({ body }) => {
    seen.shellInput = toolResults(body).at(-2) ?? null;
    return { calls: [{ name: "task.complete", args: { summary: "edited", self_review: { findings: [] } } }] };
  };
  const m = await steppedModel(script);
  server = m.server;
  dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-apps-"));
  ({ app, page } = await launch(dataDir, { env: { MODBIT_OPENAI_BASE_URL: m.url } }));
  await setContentSize(app, page, 1280, 800);
});

test.afterAll(async () => {
  modelGate.open();
  if (app) await closeApp(app);
  server?.close();
});

test("the panel is absent until the task runs; then Changes shows the live diff while the task is still Running", async () => {
  await page.getByTestId("goal").fill("Edit the notes");
  await page.getByTestId("workspace").fill(repo);
  await page.getByTestId("run").click();
  const card = page.getByTestId("task-card").first();
  taskId = (await card.getAttribute("data-task-id"))!;
  sessionId = (await page.evaluate(() => window.modbit.localState()).then((s) => s.sessionId))!;
  expect(await page.getByTestId("region-apps").count(), "a task that has not started has no artifact").toBe(0);
  await card.getByTestId("task-start").click();
  await expect(card.getByTestId("task-state")).toHaveText("Running", { timeout: 60_000 });
  // Four terminals appear as the script starts them; the task is held at the gate.
  await expectTerminals(page, taskId, 4);
  await card.focus();
  await page.keyboard.press(`${MOD}+e`);
  await expect(page.getByTestId("region-apps")).toBeVisible();
  await expect(page.getByTestId("app-changes")).toHaveAttribute("data-task-state", "Running");
  await expect(page.getByTestId("changes-file")).toHaveCount(1);
  await expect(page.getByTestId("changes-hunk")).toContainText("line 2 changed");
  await expect(page.getByTestId("changes-note")).toContainText("still working");
  await accessible(page, "changes app for a running task", '[data-testid="region-apps"]');
  // The full file comes from the Core's code view, with the changed line marked.
  await page.getByTestId("changes-show-full").click();
  await expect(page.getByTestId("changes-full")).toBeVisible();
  await expect(page.locator('[data-testid="changes-full"] [data-changed="true"]')).toContainText("line 2 changed");
});

test("Terminal: owners and states are listed, an exited terminal shows its code, typing reaches the PTY and the output comes back", async () => {
  await page.getByTestId("tab-terminal").click();
  const rows = page.getByTestId("terminal-row");
  await expect(rows).toHaveCount(4);
  const all = await listTerminals();
  expect(all.every((t) => t.owner === "agent")).toBe(true);
  // The four start in one turn, so tell them apart by their command, not by start time.
  const byCommand = (needle: string) => all.find((t) => t.title.includes(needle) || t.cwd.includes(needle))!;
  const echo = byCommand("READY");
  const size = byCommand("process.stdout.columns");
  const big = byCommand("repeat(1019)");
  const exited = byCommand("process.exit(3)");
  expect([echo, size, big, exited].every((t) => t !== undefined), JSON.stringify(all.map((t) => t.title))).toBe(true);
  handles.splice(0, handles.length, echo.terminalId, size.terminalId, big.terminalId, exited.terminalId);
  await expect(page.locator(`[data-terminal-id="${exited.terminalId}"] [data-testid="terminal-state"]`)).toContainText("exited 3", { timeout: 30_000 });
  await expect(page.locator(`[data-terminal-id="${echo.terminalId}"] [data-testid="terminal-state"]`)).toContainText("running");

  // The exited terminal replays its output and states how it ended.
  await pickTerminal(exited.terminalId);
  // The replay of a 111-byte terminal can take longer than the default 5s to reach the screen on the loaded Windows runner (the failure
  // diagnostics showed the line on screen moments after the timeout); the assertion is unchanged.
  await expect(screen()).toContainText("finished", { timeout: 30_000 }).catch(async (e: Error) => {
    const log = await readTerminal(page, { sessionId, taskId, terminalId: exited.terminalId, after: "0", windowBytes: 65536 });
    const view = await terminalView().evaluate((el) => ({ phase: el.getAttribute("data-phase"), cursor: el.getAttribute("data-cursor"), bytes: el.getAttribute("data-bytes"), rows: el.querySelectorAll(".xterm-rows > div").length, text: el.querySelector(".xterm-rows")?.textContent?.replace(/\s+/g, " ") ?? null }));
    const row = (await listTerminals()).find((t) => t.terminalId === exited.terminalId);
    throw new Error(`${e.message}\nthe Core's replay of the exited terminal: ${JSON.stringify(new TextDecoder().decode(Uint8Array.from(log.bytes)))}\nthe view: ${JSON.stringify(view)}\nthe listed terminal: ${JSON.stringify(row)}`);
  });
  await expect(page.getByTestId("terminal-exit")).toContainText("Exited with code 3");
  await accessible(page, "terminal app, an exited terminal", '[data-testid="region-apps"]');

  await pickTerminal(echo.terminalId);
  await expect(screen()).toContainText("READY");
  await expect(page.getByTestId("terminal-owner-note")).toContainText("Agent terminal");
  await screen().click();
  await page.keyboard.type("hello-pty");
  await page.keyboard.press("Enter");
  await expect(screen()).toContainText("got:hello-pty");
  await accessible(page, "terminal app, a live terminal with typed input", '[data-testid="region-apps"]');
});

test("the person's input lease: held while typing, the agent's shell.input is refused meanwhile, released on blur", async () => {
  const echo = handles[0]!;
  await expect(terminalView()).toHaveAttribute("data-lease", "held");
  await expect.poll(async () => (await listTerminals()).find((t) => t.terminalId === echo)!.inputLeaseHolder).toMatch(/^user:/);
  // Now the agent tries to type into the terminal the person holds. Its next call in the same turn (closing a worktree) waits for the
  // person's approval, so the card asks for it only after the input attempt has been decided: the lease is still held until then.
  modelGate.open();
  await expect(page.getByTestId("task-card").first().getByTestId("task-approval")).toBeVisible({ timeout: 60_000 });
  await expect(terminalView()).toHaveAttribute("data-lease", "held");
  expect(await screen().textContent()).not.toContain("agent-typed");
  // Leaving the terminal gives the lease back.
  await page.getByTestId("terminal-leave").focus();
  await expect(terminalView()).toHaveAttribute("data-lease", "free");
  await expect.poll(async () => (await listTerminals()).find((t) => t.terminalId === echo)!.inputLeaseHolder).toBe("");

  // The agent's worktree close is waiting for the person: approve the exact intent from the card (y, then Enter), as the keyboard model does.
  const card = page.getByTestId("task-card").first();
  await card.focus();
  await page.keyboard.press("y");
  await expect(page.getByTestId("confirm")).toHaveAttribute("data-confirm-kind", "approve");
  await page.keyboard.press("Enter");
  await expect(page.getByTestId("confirm")).toHaveCount(0);
  // The model's next request carries the result of the agent's earlier input attempt: refused while the person held the lease.
  await expect.poll(() => seen.shellInput, { timeout: 60_000 }).not.toBeNull();
  expect(seen.shellInput!, "the Core refused the agent's input while the person held the lease").toContain("INPUT_LEASED");
});

test("replay: a reload shows the same bytes; two attaches from the start read identical bytes (byte-exact)", async () => {
  const echo = handles[0]!;
  const before = await terminalView().getAttribute("data-cursor");
  const first = await readTerminal(page, { sessionId, taskId, terminalId: echo, after: "0", windowBytes: 4096 });
  expect(first.bytes.length).toBeGreaterThan(0);
  expect(new TextDecoder().decode(Uint8Array.from(first.bytes))).toContain("got:hello-pty");
  expect(String(first.bytes.length)).toBe(first.head);

  await page.reload();
  await expect(page.getByTestId("core-status")).toContainText("Core connected", { timeout: 60_000 });
  // The panel, its tab and the terminal come back from the per-task state; the screen is rebuilt from the Core's replay.
  await expect(page.getByTestId("tab-terminal")).toHaveAttribute("aria-selected", "true", { timeout: 30_000 });
  await pickTerminal(echo);
  // The screen is rebuilt from the Core's replay after a reload; on a loaded runner that takes longer than the default wait.
  await expect(screen()).toContainText("got:hello-pty", { timeout: 30_000 });
  await expect(screen()).toContainText("READY", { timeout: 30_000 });
  if (process.platform === "win32") {
    // A Windows pseudo-console repaints its screen when the viewer attaches at a size (the reload's resize), so the
    // log grows by that repaint (160 bytes on the runner) after the first read. The exact claims are the ones the log
    // itself makes: the view stands at the Core's head, and everything read before is still the log's first bytes.
    await expect
      .poll(async () => {
        const head = (await listTerminals()).find((t) => t.terminalId === echo)!.bytesSoFar;
        return (await terminalView().getAttribute("data-cursor")) === head;
      }, { timeout: 30_000 })
      .toBe(true);
    const second = await readTerminal(page, { sessionId, taskId, terminalId: echo, after: "0", windowBytes: 65536 });
    expect(sha(second.bytes.slice(0, first.bytes.length))).toBe(sha(first.bytes));
    expect(second.bytes.length).toBeGreaterThanOrEqual(first.bytes.length);
    return;
  }
  await expect(terminalView()).toHaveAttribute("data-cursor", before!);
  const second = await readTerminal(page, { sessionId, taskId, terminalId: echo, after: "0", windowBytes: 65536 });
  expect(sha(second.bytes)).toBe(sha(first.bytes));
});

test("resize: the viewer's size reaches the PTY and the child prints it", async () => {
  const sizeTerm = handles[1]!;
  await pickTerminal(sizeTerm);
  // The Core records the size the view sent; the child's own report of its terminal must say the same.
  await expect
    .poll(async () => {
      const t = (await listTerminals()).find((x) => x.terminalId === sizeTerm)!;
      return `SIZE ${t.cols} ${t.rows}`;
    })
    .not.toBe("SIZE 80 24");
  const reported = async () => {
    const t = (await listTerminals()).find((x) => x.terminalId === sizeTerm)!;
    return { want: `SIZE ${t.cols} ${t.rows}`, cols: t.cols };
  };
  const first = await reported();
  await expect(screen()).toContainText(first.want, { timeout: 30_000 });
  // A narrower window changes the panel, the view refits, and the child reports the new size.
  await setContentSize(app, page, 1100, 800); // beside the conversation the panel is 416 px here, against 596 px at 1280
  await expect.poll(async () => (await reported()).cols, { timeout: 30_000 }).not.toBe(first.cols);
  const second = await reported();
  await expect(screen()).toContainText(second.want, { timeout: 30_000 });
  await setContentSize(app, page, 1280, 800);
});

test("slow consumer at the IPC level: the Core sends no more than the window beyond the last acknowledgement", async () => {
  const bigTerm = handles[2]!;
  const r = await page.evaluate(
    async ({ sessionId, taskId, terminalId }) => {
      const WINDOW = 4096;
      const att = await window.modbit.terminalAttach(sessionId, taskId, terminalId, "0", { windowBytes: WINDOW });
      let acked = 0n;
      let received = 0n;
      let worst = 0n;
      let lastFrameAt = Date.now();
      const off = window.modbit.onTerminalFrame((f) => {
        if (f.attachId !== att.attachId || f.kind !== "output") return;
        received = BigInt(f.cursor) + BigInt(f.data.length);
        if (received - acked > worst) worst = received - acked;
        lastFrameAt = Date.now();
      });
      const quiet = async () => {
        // Quiet means no frame for 400 ms after at least one arrived; the Core has nothing more it may send.
        while (Date.now() - lastFrameAt < 400) await new Promise((res) => setTimeout(res, 50));
      };
      await quiet();
      const stalledAt = received;
      // Consume in steps: each acknowledgement lets the Core send up to one more window.
      const steps: string[] = [];
      for (let i = 0; i < 4; i++) {
        acked = received;
        await window.modbit.terminalAck(att.attachId, acked.toString());
        lastFrameAt = Date.now();
        await quiet();
        steps.push(`${acked}->${received}`);
      }
      off();
      await window.modbit.terminalDetach(att.attachId).catch(() => undefined);
      return { window: WINDOW, stalledAt: Number(stalledAt), worst: Number(worst), finalReceived: Number(received), grantedWindow: att.windowBytes };
    },
    { sessionId, taskId, terminalId: bigTerm },
  );
  expect(r.grantedWindow).toBe("4096");
  // The broker pushes the next chunk while fewer than a window of bytes are unacknowledged, so the bound is the window plus one chunk (64 KiB at most).
  const CHUNK = 64 * 1024;
  expect(r.stalledAt, "the Core sent something, then stopped without an acknowledgement").toBeGreaterThan(0);
  expect(r.stalledAt, "it stopped within a window plus a chunk, with some 400 KiB waiting on disk").toBeLessThanOrEqual(r.window + CHUNK);
  expect(r.stalledAt).toBeLessThan(100 * 1024);
  expect(r.worst, "never more than a window plus a chunk beyond the last acknowledgement").toBeLessThanOrEqual(r.window + CHUNK);
  expect(r.finalReceived, "acknowledging lets the stream move on").toBeGreaterThan(r.stalledAt);
});

test("a stopped terminal: Stop warns that a running process ends, and the list shows it stopped", async () => {
  const sizeTerm = handles[1]!;
  await page.locator(`[data-terminal-id="${sizeTerm}"] [data-testid="terminal-kill"]`).click();
  await expect(page.getByTestId("terminal-kill-dialog")).toContainText("still running");
  await accessible(page, "the stop-terminal confirmation");
  await page.getByTestId("terminal-kill-cancel").click();
  expect((await listTerminals()).find((t) => t.terminalId === sizeTerm)!.state, "keeping it running changes nothing").toBe("RUNNING");
  await page.locator(`[data-terminal-id="${sizeTerm}"] [data-testid="terminal-kill"]`).click();
  await page.getByTestId("terminal-kill-confirm").click();
  await expect(page.locator(`[data-terminal-id="${sizeTerm}"] [data-testid="terminal-state"]`)).toContainText("stopped", { timeout: 30_000 });
});

test("Files: the workspace is browsable read-only; binary, oversized, protected and outside paths are refused in the Core's terms", async () => {
  await page.getByTestId("tab-files").click();
  await expect(page.getByTestId("files-tree")).toBeVisible();
  await expect(page.locator('[data-testid="files-entry"][data-path="notes.txt"]')).toBeVisible();
  // A text file opens.
  await page.locator('[data-testid="files-entry"][data-path="notes.txt"] [data-testid="files-file"]').click();
  await expect(page.getByTestId("files-text")).toContainText("line 2 changed");
  // A directory expands lazily.
  await page.locator('[data-testid="files-entry"][data-path="src"] [data-testid="files-dir"]').click();
  await page.locator('[data-testid="files-entry"][data-path="src/lib.rs"] [data-testid="files-file"]').click();
  await expect(page.getByTestId("files-text")).toContainText("pub fn answer");
  // Binary and oversized files are refused with a typed message, not shown.
  await page.locator('[data-testid="files-entry"][data-path="blob.bin"] [data-testid="files-file"]').click();
  await expect(page.getByTestId("files-note")).toContainText("Binary file");
  await page.locator('[data-testid="files-entry"][data-path="big.txt"] [data-testid="files-file"]').click();
  await expect(page.getByTestId("files-note")).toContainText("Too large to show inline");
  // A protected file is named and cannot be opened.
  await expect(page.locator('[data-testid="files-entry"][data-path=".env"] [data-testid="files-withheld"]')).toContainText("protected");
  await expect(page.locator('[data-testid="files-entry"][data-path=".env"] [data-testid="files-file"]')).toHaveCount(0);
  await accessible(page, "files app", '[data-testid="region-apps"]');

  // Underneath the buttons, the typed calls refuse what is not the task's workspace.
  const refused = await page.evaluate(async (taskId) => {
    const ask = async (fn: () => Promise<unknown>) => fn().then(() => "ALLOWED", (e: Error) => e.message);
    return {
      climb: await ask(() => window.modbit.readWorkspaceFile(taskId, "../outside.txt")),
      absolute: await ask(() => window.modbit.readWorkspaceFile(taskId, "/etc/hosts")),
      drive: await ask(() => window.modbit.readWorkspaceFile(taskId, "C:\\Windows\\win.ini")),
      listClimb: await ask(() => window.modbit.listWorkspaceDir(taskId, "src/../..")),
      protectedFile: await ask(() => window.modbit.readWorkspaceFile(taskId, ".env")),
      symlink: await ask(() => window.modbit.readWorkspaceFile(taskId, "escape/secret.txt")),
      symlinkDir: await ask(() => window.modbit.listWorkspaceDir(taskId, "escape")),
      missing: await ask(() => window.modbit.readWorkspaceFile(taskId, "nope.txt")),
      notAHandle: await ask(() => window.modbit.readWorkspaceFile("not-a-task", "notes.txt")),
    };
  }, taskId);
  expect(refused.climb).toContain("BAD_ARGUMENT");
  expect(refused.absolute).toContain("BAD_ARGUMENT");
  expect(refused.drive).toContain("BAD_ARGUMENT");
  expect(refused.listClimb).toContain("BAD_ARGUMENT");
  expect(refused.protectedFile).toContain("PROTECTED");
  expect(refused.missing).toContain("NOT_FOUND");
  expect(refused.notAHandle).toContain("BAD_ARGUMENT");
  if (process.platform !== "win32") {
    expect(refused.symlink, "a link that leaves the workspace is refused by the Core's path policy, not by this process").toContain("OUTSIDE_ROOT");
    expect(refused.symlinkDir).toContain("OUTSIDE_ROOT");
  } else {
    console.log("SKIPPED the symlink-escape assertions: creating a symlink needs a privilege the Windows runner does not grant; the Core's own suite covers the path policy.");
  }
});

test("Evidence: the task's receipts, checks and context accounting are shown read-only", async () => {
  await expect.poll(async () => (await page.evaluate((t) => window.modbit.effectReceipts(t), taskId)).receipts.length, { timeout: 60_000 }).toBeGreaterThanOrEqual(2);
  await page.getByTestId("tab-evidence").click();
  await expect(page.getByTestId("app-evidence")).toBeVisible();
  // Closing the worktree was a protected effect: it left an authorization receipt and its result receipt, and the chain verifies.
  await expect(page.getByTestId("evidence-receipt").first()).toBeVisible({ timeout: 30_000 });
  expect(await page.getByTestId("evidence-receipt").count()).toBeGreaterThanOrEqual(2);
  await expect(page.locator('[data-testid="evidence-receipt"][data-status="AUTHORIZED"]').first()).toBeVisible();
  await expect(page.locator('[data-testid="evidence-receipt"][data-status="SUCCESS"]').first()).toBeVisible();
  await expect(page.getByTestId("evidence-chain")).toHaveAttribute("data-valid", "true");
  const fromCore = await page.evaluate((t) => window.modbit.effectReceipts(t), taskId);
  expect(await page.getByTestId("evidence-receipt").count()).toBe(fromCore.receipts.length);
  // The categories are the Core's, and they sum to the total it reports.
  const inspector = await page.evaluate((t) => window.modbit.contextInspector(t), taskId);
  if (inspector.accounting) {
    expect(inspector.accounting.categories.reduce((n, c) => n + BigInt(c.tokens), 0n).toString()).toBe(inspector.accounting.totalTokens);
    await expect(page.getByTestId("evidence-category")).toHaveCount(inspector.accounting.categories.length);
  } else {
    await expect(page.getByTestId("evidence-no-accounting")).toBeVisible();
  }
  await expect(page.getByTestId("evidence-pack")).toBeVisible();
  await accessible(page, "evidence app", '[data-testid="region-apps"]');
});

test("the apps are keyboard reachable: tabs by arrows, Shift+Escape leaves the terminal, and the terminal keeps the keys it is typed into", async () => {
  await page.getByTestId("tab-evidence").focus();
  await page.keyboard.press("Home");
  await expect(page.getByTestId("tab-changes")).toHaveAttribute("aria-selected", "true");
  await page.keyboard.press("ArrowRight");
  await expect(page.getByTestId("tab-terminal")).toHaveAttribute("aria-selected", "true");
  await pickTerminal(handles[0]!);
  await screen().click();
  // Escape belongs to the process; the app's own "back to the fleet" handler must not take it.
  await page.keyboard.press("Escape");
  await expect(page.getByTestId("region-apps")).toBeVisible();
  await expect(terminalView()).toBeVisible();
  await page.keyboard.press("Shift+Escape");
  await expect(page.getByTestId("terminal-leave")).toBeFocused();
});

test("invalid arguments to the new calls are rejected by main", async () => {
  const bad = await page.evaluate(
    async ({ sessionId, taskId }) => {
      const ask = async (fn: () => Promise<unknown>) => fn().then(() => "ALLOWED", (e: Error) => e.message);
      return [
        await ask(() => window.modbit.terminalAttach("short", taskId, "abcd", "0")),
        await ask(() => window.modbit.terminalAttach(sessionId, "short", "abcd", "0")),
        await ask(() => window.modbit.terminalAttach(sessionId, taskId, "../../x", "0")),
        await ask(() => window.modbit.terminalAttach(sessionId, taskId, "abcd", "-1")),
        await ask(() => window.modbit.terminalAttach(sessionId, taskId, "abcd", "0", { windowBytes: 999_999_999 })),
        await ask(() => window.modbit.terminalAck("not an id!", "1")),
        await ask(() => window.modbit.terminalAck("deadbeef", "1")),
        await ask(() => window.modbit.terminalResize("deadbeef", 0, 80)),
        await ask(() => window.modbit.terminalResize("deadbeef", 24, 100000)),
        await ask(() => window.modbit.terminalWrite("deadbeef", "")),
        await ask(() => window.modbit.terminalWrite("deadbeef", "x".repeat(70_000))),
        await ask(() => window.modbit.terminalInput("deadbeef", "yes" as unknown as boolean)),
        await ask(() => window.modbit.terminalWrite("deadbeef", "x")),
        await ask(() => window.modbit.terminalResize("deadbeef", 24, 80)),
        await ask(() => window.modbit.terminalInput("deadbeef", true)),
        await ask(() => window.modbit.terminalDetach("deadbeef")),
        await ask(() => window.modbit.terminalKill("short", taskId, "abcd")),
        await ask(() => window.modbit.listWorkspaceDir("short", "")),
        await ask(() => window.modbit.effectReceipts("short")),
      ];
    },
    { sessionId, taskId },
  );
  for (const [i, m] of bad.entries()) expect(m, `call ${i}`).toMatch(/BAD_ARGUMENT|UNKNOWN_ATTACHMENT/);
  expect(bad.filter((m) => m.includes("UNKNOWN_ATTACHMENT")).length, "well-formed handles this window never opened are refused as unknown").toBe(5);
  expect(await page.evaluate(() => window.modbit.debugIpcRefusals()).then((r) => r.length), "these came from the app's own frame").toBe(0);
});
