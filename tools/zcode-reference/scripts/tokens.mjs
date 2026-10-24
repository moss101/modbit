// Dumps every resolved custom property (colour, text, radius, shadow, font) of ZCode's real stylesheet for both Zai themes.
import { writeFileSync } from "node:fs";
import { withPage } from "./electron-page.mjs";
import { fileURLToPath } from "node:url";
const OUT = fileURLToPath(new URL("../../../evidence/zcode-ui/reference", import.meta.url));
const out = {};
for (const theme of ["light", "dark"]) {
  await withPage(`http://127.0.0.1:5199/?sheet=tooltip&theme=${theme}`, { scheme: theme }, async (page) => {
    await page.waitForTimeout(600);
    out[theme] = await page.evaluate(() => {
      const names = new Set();
      const walk = (rules) => { for (const r of rules) { if (r.style) for (const n of r.style) if (n.startsWith("--")) names.add(n); if (r.cssRules) walk(r.cssRules); } };
      for (const s of document.styleSheets) { try { walk(s.cssRules); } catch {} }
      const cs = getComputedStyle(document.documentElement);
      const o = {};
      for (const n of [...names].sort()) o[n] = cs.getPropertyValue(n).trim();
      return o;
    });
    console.log(theme, Object.keys(out[theme]).length);
  });
}
writeFileSync(`${OUT}/zcode-tokens-resolved.json`, JSON.stringify(out, null, 1));
