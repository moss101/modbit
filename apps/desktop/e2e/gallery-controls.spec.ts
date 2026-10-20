/**
 * REQ-PX-057 / 058 / 060 / 062: the model-free gallery states of the approval
 * stack, the run-mode chooser, the context ring and its trays and the
 * checkpoint controls, in the real Electron app with no Core behind them, under
 * the axe-core suite in light, dark and high contrast. A QA aid: it proves
 * these states are accessible, never that a behaviour works (approvals.spec.ts,
 * context.spec.ts and checkpoints.spec.ts prove the behaviour against a real Core).
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

test("PX-058/057/060/062: every new state is accessible in every theme: approval cards, the run-mode and warning dialogs, the ring and its trays, the restore, redo and Revert-or-Keep dialogs, the edit box and the Redo bar", async () => {
  test.setTimeout(240_000);
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-gallery-controls-"));
  const { app, page } = await launch(dataDir, { outDir: OUT, waitForCore: false, env: { MODBIT_CORE_BIN: join(dataDir, "no-such-core") } });
  try {
    await page.evaluate(() => {
      location.hash = "#gallery";
    });
    await expect(page.getByTestId("gallery-controls")).toBeVisible();
    // The ring has a state for every level, said in words as well as drawn.
    const levels = await page.getByTestId("gallery-rings").getByTestId("context-ring").evaluateAll((els) => els.map((e) => e.getAttribute("data-level")));
    expect(levels).toEqual(["unknown", "ok", "warn", "danger"]);
    // The breakdown sums to its total in the fixture, as it must from the Core.
    await expect(page.getByTestId("gallery-context-tray").getByTestId("context-tray")).toHaveAttribute("data-sums", "true");
    await expect(page.getByTestId("gallery-context-tray").getByTestId("context-category")).toHaveCount(9);
    // The card in its three states.
    await expect(page.getByTestId("gallery-approval-plain").getByTestId("approval-count")).toContainText("3 pending");
    await expect(page.getByTestId("gallery-approval-confirm").getByTestId("approval-confirm")).toContainText("cannot be undone");
    await expect(page.getByTestId("gallery-approval-foreign").getByTestId("approval-foreign")).toBeVisible();
    await expect(page.getByTestId("gallery-approval-foreign").getByTestId("approval-note")).toContainText("no longer the one this card showed");
    await expect(page.getByTestId("gallery-checkpoint-rows").getByTestId("turn-checkpoint")).toBeVisible();

    for (const theme of THEMES) {
      await page.getByTestId(`theme-${theme}`).click();
      await expect(page.locator("html")).toHaveAttribute("data-theme", theme);
      await accessible(page, `the controls gallery in ${theme}`, '[data-testid="gallery-controls"]');
      await accessible(page, `the whole gallery in ${theme}`);

      // The dialogs, one at a time.
      await page.getByTestId("gallery-open-runmode").click();
      await expect(page.getByTestId("runmode-dialog")).toBeVisible();
      await accessible(page, `the run-mode dialog in ${theme}`);
      await page.getByTestId("runmode-radio-RUN_EVERYTHING").click();
      await expect(page.getByTestId("runmode-warning")).toBeVisible();
      await expect(page.getByTestId("runmode-warning-confirm")).toBeDisabled();
      await accessible(page, `the widest-preset warning in ${theme}`);
      await page.getByTestId("runmode-warning-cancel").click();
      await page.getByTestId("rule-pattern-input").fill("cargo test");
      await page.getByTestId("rule-add").click();
      await expect(page.getByTestId("runmode-error")).toContainText("The Core did not make the rule");
      await accessible(page, `the run-mode dialog with a refusal in ${theme}`);
      await page.getByTestId("runmode-close").click();

      await page.getByTestId("gallery-open-restore").click();
      await expect(page.getByTestId("restore-edited")).toBeVisible();
      await expect(page.getByTestId("restore-continue")).toBeDisabled();
      await accessible(page, `the restore dialog in ${theme}`);
      await page.getByTestId("restore-keep").check();
      await expect(page.getByTestId("restore-continue")).toBeEnabled();
      await page.getByTestId("restore-cancel").click();

      await page.getByTestId("gallery-open-redo").click();
      await expect(page.getByTestId("restore-body")).toHaveAttribute("data-redo", "true");
      await accessible(page, `the redo dialog in ${theme}`);
      await page.keyboard.press("Escape");
      await expect(page.getByTestId("restore-dialog")).toHaveCount(0);

      await page.getByTestId("gallery-open-edit-files").click();
      await expect(page.getByTestId("edit-files-dialog")).toBeVisible();
      await accessible(page, `Revert or keep in ${theme}`);
      await page.keyboard.press("Escape");
      await expect(page.getByTestId("edit-files-dialog")).toHaveCount(0);
    }
  } finally {
    await closeApp(app);
  }
});
