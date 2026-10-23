/**
 * REQ-PX-063 / 064 / 068: the model-free gallery states of the Projects group,
 * the Project grouping, a project's page (with members, pull requests, CI and
 * an empty and an archived one), the worktree list, the removal refusals, the
 * apply-back conflict plan, its approval and its result, and the project and
 * view dialogs, in the real Electron app with no Core behind them, under the
 * axe-core suite in light, dark and high contrast. A QA aid: it proves these
 * states are accessible, never that a behaviour works (projects.spec.ts and
 * worktrees.spec.ts prove the behaviour against a real Core).
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

test("PX-063/064/068: the project, worktree and apply-back states render from fixtures with no Core, with words beside every state, and are accessible in every theme", async () => {
  test.setTimeout(240_000);
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-gallery-projects-"));
  const { app, page } = await launch(dataDir, { outDir: OUT, waitForCore: false, env: { MODBIT_CORE_BIN: join(dataDir, "no-such-core") } });
  try {
    await page.evaluate(() => {
      location.hash = "#gallery";
    });
    await expect(page.getByTestId("gallery-projects")).toBeVisible();
    const status = await page.evaluate(() => window.modbit.coreStatus());
    expect((status as { state: string }).state, "no Core is connected to this page").not.toBe("connected");

    // The Projects group is separate from the repositories and holds its tasks and its New project row.
    const list = page.getByTestId("gallery-projects-list");
    await expect(list.locator('[data-testid="agents-section"][data-section="projects"]')).toBeVisible();
    await expect(list.getByTestId("project-row")).toHaveCount(3);
    await expect(list.getByTestId("project-new-row")).toBeVisible();
    await expect(list.getByTestId("project-row").first().getByTestId("project-rollup")).toContainText("3 tasks · 1 needs attention");
    await expect(list.getByTestId("agents-note")).toContainText("A draft cannot join a project yet.");
    // Under the Project grouping the live projects (one empty) and the tasks of no project are the sections.
    await expect(page.getByTestId("gallery-projects-grouped").getByTestId("agents-section")).toHaveCount(3);
    // A project page: counts by class in words, the pull request and CI lines only where the log holds them.
    const page1 = page.getByTestId("gallery-project-page");
    await expect(page1.getByTestId("project-rollup-words")).toContainText("3 tasks");
    await expect(page1.getByTestId("project-prs")).toContainText("2 (1 passing, 1 failing)");
    await expect(page1.getByTestId("project-member")).toHaveCount(3);
    await expect(page.getByTestId("gallery-project-empty").getByTestId("project-prs")).toHaveText("none recorded");
    await expect(page.getByTestId("gallery-project-archived").getByTestId("project-page-restore")).toBeVisible();
    // Worktrees: every state has its words; a skipped cleanup is Skipped; nothing offers a removal the Core refuses.
    const wts = page.getByTestId("gallery-worktrees");
    await expect(wts.getByTestId("worktree-row")).toHaveCount(6);
    await expect(wts.getByTestId("worktrees-report")).toContainText("Skipped:");
    await expect(wts.getByTestId("worktrees-attention-line")).toHaveCount(1);
    for (const state of ["running", "review", "dirty", "unapplied", "removable", "retained", "orphan"]) {
      if (state === "review") continue;
      await expect(wts.locator(`[data-testid="worktree-state"][data-state="${state}"]`).first()).toBeVisible();
    }
    // The removal refusals say the Core's code and offer nothing to confirm.
    await expect(page.getByTestId("gallery-remove-running").getByTestId("worktree-remove-code")).toHaveText("TASK_RUNNING");
    await expect(page.getByTestId("gallery-remove-running").getByTestId("worktree-remove-confirm")).toHaveCount(0);
    await expect(page.getByTestId("gallery-remove-unapplied").getByTestId("worktree-remove-code")).toHaveText("WORKTREE_NEEDS_DECISION");
    await expect(page.getByTestId("gallery-remove-confirm").getByTestId("worktree-remove-loses").locator("li")).toHaveCount(3);
    // The conflict plan: Cancel selected, an overwrite needs its files typed, only the available options can be chosen.
    const conflict = page.getByTestId("gallery-apply-conflict");
    await expect(conflict.locator('[data-testid="apply-option-CANCEL"] input')).toBeChecked();
    await expect(conflict.getByTestId("apply-continue")).toBeDisabled();
    await conflict.locator('[data-testid="apply-option-FULL_OVERWRITE"] input').check();
    await expect(conflict.getByTestId("apply-required").locator("li")).toHaveCount(4);
    await expect(conflict.getByTestId("apply-continue")).toBeDisabled();
    await expect(conflict.getByTestId("apply-remember")).toHaveCount(0);
    await conflict.locator('[data-testid="apply-option-MERGE_MANUALLY"] input').check();
    await expect(conflict.getByTestId("apply-remember")).toBeVisible();
    await expect(conflict.locator('[data-testid="apply-option-STASH"] input')).toBeDisabled();
    await expect(conflict.getByTestId("apply-refusal")).toContainText("needs the files typed");
    await conflict.locator('[data-testid="apply-option-CANCEL"] input').check();
    await expect(page.getByTestId("gallery-apply-approve").getByTestId("apply-deny")).toBeVisible();
    await expect(page.getByTestId("gallery-apply-done").getByTestId("apply-stash")).toContainText("git stash pop");

    for (const theme of THEMES) {
      await page.getByTestId(`theme-${theme}`).click();
      await expect(page.locator("html")).toHaveAttribute("data-theme", theme);
      await accessible(page, `the projects gallery in ${theme}`, '[data-testid="gallery-projects"]');
      for (const [open, dialog] of [
        ["gallery-open-new-project", "project-new-dialog"],
        ["gallery-open-save-view", "view-save-dialog"],
      ] as const) {
        await page.getByTestId(open).click();
        await expect(page.getByTestId(dialog)).toBeVisible();
        await accessible(page, `${dialog} in ${theme}`);
        await page.keyboard.press("Escape");
        await expect(page.getByTestId(dialog)).toHaveCount(0);
      }
    }
  } finally {
    await closeApp(app);
  }
});
