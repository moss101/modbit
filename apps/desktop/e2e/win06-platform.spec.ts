/**
 * WIN-06 platform conformance (PX-030, docs/78): the Windows shell items the
 * catalog did not already prove —
 *
 * - the Core binary starts from a path with spaces and non-ASCII characters
 *   in it (`C:\Users\Zoë Müller\…\bin with spaces\`), because discovery and
 *   spawn must be argv-safe on every spelling a real user profile has;
 * - under OS-level forced colors the shell stays legible and its surfaces
 *   stay opaque (no translucent surface exists; ESH-B03's fallback is
 *   trivially true and this pins it);
 * - on Windows the keychain backend is DPAPI specifically.
 */
import { _electron as electron, expect, test } from "@playwright/test";
import { copyFileSync, mkdirSync, mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { closeApp } from "./support/close-app.ts";

const appDir = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const repoRoot = resolve(appDir, "..", "..");
const coreBin = join(repoRoot, "target", "debug", process.platform === "win32" ? "modbit-core.exe" : "modbit-core");
const execdBin = join(repoRoot, "target", "debug", process.platform === "win32" ? "modbit-execd.exe" : "modbit-execd");

async function launchWith(coreOverride: string, dataDir: string) {
  const app = await electron.launch({
    args: [join(appDir, "dist", "main", "main.cjs")],
    env: {
      ...process.env,
      MODBIT_DATA_DIR: dataDir,
      MODBIT_CORE_BIN: coreOverride,
      OPENAI_API_KEY: "",
      ANTHROPIC_API_KEY: "",
      MODBIT_OPENAI_BASE_URL: "",
      MODBIT_ANTHROPIC_BASE_URL: "",
    },
  });
  const page = await app.firstWindow();
  return { app, page };
}

test("WIN-06: the Core starts from a path with spaces and non-ASCII characters", async ({}, testInfo) => {
  testInfo.annotations.push({ type: "win06", description: `core at ${coreBin}` });
  // A user profile spelling no test used before: spaces and non-ASCII.
  const home = mkdtempSync(join(tmpdir(), "modbit-win06-"));
  const dir = join(home, "Zoë Müller", "bin with spaces");
  mkdirSync(dir, { recursive: true });
  const core = join(dir, process.platform === "win32" ? "modbit-core.exe" : "modbit-core");
  copyFileSync(coreBin, core);
  // The broker sits beside the Core the way the packaged app lays it out.
  try {
    copyFileSync(execdBin, join(dir, process.platform === "win32" ? "modbit-execd.exe" : "modbit-execd"));
  } catch {
    // A dev tree without the broker still connects a Core; terminals degrade.
  }
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-win06-data-"));
  const { app, page } = await launchWith(core, dataDir);
  try {
    await expect(page.getByTestId("core-status")).toContainText("Core connected", { timeout: 60_000 });
  } finally {
    await closeApp(app);
  }
});

test("WIN-06: under OS forced colors the shell stays legible and its surfaces stay opaque", async () => {
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-win06-fc-"));
  const { app, page } = await launchWith(coreBin, dataDir);
  try {
    await expect(page.getByTestId("core-status")).toContainText("Core connected", { timeout: 60_000 });
    await page.emulateMedia({ forcedColors: "active", reducedMotion: "reduce" });
    // The first-run surface's primary controls are still there and visible,
    // whatever palette the forced-colors engine picked.
    await expect(page.getByTestId("provider-key")).toBeVisible();
    await expect(page.getByTestId("provider-test")).toBeVisible();
    // ESH-B03: no translucent fallback to hunt for — every surface the user
    // looks at paints an opaque background.
    const backed = await page.evaluate(() => {
      const ids = ["provider-key", "provider-test"];
      return ids.map((id) => {
        const el = document.querySelector(`[data-testid="${id}"]`);
        if (!el) return { id, missing: true, painted: false };
        // Opacity is a *backdrop* property: the control itself may paint
        // nothing while a container paints the full surface. Something on
        // the chain must paint an alpha-1 background (ESH-B03: no
        // translucent fallback).
        let node: HTMLElement | null = el as HTMLElement;
        let painted = false;
        while (node) {
          const bg = getComputedStyle(node).backgroundColor;
          const m = /rgba?\(/.exec(bg);
          if (m) {
            const a = /rgba\(.*,\s*([\d.]+)\)/.exec(bg);
            if (!a || Number(a[1]) === 1) {
              painted = true;
              break;
            }
          }
          node = node.parentElement;
        }
        return { id, missing: false, painted };
      });
    });
    for (const surface of backed) {
      expect(surface.missing, `${surface.id} present`).toBe(false);
      expect(surface.painted, `${surface.id} sits on an opaque backdrop`).toBe(true);
    }
  } finally {
    await closeApp(app);
  }
});

test("WIN-06: on Windows the keychain backend is DPAPI", async () => {
  test.skip(process.platform !== "win32", "the backend name is per-platform");
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-win06-dpapi-"));
  const { app, page } = await launchWith(coreBin, dataDir);
  try {
    await expect(page.getByTestId("core-status")).toContainText("Core connected", { timeout: 60_000 });
    const status = await page.evaluate(() => window.modbit.providerStatus());
    expect(status.keychainBackend, "the Windows keychain is DPAPI").toBe("dpapi");
  } finally {
    await closeApp(app);
  }
});
