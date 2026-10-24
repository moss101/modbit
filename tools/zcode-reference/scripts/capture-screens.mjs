// Captures every screen section of Modbit's state gallery (real components from fixtures, no model) in light and dark, for the
// supplemental before/after record of PX-143. usage: node capture-screens.mjs <build dir under apps/desktop> <output dir>
// e.g. node capture-screens.mjs dist/baseline-build evidence/m10/PX-143/before
import { mkdirSync, mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { closeApp, launch, setContentSize } from "../../../apps/desktop/e2e/support/ui-harness.ts";

const [, , build = "dist/gallery-build", out = "screens"] = process.argv;
const IDS = ["gallery-primitives", "gallery-shell", "gallery-agent-list", "gallery-conversation", "gallery-controls", "gallery-projects", "gallery-computer", "gallery-automations", "gallery-state-approvals", "gallery-state-generating", "gallery-state-error", "gallery-state-empty", "gallery-state-offline"];
mkdirSync(out, { recursive: true });
const dataDir = mkdtempSync(join(tmpdir(), "mb-screens-"));
const { app, page } = await launch(dataDir, { outDir: build, waitForCore: false, env: { MODBIT_CORE_BIN: join(dataDir, "no-such-core") } });
await setContentSize(app, page, 1280, 1000);
await page.evaluate(() => (location.hash = "#gallery"));
await page.getByTestId("gallery").waitFor();
for (const theme of ["light", "dark"]) {
  await page.getByTestId(`theme-${theme}`).click();
  await page.waitForTimeout(500);
  for (const id of IDS) {
    const el = page.getByTestId(id);
    if ((await el.count()) === 0) continue;
    await el.first().scrollIntoViewIfNeeded();
    await el.first().screenshot({ path: join(out, `${id}-${theme}.png`) });
  }
  console.log("captured", theme);
}
await closeApp(app);
