/**
 * REQ-PX-086 / QUAL-PX-086: the model-free gallery states of the Automations
 * surface in the real Electron app with no Core behind them, under the
 * axe-core suite in light, dark and high contrast: the empty list, the list
 * with every state and every attention kind, the editor with the Core's
 * issues, the approval of a write-capable definition, repository definitions
 * (not enabled, and changed after approval), the history with every typed
 * reason, a finished dry run and the kill confirmation. A QA aid: it proves
 * these states are accessible, never that a behaviour works
 * (automations.spec.ts proves those against a real Core).
 */
import { execFileSync } from "node:child_process";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { expect, test } from "@playwright/test";
import { accessible, appDir, closeApp, launch } from "./support/ui-harness.ts";

const OUT = "dist/gallery-build";
const THEMES = ["light", "dark", "high-contrast"] as const;

test.beforeAll(() => {
  execFileSync("node", ["build.mjs"], { cwd: appDir, env: { ...process.env, MODBIT_GALLERY: "1", MODBIT_OUTDIR: OUT }, stdio: "pipe" });
});

test("PX-086: every automations state is on the gallery with its words, and accessible in every theme: the empty list, every definition state and attention kind, the editor's issues, the approval of a write-capable definition, repository definitions, every typed reason in the history, a dry run's report and the kill confirmation", async () => {
  test.setTimeout(240_000);
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-gallery-auto-"));
  const { app, page } = await launch(dataDir, { outDir: OUT, waitForCore: false, env: { MODBIT_CORE_BIN: join(dataDir, "no-such-core") } });
  try {
    await page.evaluate(() => {
      location.hash = "#gallery";
    });
    const gallery = page.getByTestId("gallery-automations");
    await expect(gallery).toBeVisible();
    const status = await page.evaluate(() => window.modbit.coreStatus());
    expect((status as { state: string }).state, "no Core is connected to this page").not.toBe("connected");

    await expect(page.getByTestId("gallery-automations-empty").getByTestId("automations-empty")).toContainText("No automations yet");

    // Every definition state, in words and by name; every attention kind with the action that clears it.
    const states = page.getByTestId("gallery-automations-states");
    await expect(states.getByTestId("automation-row")).toHaveCount(7);
    const seen = await states.getByTestId("automation-row").evaluateAll((els) => [...new Set(els.map((e) => e.getAttribute("data-state")))]);
    expect(seen.sort()).toEqual(["AUTO_DISABLED", "DISABLED", "ENABLED", "NEEDS_APPROVAL", "PAUSED"]);
    await expect(states.getByTestId("automation-state").first()).toContainText("Enabled");
    const kinds = await states.getByTestId("automation-attention").evaluateAll((els) => els.map((e) => `${e.getAttribute("data-kind")}:${e.querySelector("[data-action]")?.getAttribute("data-action")}`));
    expect(kinds.sort()).toEqual(["APPROVAL_EXPIRED:acknowledge", "AUTO_DISABLED:review", "PARKED_APPROVAL:approval", "RUN_FAILED:acknowledge", "SKIPPED_BUDGET:acknowledge", "SOURCE_CHANGED:review"]);
    await expect(states.locator('[data-testid="automation-triggers"] li[data-kind="schedule"]').first()).toContainText("UTC");

    // The editor shows every issue with its path and code, and cannot save.
    const editor = page.getByTestId("gallery-automations-editor");
    await expect(editor.getByTestId("validation-status")).toHaveAttribute("data-state", "invalid");
    const codes = await editor.getByTestId("validation-issue").evaluateAll((els) => els.map((e) => `${e.getAttribute("data-code")} ${e.getAttribute("data-path")}`));
    expect(codes).toEqual(["BELOW_FLOOR /triggers/0/cron", "BAD_REGEX /triggers/1/filters/title_regex", "UNKNOWN_CAPABILITY /profile/capabilities/0"]);
    await expect(editor.getByTestId("automation-save")).toBeDisabled();

    // Repository definitions: data until approved; a changed file says so.
    await expect(page.getByTestId("gallery-automations-repo").getByTestId("repository-note")).toHaveAttribute("data-enabled", "false");
    await expect(page.getByTestId("gallery-automations-repo").getByTestId("detail-test")).toBeDisabled();
    await expect(page.getByTestId("gallery-automations-changed").getByTestId("content-changed")).toBeVisible();
    await expect(page.getByTestId("gallery-automations-changed").locator('[data-testid="automation-attention"][data-kind="SOURCE_CHANGED"]')).toBeVisible();

    // The history has a row for every typed reason the Core records, each in words.
    const history = page.getByTestId("gallery-automations-history");
    const reasons = await history.getByTestId("run-row").evaluateAll((els) => els.map((e) => e.getAttribute("data-reason")));
    for (const r of ["DUPLICATE", "PAUSED", "BUDGET", "CONCURRENCY", "MISSED", "APPROVAL_EXPIRED", "KILLED", "REPLACED", "BUDGET_EXHAUSTED", "FILTER", "TASK_FAILED"]) expect(reasons, r).toContain(r);
    await expect(history.locator('[data-testid="run-row"][data-reason="KILLED"]')).toContainText("kill switch");
    await expect(history.locator('[data-testid="run-row"][data-catch-up="true"]').getByTestId("run-catch-up")).toBeVisible();
    await expect(history.locator('[data-testid="run-row"][data-test="true"]').getByTestId("run-findings")).toHaveAttribute("data-count", "2");
    expect(await history.getByTestId("run-open-task").count(), "a run with a task links to it").toBeGreaterThan(0);

    for (const theme of THEMES) {
      await page.getByTestId(`theme-${theme}`).click();
      await expect(page.locator("html")).toHaveAttribute("data-theme", theme);
      await accessible(page, `the automations gallery in ${theme}`, '[data-testid="gallery-automations"]');

      await page.getByTestId("gallery-open-enable").click();
      await expect(page.getByTestId("enable-capabilities")).toContainText("fs.write");
      await expect(page.getByTestId("enable-paths")).toContainText("reports/**");
      await expect(page.getByTestId("enable-confirm")).toBeDisabled();
      await accessible(page, `the approval of a write-capable definition in ${theme}`);
      await page.getByTestId("enable-ack").check();
      await expect(page.getByTestId("enable-confirm")).toBeEnabled();
      await page.keyboard.press("Escape");
      await expect(page.getByTestId("enable-dialog")).toHaveCount(0);

      await page.getByTestId("gallery-open-test").click();
      await expect(page.getByTestId("would-have-asked-entry")).toHaveCount(2);
      await expect(page.getByTestId("test-denied-note")).toContainText("No protected effect was performed");
      await accessible(page, `a dry run's report in ${theme}`);
      await page.getByTestId("test-close").click();

      await page.getByTestId("gallery-open-kill").click();
      await expect(page.getByTestId("kill-body")).toContainText("cancels the runs in progress");
      await accessible(page, `the kill confirmation in ${theme}`);
      await page.getByTestId("kill-confirm").click();
      await expect(page.getByTestId("kill-report")).toHaveAttribute("data-cancelled", "1");
      await expect(page.getByTestId("kill-report")).toHaveAttribute("data-dropped", "2");
      await accessible(page, `the kill report in ${theme}`);
      await page.getByTestId("kill-close").click();
      await expect(page.getByTestId("kill-dialog")).toHaveCount(0);
    }
  } finally {
    await closeApp(app);
  }
});
