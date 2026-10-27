// Resolves each --color-* token of ZCode's real stylesheet to canonical rgba (per theme) through the browser.
import { writeFileSync } from "node:fs";
import { withPage } from "./electron-page.mjs";
import { TOKENS_SRC } from "./probe-lib.mjs";
import { fileURLToPath } from "node:url";
const OUT = fileURLToPath(new URL("../../../evidence/zcode-ui/reference", import.meta.url));
const out = {};
for (const theme of ["light", "dark"]) {
  await withPage(`http://127.0.0.1:5199/?sheet=tooltip&theme=${theme}`, { scheme: theme }, async (page) => {
    await page.waitForTimeout(500);
    const names = await page.evaluate(() => {
      const found = new Set();
      const walk = (rules) => { for (const r of rules) { if (r.style) for (const n of r.style) if (n.startsWith("--color-")) found.add(n); if (r.cssRules) walk(r.cssRules); } };
      for (const s of document.styleSheets) { try { walk(s.cssRules); } catch {} }
      return [...found].sort();
    });
    out[theme] = await page.evaluate(`(${TOKENS_SRC})(${JSON.stringify(names)})`);
    console.log(theme, Object.keys(out[theme]).length);
  });
}
writeFileSync(`${OUT}/zcode-colors-rgba.json`, JSON.stringify(out));
