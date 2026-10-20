/**
 * REQ-PX-074: the model-free gallery states of native computer control's
 * desktop surface (the permission window, presence and Stop, the action cards
 * and the approval card), in the real Electron app with no Core behind them,
 * under the axe-core suite in light, dark and high contrast. A QA aid: it
 * proves these states are accessible, never that a behaviour works
 * (computer-surface.spec.ts proves those against a real Core and actuator).
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

test("PX-074: every state of the native-control surface is accessible in every theme, says its state in words, and offers no Always on a computer card", async () => {
  test.setTimeout(240_000);
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-gallery-computer-"));
  const { app, page } = await launch(dataDir, { outDir: OUT, waitForCore: false, env: { MODBIT_CORE_BIN: join(dataDir, "no-such-core") } });
  try {
    await page.evaluate(() => {
      location.hash = "#gallery";
    });
    await expect(page.getByTestId("gallery-computer")).toBeVisible();
    // The permission rows: checking, allow, ready; the steps only where there is a step to take.
    const allow = page.getByTestId("gallery-computer-perm-allow");
    await expect(allow.getByTestId("perm-accessibility")).toHaveAttribute("data-status", "allow");
    await expect(allow.getByTestId("perm-screen_recording")).toHaveAttribute("data-status", "checking");
    await expect(allow.getByTestId("perm-accessibility-steps")).toContainText("Privacy & Security");
    await expect(page.getByTestId("gallery-computer-perm-ready").getByTestId("perm-allready")).toBeVisible();
    await expect(page.getByTestId("gallery-computer-perm-ended").getByTestId("perm-polling-ended")).toContainText("Check again");
    await expect(page.getByTestId("gallery-computer-perm-nohelper").getByTestId("perm-nohelper")).toContainText("not running");
    // Presence: who, what, and Stop only while the machine is held.
    await expect(page.getByTestId("gallery-computer-presence-held").getByTestId("computer-presence-text")).toContainText("TextEdit");
    await expect(page.getByTestId("gallery-computer-presence-held").getByTestId("computer-stop")).toHaveCount(1);
    await expect(page.getByTestId("gallery-computer-presence-screen").getByTestId("computer-presence-text").first()).toContainText("whole screen");
    await expect(page.getByTestId("gallery-computer-presence-stopped").getByTestId("computer-stop")).toHaveCount(0);
    await expect(page.getByTestId("gallery-computer-presence-stopped")).toContainText("Stopped by you");
    await expect(page.getByTestId("gallery-computer-presence-latched")).toContainText("Outcome unknown");
    // Cards: each state in words.
    const cards = page.getByTestId("gallery-computer-cards").getByTestId("computer-card");
    await expect(cards).toHaveCount(8);
    await expect(cards.nth(3).getByTestId("computer-card-status")).toContainText("target stale");
    await expect(cards.nth(4).getByTestId("computer-card-status")).toContainText("Skipped");
    await expect(cards.nth(5).getByTestId("computer-frame-expired")).toBeVisible();
    await expect(cards.nth(6).getByTestId("computer-latch-resolve")).toBeVisible();
    await expect(cards.nth(7).getByTestId("computer-card-status")).toContainText("USER_ABORTED");
    // Approval cards: the exact intent, and never an allowlist affordance.
    for (const id of ["action", "grant", "screen", "confirm", "stale"]) {
      const g = page.getByTestId(`gallery-computer-approval-${id}`);
      await expect(g.getByTestId("computer-approval")).toBeVisible();
      for (const t of ["approval-always", "approval-prefix", "approval-scope", "approval-change-mode"]) await expect(g.getByTestId(t), `${id}: ${t}`).toHaveCount(0);
    }
    await expect(page.getByTestId("gallery-computer-approval-screen").getByTestId("computer-approval-scope")).toContainText("takes the display and the real mouse");
    await expect(page.getByTestId("gallery-computer-approval-stale").getByTestId("approval-note")).toContainText("no longer the one this card showed");

    for (const theme of THEMES) {
      await page.getByTestId(`theme-${theme}`).click();
      await expect(page.locator("html")).toHaveAttribute("data-theme", theme);
      await accessible(page, `the native-control gallery in ${theme}`, '[data-testid="gallery-computer"]');
    }
  } finally {
    await closeApp(app);
  }
});
