/**
 * M10.2 / PX-030 (docs/76 "packaging"): the packaged app, not the checkout,
 * starts its own bundled Core. No MODBIT_CORE_BIN and no source tree: the only
 * way "Core connected" appears is that the packaged main process resolved the
 * Core and execd from its resources directory and the Core opened its database.
 */
import { _electron as electron, expect, test } from "@playwright/test";
import { execFileSync, spawnSync } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { packagedExecutable } from "./packaged-app.js";

test("the packaged app starts the Core it carries and reports it connected", async () => {
  const { exe, resources } = packagedExecutable();
  const coreName = process.platform === "win32" ? "modbit-core.exe" : "modbit-core";
  expect(existsSync(join(resources, coreName)), "the Core ships in the app resources").toBe(true);
  expect(existsSync(join(resources, process.platform === "win32" ? "modbit-execd.exe" : "modbit-execd")), "the execd broker ships beside it").toBe(true);
  // The package says what it is: UNSIGNED and for tests only. No code-signing identity exists yet (the owner's).
  const info = JSON.parse(readFileSync(join(resources, "build-info.json"), "utf8"));
  expect(info).toMatchObject({ label: "UNSIGNED", signed: false, notarized: false });
  expect(JSON.parse(readFileSync(join(resources, "update-keys.json"), "utf8")).keys.length, "an unsigned test package pins no update key: updates fail closed").toBe(info.update_keys_pinned);
  if (process.platform === "darwin") {
    // No Developer ID, no team: whatever signature the bundle carries is the linker's or electron-builder's ad-hoc one,
    // which macOS needs to run arm64 code at all and which names nobody.
    const bundle = join(resources, "..", "..");
    const sig = spawnSync("codesign", ["-dv", "--verbose=2", bundle], { encoding: "utf8" });
    const said = `${sig.stdout}${sig.stderr}`;
    expect(said).not.toMatch(/Authority=Developer ID/);
    expect(said).toMatch(/TeamIdentifier=not set|code object is not signed at all/);
  }

  const env = { ...process.env, MODBIT_DATA_DIR: mkdtempSync(join(tmpdir(), "modbit-packaged-")) } as Record<string, string>;
  delete env.MODBIT_CORE_BIN;
  delete env.MODBIT_EXECD_BIN;
  const app = await electron.launch({ executablePath: exe, env });
  try {
    expect(await app.evaluate(({ app: a }) => a.isPackaged)).toBe(true);
    const page = await app.firstWindow();
    await expect(page.getByTestId("core-status")).toContainText("Core connected", { timeout: 60_000 });
    // The connected Core is the bundled one, not a stray debug build.
    const pid = app.process().pid;
    if (process.platform !== "win32" && pid !== undefined) {
      const ps = execFileSync("ps", ["-A", "-o", "command="], { encoding: "utf8" });
      expect(ps).toContain(join(resources, coreName));
    }
  } finally {
    await app.close();
  }
});
