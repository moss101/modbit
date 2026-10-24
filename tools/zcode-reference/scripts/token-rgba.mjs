// Resolves each --color-* token of ZCode's real stylesheet to canonical rgba (per theme) through the browser.
import { writeFileSync } from "node:fs";
import { withPage } from "./electron-page.mjs";
import { fileURLToPath } from "node:url";
const OUT = fileURLToPath(new URL("../../../evidence/zcode-ui/reference", import.meta.url));
const out = {};
for (const theme of ["light", "dark"]) {
  await withPage(`http://127.0.0.1:5199/?sheet=tooltip&theme=${theme}`, { scheme: theme }, async (page) => {
    await page.waitForTimeout(500);
    out[theme] = await page.evaluate(() => {
      const names = new Set();
      const walk = (rules) => { for (const r of rules) { if (r.style) for (const n of r.style) if (n.startsWith("--color-")) names.add(n); if (r.cssRules) walk(r.cssRules); } };
      for (const s of document.styleSheets) { try { walk(s.cssRules); } catch {} }
      const cv = document.createElement("canvas"); cv.width = cv.height = 1;
      const cx = cv.getContext("2d", { willReadFrequently: true });
      const px = (c, bg) => { cx.clearRect(0, 0, 1, 1); cx.fillStyle = bg; cx.fillRect(0, 0, 1, 1); cx.fillStyle = c; cx.fillRect(0, 0, 1, 1); return cx.getImageData(0, 0, 1, 1).data; };
      const probe = document.createElement("div"); document.body.appendChild(probe);
      const o = {};
      for (const n of [...names].sort()) {
        probe.style.color = `var(${n})`;
        const c = getComputedStyle(probe).color;
        if (!c) continue;
        const w = px(c, "#fff"), b = px(c, "#000");
        const a = Math.max(0, Math.min(1, 1 - (w[0] - b[0]) / 255));
        o[n] = a < 0.004 ? [0, 0, 0, 0] : [Math.round(b[0] / a), Math.round(b[1] / a), Math.round(b[2] / a), Math.round(a * 1000) / 1000];
      }
      probe.remove();
      return o;
    });
    console.log(theme, Object.keys(out[theme]).length);
  });
}
writeFileSync(`${OUT}/zcode-colors-rgba.json`, JSON.stringify(out));
