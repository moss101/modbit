/**
 * PX-144 / PX-145 (QUAL-PX-144, QUAL-PX-145 desktop half; DR-PX-2026-10-25-017):
 * the autonomous coding path through the real Electron renderer, the real
 * preload bridge, a real Core, a real `tsc` and a real `npm run build`. Only
 * the provider is scripted, and it never calls `verify.run` or
 * `repair.attempt`: the Core verifies the model's edit itself and the task
 * completes with no approval card. This proves the path, not a model; a live
 * model's run is a separate leg (docs/82).
 */
import { expect, test } from "@playwright/test";
import { chmodSync, existsSync, mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { execFileSync } from "node:child_process";
import { accessible, appDir, closeApp, git, launch, setContentSize } from "./support/ui-harness.ts";
import { createTask, openConversation, routedModel, sessionOf } from "./support/conversation-harness.ts";

const REPO_TSC = resolve(appDir, "..", "..", "node_modules", ".bin", "tsc");

/** A committed TypeScript project whose `npm run build` fails on one type error, through the checkout's own `tsc`. */
function brokenProject(): string {
  const root = mkdtempSync(join(tmpdir(), "modbit-auto-ts-"));
  const put = (rel: string, text: string) => {
    mkdirSync(join(root, rel, ".."), { recursive: true });
    writeFileSync(join(root, rel), text);
  };
  put("package.json", JSON.stringify({ name: "app", version: "1.0.0", scripts: { build: "tsc -p ." } }));
  put("package-lock.json", "{}");
  put("tsconfig.json", JSON.stringify({ compilerOptions: { strict: true, noEmit: true, target: "es2020", module: "commonjs", types: [] }, include: ["src"] }));
  put(".gitignore", "node_modules/\n");
  put(".modbit/verification.json", JSON.stringify({ commands: [] }));
  put("src/index.ts", 'export const total: number = "oops";\n');
  git(root, "init", "-q", "-b", "main");
  git(root, "add", "-A");
  git(root, "-c", "user.name=t", "-c", "user.email=t@e", "commit", "-q", "-m", "base");
  mkdirSync(join(root, "node_modules/.bin"), { recursive: true });
  writeFileSync(join(root, "node_modules/.bin/tsc"), `#!/bin/sh\nexec "${REPO_TSC}" "$@"\n`);
  chmodSync(join(root, "node_modules/.bin/tsc"), 0o755);
  return root;
}

const buildPasses = (root: string): boolean => {
  try {
    execFileSync("npm", ["run", "build", "--silent"], { cwd: root, stdio: "ignore" });
    return true;
  } catch {
    return false;
  }
};

test("PX-144/145: Autonomous is chosen in the run-mode dialog; a broken TypeScript build is verified by the Core after the edit and the task completes with no approval", async () => {
  test.skip(process.platform === "win32" || !existsSync(REPO_TSC), "needs a POSIX shell and this checkout's installed TypeScript");
  test.setTimeout(300_000);
  const root = brokenProject();
  expect(buildPasses(root), "the fixture really is broken").toBe(false);
  const model = await routedModel((_goal, results, body) => {
    if (body.includes("every mandatory check passed")) return [{ call: { name: "task.complete", args: { summary: "the build passes", self_review: { findings: [] } } } }];
    if (results === 0) return ["Planning.", { call: { name: "plan.update", args: { outcome: "the build passes", expected_files: ["src/index.ts"], verification: ["npm run build"] } } }];
    if (results === 1) return [{ call: { name: "fs.read", args: { path: "src/index.ts" } } }];
    if (results === 2) return ["Fixing the type.", { call: { name: "change.apply", args: { path: "src/index.ts", op: "replace", content: "export const total: number = 42;\n" } } }];
    return ["Waiting for the Core's verification."];
  });
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-autonomy-"));
  const { app, page } = await launch(dataDir, { env: { MODBIT_OPENAI_BASE_URL: model.url } });
  try {
    await setContentSize(app, page, 1280, 800);
    const goal = "The build is broken: fix the TypeScript compile error";
    const taskId = await createTask(page, goal, root);
    await openConversation(page, taskId);
    // A task starts Supervised: it asks for what no rule covers.
    await expect(page.getByTestId("runmode-current")).toHaveText("Supervised");
    // Autonomous is chosen behind the acknowledgement, which names what still asks.
    await page.getByTestId("runmode-button").click();
    await page.getByTestId("runmode-radio-AUTONOMOUS").click();
    await expect(page.getByTestId("runmode-warning-autonomous")).toContainText("still ask");
    await expect(page.getByTestId("runmode-warning-confirm")).toBeDisabled();
    await accessible(page, "the Autonomous warning");
    await page.getByTestId("runmode-understood").check();
    await page.getByTestId("runmode-warning-confirm").click();
    await expect(page.getByTestId("runmode-current")).toHaveText("Autonomous");
    await page.getByTestId("runmode-close").click();
    // Start it through the bridge, as the Fleet card does.
    const sessionId = await sessionOf(page);
    await page.evaluate(([s, t]) => window.modbit.startTask(s!, t!), [sessionId, taskId]);
    await expect.poll(() => page.evaluate((t) => window.modbit.taskStatus(t).then((s) => s.state), taskId), { timeout: 240_000 }).toBe("ReadyForReview");
    expect(buildPasses(root), "the build passes after the run").toBe(true);
    // Nobody was asked, and the Core's verification is on the task's record.
    const approvals = await page.evaluate((s) => window.modbit.pendingApprovals(s!).then((l) => l.length), sessionId);
    expect(approvals).toBe(0);
    await expect(page.getByTestId("approval-card")).toHaveCount(0);
    const receipts = await page.evaluate((t) => window.modbit.effectReceipts(t), taskId);
    expect(receipts.receipts.some((r) => r.policyDecision.startsWith("approval:"))).toBe(false);
    expect(model.requests()).toBeGreaterThanOrEqual(4);
    expect((await page.evaluate((t) => window.modbit.runMode(t), taskId)).mode).toBe("AUTONOMOUS");
  } finally {
    await closeApp(app);
    model.server.close();
  }
});
