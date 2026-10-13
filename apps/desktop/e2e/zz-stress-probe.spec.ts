// TEMPORARY (e2e-hardening2): what the hosted Windows session does to a visible, idle Electron window. Deleted before the final push.
import { test } from "@playwright/test";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { closeApp, describeRenderer, launch } from "./support/ui-harness.ts";

test("probe: the renderer of an idle window, every 5 s for 40 s, and after a reload", async () => {
  test.setTimeout(180_000);
  const { app, page } = await launch(mkdtempSync(join(tmpdir(), "modbit-e2e-probe-")));
  for (let i = 0; i < 8; i++) {
    console.log(`PROBE t=${i * 5}s ${await describeRenderer(app, page)}`);
    await new Promise((r) => setTimeout(r, 5_000));
  }
  await page.reload();
  console.log(`PROBE after-reload ${await describeRenderer(app, page)}`);
  await closeApp(app);
});
