// Captures ZCode reference ground truth from the specimen harness rendered in Electron: screenshots + computed records.
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { PROBE_SRC } from "./probe-lib.mjs";
import { withPage } from "./electron-page.mjs";
import { fileURLToPath } from "node:url";
const OUT = fileURLToPath(new URL("../../../evidence/zcode-ui/reference", import.meta.url));
mkdirSync(OUT, { recursive: true });
const SHEETS = (process.env.SHEETS || "controls,menu,select,popover,tooltip,dialog,alert-dialog,toast").split(",");
const THEMES = ["light", "dark"];
const FILE = `${OUT}/zcode-computed.json`;
const all = existsSync(FILE) ? JSON.parse(readFileSync(FILE, "utf8")) : {};
for (const theme of THEMES) {
  for (const sheet of SHEETS) {
    await withPage(`http://127.0.0.1:5199/?sheet=${sheet}&theme=${theme}`, { scheme: theme }, async (page) => {
      const errs = [];
      page.on("pageerror", (e) => errs.push(String(e)));
      await page.waitForTimeout(900);
      await page.screenshot({ path: `${OUT}/${sheet}-${theme}.png` });
      const recs = await page.evaluate(`(${PROBE_SRC})('[data-slot], [data-specimen], kbd, h2', null)`);
      all[`${sheet}:${theme}`] = recs;
      console.log(sheet, theme, recs.length, "records", errs.length ? "ERR " + errs.slice(0, 2).join(" | ") : "ok");
    });
  }
}
writeFileSync(FILE, JSON.stringify(all, null, 1));
