/**
 * QUAL-PX-214 / PX-E2E-214 (DR-PX-2026-11-11-021 MWS-18): the isolated reviewer's findings one by one on the Review page, on the
 * real Electron build, a real Core with a real isolated review, and a scripted model standing in for the provider only.
 *
 * A signed model registry (activated in the data directory by the real CLI before the app starts) gives the plan a reviewer slot.
 * The task changes a login and adds a migration, so the Core's acceptance gate asks for independent review; the scripted reviewer
 * answers through `review.report` with three findings: a real one, one about a file that is not in the tree and one with no id.
 *  - Review shows the verdict in words and the three findings, each with its severity, place, claim and suggested action, equal to
 *    what the Core's review bundle answers; the one about the missing file is worded as not confirmed, never as found.
 *  - Walk through goes over the open findings one at a time.
 *  - Dismiss asks why, and the Core records who and why: the page and the Core's bundle agree.
 *  - Fix this one queues exactly one input, in the Core's input queue, that names that finding and no other; the candidate goes back
 *    to work; the bridge relays the Core's refusals (an unknown finding) and refuses malformed arguments before the Core.
 *  - Resuming the task runs the fix; while the next review runs the old report is marked as from an earlier version; the next
 *    review's brief lists the finding still open and not the dismissed one; its answer replaces the old report.
 * Fail conditions, each with a retained mutation (evidence/m10/PX-214/mutation-check-*.txt) the check is shown to catch: a finding
 * that failed the tree check worded as found, a fix input that names every finding, a dismissal without a reason.
 * With MODBIT_SHOTS_DIR set the same run writes the evidence screenshots.
 */
import { expect, test, type ElectronApplication, type Page } from "@playwright/test";
import { spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { createHash } from "node:crypto";
import { routedModel, sessionOf } from "./support/conversation-harness.ts";
import { agentRow, expectTaskState, runTask } from "./support/new-task.ts";
import { accessible, closeApp, coreBin, git, launch, setContentSize } from "./support/ui-harness.ts";

const SHOTS = process.env.MODBIT_SHOTS_DIR ?? "";
const GOAL = "Validate the px214 login and add the migration";
const LOGIN = "def login(u, p):\n    return u == 'a'\n";
const CLAIM_REAL = "`and p` only checks the password is non-empty";
const CLAIM_PHANTOM = "unused import of phantom helpers";
const CLAIM_MIGRATION = "the migration has no down step";
const REASON = "that migration is reversible by restoring the snapshot";

const cliBin = join(dirname(coreBin), process.platform === "win32" ? "modbit-cli.exe" : "modbit-cli");

function repo(): string {
  const dir = mkdtempSync(join(tmpdir(), "modbit-e2e-px214-repo-"));
  const files: [string, string][] = [
    ["src/auth/login.py", LOGIN],
    ["db/migrations/001_init.sql", "create table t (id int);\n"],
    ["tests/test_login.py", "def test_login():\n    assert True\n"],
    ["check.sh", "grep -q 'validated' src/auth/login.py\n"],
    [".modbit/verification.json", JSON.stringify({ commands: [{ id: "gate", argv: ["sh", "check.sh"] }] })],
  ];
  for (const [p, c] of files) {
    mkdirSync(dirname(join(dir, p)), { recursive: true });
    writeFileSync(join(dir, p), c);
  }
  git(dir, "init", "-q", "-b", "main");
  git(dir, "add", "-A");
  git(dir, "-c", "user.name=t", "-c", "user.email=t@e", "commit", "-q", "-m", "base");
  return dir;
}

/** The solver's script: a login edit and a migration (a change the gate wants reviewed), then, after the fix is queued and the task resumed, the revision. */
const loginHash = createHash("sha256").update(LOGIN).digest("hex");
const SOLVER: { call: { name: string; args: unknown } }[][] = [
  [{ call: { name: "plan.update", args: { outcome: "validate the login and add the migration", expected_files: ["src/auth/login.py", "db/migrations/002_add.sql"], verification: ["sh check.sh"] } } }],
  [{ call: { name: "fs.read", args: { path: "src/auth/login.py" } } }],
  [{ call: { name: "change.apply", args: { path: "src/auth/login.py", op: "replace", content: "def login(u, p):\n    # validated\n    return u == 'a' and p\n", expected_content_hash: loginHash } } }],
  [{ call: { name: "change.apply", args: { path: "db/migrations/002_add.sql", op: "create", content: "alter table t add column name text;\n" } } }],
  [{ call: { name: "plan.update", args: { outcome: "validate the login and add the migration", expected_files: ["src/auth/login.py", "db/migrations/002_add.sql"], verification: ["sh check.sh"], reason: "the migration is authored by hand: there is no generator in this repository" } } }],
  [{ call: { name: "test.run", args: { argv: ["sh", "check.sh"], inherit_env: true } } }],
  [{ call: { name: "task.complete", args: { summary: "validated login, migration added", self_review: { findings: [] }, verification: ["sh check.sh"] } } }],
  [{ call: { name: "change.apply", args: { path: "src/auth/login.py", op: "replace", content: "import hashlib\n# FIXED-HASH\ndef login(u, p):\n    # validated\n    return u == 'a' and hashlib.sha256(p.encode()).hexdigest() == STORED\n" } } }],
  [{ call: { name: "test.run", args: { argv: ["sh", "check.sh"], inherit_env: true } } }],
  [{ call: { name: "task.complete", args: { summary: "the password is compared against a stored hash", self_review: { findings: [] }, verification: ["sh check.sh"] } } }],
];

const FIRST_REPORT = {
  verdict: "BLOCK",
  confidence: 77,
  summary: "the password is only truthiness-checked",
  findings: [
    { id: "f1", severity: "HIGH", category: "security", path: "src/auth/login.py", line: 3, claim: CLAIM_REAL, evidence_refs: ["src/auth/login.py:3"], suggested_action: "compare against a stored hash" },
    { id: "f2", severity: "LOW", category: "style", path: "src/auth/phantom.py", line: 1, claim: CLAIM_PHANTOM, evidence_refs: [], suggested_action: "remove the import" },
    { severity: "MEDIUM", category: "regression", path: "db/migrations/002_add.sql", line: 1, claim: CLAIM_MIGRATION, evidence_refs: [], suggested_action: "add the reverse migration" },
  ],
  unresolved_questions: ["where is the stored hash kept?"],
};
const SECOND_REPORT = { verdict: "PASS", confidence: 90, summary: "the password is compared against a stored hash", findings: [], unresolved_questions: [] };

/** A signed registry that gives the plan a reviewer slot, activated in the data directory by the real CLI (the desktop has no bridge for it). */
function activateRegistry(dataDir: string): string {
  const env = { ...process.env, MODBIT_CORE_BIN: coreBin, OPENAI_API_KEY: "", ANTHROPIC_API_KEY: "" } as Record<string, string>;
  const cli = (args: string[], extra: Record<string, string> = {}) => {
    const r = spawnSync(cliBin, ["--data-dir", dataDir, ...args], { encoding: "utf8", env: { ...env, ...extra } });
    if (r.status !== 0) throw new Error(`modbit-cli ${args.slice(0, 2).join(" ")}: ${r.stdout}${r.stderr}`);
    return r.stdout;
  };
  const work = mkdtempSync(join(tmpdir(), "modbit-e2e-px214-registry-"));
  const keyFile = join(work, "ops.key");
  const trusted = /MODBIT_REGISTRY_KEYS=(\S+)/.exec(cli(["registry", "keygen", "--out", keyFile, "--key-id", "ops"]))?.[1];
  if (!trusted) throw new Error("the CLI printed no trusted key");
  const now = Date.now();
  const entry = (model: string, roles: string[], input: number, output: number) => ({
    endpoint: "openai", provider: "openai", family: "gpt-5", model, roles, input_modalities: ["text"], context_tokens: 400_000, max_output_tokens: 64_000,
    tools: true, vision: false, reasoning: true, structured_output: true,
    economics: { input_per_mtok_minor: input, output_per_mtok_minor: output, currency: "USD", scale: 2 },
    latency: { p50_ms: 900, p95_ms: 4_200 }, governance: { data_residency: "us", retains_prompts: false, allowed_profiles: [] }, revoked: false, fallbacks: [],
  });
  const doc = {
    promotion: null, schema_version: 1, registry_generation: "registry-px214", stats_version: "stats-1", issued_at_ms: now - 60_000, expires_at_ms: now + 86_400_000,
    quality_floors: [{ mode: "auto", min_quality: 0.72, max_cost_minor: 5_000, currency: "USD", scale: 2 }],
    entries: [entry("gpt-5-mini", ["solver"], 25, 200), entry("gpt-5", ["solver", "reviewer"], 125, 1_000)],
  };
  const docFile = join(work, "doc.json");
  const signed = join(work, "signed.json");
  writeFileSync(docFile, JSON.stringify(doc));
  cli(["registry", "sign", "--key", keyFile, "--key-id", "ops", "--doc", docFile, "--out", signed], { MODBIT_REGISTRY_KEYS: trusted });
  cli(["registry", "activate", signed], { MODBIT_REGISTRY_KEYS: trusted });
  endCliCore(dataDir);
  return trusted;
}

/**
 * The CLI starts a Core of its own on the data directory and leaves it for 30 s after its last command (`--idle-exit-secs`). The app's
 * Core must be the only one on the directory, so this one is ended (SIGTERM, as any Core is) and awaited before the app starts. The
 * registry it activated is in the store, and the app's Core restores it.
 */
function endCliCore(dataDir: string): void {
  spawnSync("pkill", ["-TERM", "-f", `--data-dir ${dataDir}`]);
  const deadline = Date.now() + 30_000;
  while (Date.now() < deadline) {
    if (spawnSync("pgrep", ["-f", `--data-dir ${dataDir}`]).status !== 0) return;
    spawnSync("sleep", ["0.2"]);
  }
  throw new Error("the CLI's Core did not end");
}

// ---------------------------------------------------------------- the bridge's log (a tap in main, as PX-208)

async function tapBridge(app: ElectronApplication): Promise<void> {
  await app.evaluate(({ ipcMain }) => {
    const g = globalThis as unknown as { __bridgeLog?: unknown[] };
    if (g.__bridgeLog) return;
    const log: { channel: string; args: unknown[] }[] = [];
    g.__bridgeLog = log;
    const handlers = (ipcMain as unknown as { _invokeHandlers: Map<string, (...a: unknown[]) => unknown> })._invokeHandlers;
    for (const [channel, fn] of [...handlers.entries()])
      handlers.set(channel, async (event: unknown, ...args: unknown[]) => {
        log.push({ channel, args });
        return fn(event, ...args);
      });
  });
}
const callsOn = (app: ElectronApplication, channel: string) => app.evaluate((_e, c) => ((globalThis as unknown as { __bridgeLog?: { channel: string; args: unknown[] }[] }).__bridgeLog ?? []).filter((x) => x.channel === c).map((x) => x.args), channel);

// ---------------------------------------------------------------- checks

/** Raw identifiers and Core codes in a surface's visible text (empty when it reads in plain words). */
async function rawWords(page: Page, testId: string): Promise<string[]> {
  const text = await page.getByTestId(testId).innerText();
  const found = text.match(/\b[A-Z][A-Z0-9]+(?:_[A-Z0-9]+)+\b|\bPX-\d+\b|\b(?:ReadyForReview|NeedsAttention|UNREADABLE|CURRENT|STALE|BLOCK|REVISE|UNCERTAIN)\b|\w\(s\)|ws-rev-\d|[{[]"\w+":/g) ?? [];
  return [...new Set(found)].map((w) => `${testId} shows “${w}”`);
}

/** What shows a finding that failed the Core's tree check as found (empty when none does): the row's flag and its words. */
async function unverifiedShownAsFound(page: Page): Promise<string[]> {
  return page.getByTestId("reviewer-finding").evaluateAll((els) => {
    const bad: string[] = [];
    for (const el of els) {
      const says = el.querySelector("[data-testid='reviewer-verified']")?.textContent ?? "";
      const id = el.getAttribute("data-finding-id");
      if (el.getAttribute("data-verified") === "false" && /found this file/i.test(says)) bad.push(`${id} did not resolve against the tree and is worded as found`);
    }
    return bad;
  });
}

/** The inputs queued for a task by a person's fix: what each names (empty list when none names more than one finding). */
function fixInputsNamingMoreThanOne(items: { text: string; provenance: string }[]): string[] {
  const claims = [CLAIM_REAL, CLAIM_PHANTOM, CLAIM_MIGRATION];
  return items.filter((i) => i.provenance === "review_finding").filter((i) => claims.filter((c) => i.text.includes(c)).length !== 1).map((i) => `an input names ${claims.filter((c) => i.text.includes(c)).length} findings`);
}

async function toReviewPage(page: Page, taskId: string): Promise<void> {
  if ((await page.getByTestId("review").count()) > 0) await page.getByTestId("review-close").click();
  if ((await page.getByTestId("review-page").count()) === 0) await page.getByTestId("open-review").click();
  await expect(page.getByTestId("review-page")).toBeVisible();
  await page.locator(`[data-testid="review-page-row"][data-task-id="${taskId}"]`).click();
  await expect(page.getByTestId("review-page-detail")).toHaveAttribute("data-task-id", taskId);
}

/**
 * Review is showing the reviewer's report in `state`. The app also opens the task's own review by itself when the task reaches review,
 * at a moment the test does not choose; Review is shown again if that took the window.
 */
async function reviewShowing(page: Page, taskId: string, state: string): Promise<void> {
  await expect
    .poll(
      async () => {
        if ((await page.getByTestId("review").count()) > 0 || (await page.getByTestId("review-page").count()) === 0) await toReviewPage(page, taskId);
        return page.getByTestId("reviewer").getAttribute("data-state", { timeout: 2_000 }).catch(() => "absent");
      },
      { timeout: 60_000, message: `Review shows the ${state} report` },
    )
    .toBe(state);
}

async function setTheme(page: Page, theme: "light" | "dark"): Promise<void> {
  await page.evaluate((t) => document.documentElement.setAttribute("data-theme", t), theme);
  await page.evaluate(() => new Promise<void>((d) => requestAnimationFrame(() => requestAnimationFrame(() => d()))));
}

/** Screenshots of the running app in both themes (the theme is an attribute: the page and its open controls stay as they are). */
async function shots(page: Page, name: string, ensure?: () => Promise<void>): Promise<void> {
  if (!SHOTS) return;
  mkdirSync(SHOTS, { recursive: true });
  for (const theme of ["light", "dark"] as const) {
    await setTheme(page, theme);
    await ensure?.();
    await page.waitForTimeout(400);
    await page.screenshot({ path: join(SHOTS, `${name}-${theme}.png`) });
  }
  await setTheme(page, "light");
}

// ---------------------------------------------------------------- the run

test("QUAL-PX-214: the reviewer's three findings on Review, walked through, one set aside with a reason, one sent to the agent, the next review re-evaluating what is open", async () => {
  test.skip(process.platform === "win32", "the isolated review environment needs a sandbox this host does not offer");
  test.setTimeout(480_000);
  const root = repo();
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-px214-"));
  const keys = activateRegistry(dataDir);

  // The stand-in model. The reviewer (the request carries the brief) reads a file, then reports; a later review (its brief carries the
  // earlier open findings) waits at the gate until the test releases it, so the stale report can be seen, then passes.
  const bodies: string[] = [];
  let release: () => void = () => {};
  const gate = new Promise<void>((r) => (release = r));
  const model = await routedModel(async (_goal, results, body) => {
    if (body.includes("[REVIEW BRIEF]")) {
      bodies.push(body);
      const later = body.includes("Earlier findings still open");
      if (results === 0) {
        if (later) await gate;
        return [{ call: { name: "fs.read", args: { path: "src/auth/login.py" } } }];
      }
      return [{ call: { name: "review.report", args: later ? SECOND_REPORT : FIRST_REPORT } }];
    }
    bodies.push(body);
    return SOLVER[results] ?? ["Done."];
  });
  const { app, page } = await launch(dataDir, { env: { MODBIT_OPENAI_BASE_URL: model.url, MODBIT_REGISTRY_KEYS: keys } });
  try {
    await setContentSize(app, page, 1280, 800);
    await tapBridge(app);

    // ---- The task reaches review; the reviewer has reported on it.
    const task = await runTask(page, GOAL, root);
    const sessionId = await sessionOf(page);
    await expectTaskState(page, task, "ReadyForReview", 120_000);
    await expect
      .poll(async () => (await page.evaluate((t) => window.modbit.reviewBundle(t), task)).reviewer.state, { timeout: 120_000, message: "the reviewer reported on the candidate" })
      .toBe("CURRENT");
    const core = await page.evaluate((t) => window.modbit.reviewBundle(t), task);
    expect(core.reviewer.verdict).toBe("BLOCK");
    expect(core.reviewer.findings.map((f) => f.findingId)).toEqual(["f1", "f2", "F3"]);
    expect(core.reviewer.findings.map((f) => f.verified)).toEqual([true, false, true]);
    const digest = core.reviewer.resultDigest;
    expect(digest).toMatch(/^[0-9a-f]{64}$/);
    if ((await page.getByTestId("review").count()) > 0) await page.getByTestId("review-close").click();

    // ---- Review lists each finding, as the Core's bundle has it.
    await toReviewPage(page, task);
    const reviewer = page.getByTestId("reviewer");
    await expect(reviewer).toHaveAttribute("data-state", "CURRENT");
    await expect(reviewer).toHaveAttribute("data-verdict", "BLOCK");
    await expect(page.getByTestId("reviewer-headline")).toHaveText("The reviewer says this change should not be accepted as it is.");
    await expect(page.getByTestId("reviewer-detail")).toContainText("the password is only truthiness-checked");
    const rows = page.getByTestId("reviewer-finding");
    await expect(rows).toHaveCount(3);
    expect(await rows.evaluateAll((els) => els.map((e) => [e.getAttribute("data-finding-id"), e.getAttribute("data-severity"), e.getAttribute("data-verified")]))).toEqual([
      ["f1", "HIGH", "true"],
      ["f2", "LOW", "false"],
      ["F3", "MEDIUM", "true"],
    ]);
    // Each row carries the Core's claim, suggested action and place (not the page's own).
    for (const [i, f] of core.reviewer.findings.entries()) {
      const row = rows.nth(i);
      await expect(row.getByTestId("reviewer-claim")).toHaveText(f.claim);
      await expect(row.getByTestId("reviewer-suggested")).toHaveText(`Suggested: ${f.suggestedAction}`);
      await expect(row.locator(".rp-path")).toHaveText(`${f.path}, line ${f.line}`);
    }
    // The finding about a file that is not in the tree is worded as not confirmed, with the Core's reason; the others as found.
    await expect(rows.nth(1).getByTestId("reviewer-verified")).toContainText("The Core could not confirm this");
    await expect(rows.nth(1).getByTestId("reviewer-verified")).toContainText("src/auth/phantom.py");
    await expect(rows.nth(0).getByTestId("reviewer-verified")).toHaveText("The Core found this file in the code.");
    expect(await unverifiedShownAsFound(page)).toEqual([]);
    // Retained mutation: a finding that failed the check, worded as found, is caught (and the text put back).
    const original = await rows.nth(1).getByTestId("reviewer-verified").innerText();
    await page.evaluate(() => {
      document.querySelectorAll("[data-testid='reviewer-finding']")[1]!.querySelector("[data-testid='reviewer-verified']")!.textContent = "The Core found this file in the code.";
    });
    expect(await unverifiedShownAsFound(page)).toEqual(["f2 did not resolve against the tree and is worded as found"]);
    await page.evaluate((t) => {
      document.querySelectorAll("[data-testid='reviewer-finding']")[1]!.querySelector("[data-testid='reviewer-verified']")!.textContent = t;
    }, original);
    expect(await unverifiedShownAsFound(page)).toEqual([]);
    await expect(page.getByTestId("reviewer-questions")).toContainText("where is the stored hash kept?");
    expect(await rawWords(page, "review-page"), "plain words on the Review page").toEqual([]);
    await accessible(page, "the Review page with the reviewer's findings");
    await shots(page, "review-reviewer-findings");

    // ---- Walk through: one open finding at a time.
    await page.getByTestId("reviewer-walk").click();
    await expect(page.getByTestId("reviewer-finding")).toHaveCount(1);
    await expect(page.getByTestId("reviewer-step")).toHaveText("Finding 1 of 3");
    await expect(page.getByTestId("reviewer-finding")).toHaveAttribute("data-finding-id", "f1");
    await shots(page, "review-walk-through");
    await page.getByTestId("reviewer-next").click();
    await expect(page.getByTestId("reviewer-finding")).toHaveAttribute("data-finding-id", "f2");
    await page.getByTestId("reviewer-next").click();
    await expect(page.getByTestId("reviewer-step")).toHaveText("Finding 3 of 3");
    await expect(page.getByTestId("reviewer-next")).toBeDisabled();
    await page.getByTestId("reviewer-prev").click();
    await expect(page.getByTestId("reviewer-finding")).toHaveAttribute("data-finding-id", "f2");
    await page.getByTestId("reviewer-walk-done").click();
    await expect(page.getByTestId("reviewer-finding")).toHaveCount(3);

    // ---- The bridge: malformed arguments never reach the Core; the Core's refusals come back as its own.
    const refused = (fn: () => Promise<unknown>) => fn().then(() => "accepted", (e: Error) => e.message);
    expect(await refused(() => page.evaluate(([s, t]) => window.modbit.fixReviewFinding(s!, t!, "../f1", "c".repeat(64)), [sessionId, task]))).toContain("BAD_ARGUMENT");
    expect(await refused(() => page.evaluate(([s, t]) => window.modbit.dismissReviewFinding(s!, t!, "f1", "c".repeat(64), "  "), [sessionId, task]))).toContain("BAD_ARGUMENT");
    expect(await refused(() => page.evaluate(([s, t, d]) => window.modbit.fixReviewFinding(s!, t!, "nope", d!), [sessionId, task, digest]))).toContain("UNKNOWN_FINDING");
    expect(await refused(() => page.evaluate(([s, t]) => window.modbit.fixReviewFinding(s!, t!, "f1", "0".repeat(64)), [sessionId, task]))).toContain("RESULT_CHANGED");
    expect((await page.evaluate((t) => window.modbit.composerQueue(t), task)).items.filter((i) => i.provenance === "review_finding"), "nothing was queued by a refusal").toEqual([]);

    // ---- Dismiss one: the page asks why; the Core records who and why.
    const third = () => page.locator('[data-testid="reviewer-finding"][data-finding-id="F3"]');
    await third().getByTestId("reviewer-dismiss").click();
    await expect(third().getByTestId("reviewer-dismiss-confirm")).toBeDisabled();
    await third().getByTestId("reviewer-dismiss-reason").fill(REASON);
    await shots(page, "review-dismiss-reason");
    await third().getByTestId("reviewer-dismiss-confirm").click();
    await expect(third()).toHaveAttribute("data-dismissed", "true");
    await expect(third().getByTestId("reviewer-handled")).toHaveText(`Set aside by you: ${REASON}`);
    await expect(third().getByTestId("reviewer-fix")).toHaveCount(0);
    await expect(page.getByTestId("reviewer-count")).toHaveText("2 of 3 still open.");
    const dismissCalls = await callsOn(app, "review:dismissFinding");
    expect(dismissCalls.filter((a) => a[2] === "F3")).toEqual([[sessionId, task, "F3", digest, REASON]]);
    const afterDismiss = await page.evaluate((t) => window.modbit.reviewBundle(t), task);
    const f3 = afterDismiss.reviewer.findings[2]!;
    expect([f3.dismissed, f3.dismissedReason, f3.dismissedBy.startsWith("user:")]).toEqual([true, REASON, true]);
    expect(afterDismiss.reviewer.findings.filter((f) => f.dismissed).length).toBe(1);
    // Walk through skips what was set aside.
    await page.getByTestId("reviewer-walk").click();
    await expect(page.getByTestId("reviewer-count")).toHaveText("2 of 3 still open.");
    await page.getByTestId("reviewer-next").click();
    await expect(page.getByTestId("reviewer-step")).toHaveText("Finding 2 of 2");
    await expect(page.getByTestId("reviewer-next")).toBeDisabled();
    await page.getByTestId("reviewer-walk-done").click();
    // A dismissed finding cannot be fixed: the Core refuses.
    expect(await refused(() => page.evaluate(([s, t, d]) => window.modbit.fixReviewFinding(s!, t!, "F3", d!), [sessionId, task, digest]))).toContain("FINDING_DISMISSED");

    // ---- Fix this one: one input, naming that finding only; the candidate goes back to work.
    const first = () => page.locator('[data-testid="reviewer-finding"][data-finding-id="f1"]');
    await first().getByTestId("reviewer-fix").click();
    await expect(page.getByTestId("reviewer-note")).toHaveText("Asked the agent to fix this one. It starts on it when you resume the task.");
    await expect(first().getByTestId("reviewer-handled")).toHaveText("You asked the agent to fix this one.");
    const fixCalls = await callsOn(app, "review:fixFinding");
    // (the earlier refused calls are in the log too: the one with the digest the page showed is this click)
    expect(fixCalls.filter((a) => a[2] === "f1" && a[3] === digest)).toEqual([[sessionId, task, "f1", digest]]);
    const queue = await page.evaluate((t) => window.modbit.composerQueue(t), task);
    const inputs = queue.items.filter((i) => i.provenance === "review_finding");
    expect(inputs.length, "exactly one input").toBe(1);
    expect(inputs[0]!.text).toContain("Finding f1 [HIGH] security at src/auth/login.py:3");
    expect(inputs[0]!.text).toContain(CLAIM_REAL);
    for (const other of [CLAIM_PHANTOM, CLAIM_MIGRATION, REASON]) expect(inputs[0]!.text).not.toContain(other);
    expect(fixInputsNamingMoreThanOne(inputs)).toEqual([]);
    // Retained mutation: an input that names two findings is caught.
    expect(fixInputsNamingMoreThanOne([{ provenance: "review_finding", text: `${CLAIM_REAL} ${CLAIM_MIGRATION}` }])).toEqual(["an input names 2 findings"]);
    await expectTaskState(page, task, /Waiting/, 30_000);
    await shots(page, "review-fix-queued");

    // ---- Resume: the fix is made and the candidate reviewed again; meanwhile the old report says it is from an earlier version.
    await page.evaluate(([s, t]) => window.modbit.startTask(s!, t!), [sessionId, task]);
    await expect
      .poll(async () => (await page.evaluate((t) => window.modbit.reviewBundle(t), task)).reviewer.state, { timeout: 120_000, message: "the Core marked the old report as from an earlier version" })
      .toBe("STALE");
    // The task reached review again while its next review runs, so the app may be showing the task's own review: back to Review.
    await reviewShowing(page, task, "STALE");
    await expect(page.getByTestId("reviewer-stale")).toContainText("from an earlier version of the change");
    await expect(page.getByTestId("reviewer-finding")).toHaveCount(3);
    await shots(page, "review-stale-report", () => reviewShowing(page, task, "STALE"));
    // The next review's brief lists what is open (f1, f2) and not what was set aside.
    await expect.poll(() => bodies.filter((b) => b.includes("Earlier findings still open")).length, { timeout: 60_000 }).toBeGreaterThan(0);
    const brief = bodies.find((b) => b.includes("Earlier findings still open"))!;
    expect(brief).toContain("[f1] HIGH security");
    expect(brief).toContain("[f2] LOW style");
    expect(brief).not.toContain(CLAIM_MIGRATION);
    expect(brief).not.toContain("[F3]");
    expect(bodies.find((b) => b.includes("[REVIEW BRIEF]"))!, "the first review had no earlier findings").not.toContain("Earlier findings still open");
    expect(bodies.some((b) => !b.includes("[REVIEW BRIEF]") && b.includes("Finding f1 [HIGH]")), "the agent was given the one finding").toBe(true);

    // ---- The next review answers; its report replaces the old one.
    release();
    await expect
      .poll(async () => (await page.evaluate((t) => window.modbit.reviewBundle(t), task)).reviewer.verdict, { timeout: 120_000, message: "the next review answered" })
      .toBe("PASS");
    // The task is ready for review again, so the app shows its own review; Review is one click away.
    await expectTaskState(page, task, "ReadyForReview", 60_000);
    await reviewShowing(page, task, "CURRENT");
    await expect(page.getByTestId("reviewer")).toHaveAttribute("data-verdict", "PASS");
    await expect(page.getByTestId("reviewer-headline")).toHaveText("The reviewer found nothing to change.");
    await expect(page.getByTestId("reviewer-finding")).toHaveCount(0);
    await expect(page.getByTestId("reviewer-count")).toHaveText("No findings.");
    expect((await page.evaluate((t) => window.modbit.reviewBundle(t), task)).reviewer.resultDigest).not.toBe(digest);
    await shots(page, "review-second-report", () => reviewShowing(page, task, "CURRENT"));
    expect(await rawWords(page, "review-page"), "plain words on the Review page").toEqual([]);
    expect(await agentRow(page, task).getAttribute("data-state")).toBeTruthy();
  } finally {
    release();
    await closeApp(app);
  }
});
