/**
 * REQ-PX-141 / QUAL-PX-141 (docs/62, DR-PX-2026-10-24-016 sections 5 and 6): the design tokens in the real Electron app against a
 * real Core. The emitted custom properties are resolved by the browser and compared with the retained capture of ZCode's real
 * stylesheet (evidence/zcode-ui/reference), read through the same canonicalising function that wrote it; the type scale, radius
 * hierarchy, shadows and fonts are measured the same way; the bundle carries no ZCode, Z.ai or Zai name and the notices ship.
 * Contrast and axe are proven by the design-tokens unit suite and by the gallery and shell specs in all three themes.
 */
import { expect, test, type Page } from "@playwright/test";
import { existsSync, mkdtempSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { TOKENS_SRC } from "../../../tools/zcode-reference/scripts/probe-lib.mjs";
import { ZDL_TOKENS } from "../../../packages/design-tokens/src/index.ts";
import { appDir, closeApp, launch } from "./support/ui-harness.ts";

const REF = join(appDir, "..", "..", "evidence", "zcode-ui", "reference");
const capture = JSON.parse(readFileSync(join(REF, "zcode-colors-rgba.json"), "utf8")) as Record<"light" | "dark", Record<string, number[]>>;
const computed = JSON.parse(readFileSync(join(REF, "zcode-computed.json"), "utf8")) as Record<string, { slot: string; style: Record<string, string> }[]>;
const resolved = JSON.parse(readFileSync(join(REF, "zcode-tokens-resolved.json"), "utf8")) as Record<"light" | "dark", Record<string, string>>;

async function setTheme(page: Page, theme: "light" | "dark" | "high-contrast"): Promise<void> {
  await page.evaluate((t) => document.documentElement.setAttribute("data-theme", t), theme);
  await expect(page.locator("html")).toHaveAttribute("data-theme", theme);
}

test("PX-141: in the real app every ZCode colour token resolves to the retained capture's colour in light and dark (exactly: the same canonicalising function reads both)", async () => {
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-zdl-tokens-"));
  const { app, page } = await launch(dataDir);
  try {
    for (const theme of ["light", "dark"] as const) {
      await setTheme(page, theme);
      const names = ZDL_TOKENS.map((n) => `--color-${n}`);
      const got = (await page.evaluate(`(${TOKENS_SRC})(${JSON.stringify(names)})`)) as Record<string, number[]>;
      const bad: string[] = [];
      for (const name of names) {
        const want = capture[theme][name];
        const have = got[name];
        if (!want || !have) {
          bad.push(`${theme} ${name}: missing`);
          continue;
        }
        const off = [0, 1, 2].some((i) => have[i] !== want[i]) || Math.abs(have[3]! - want[3]!) > 0.001;
        if (off) bad.push(`${theme} ${name}: ${JSON.stringify(have)} != capture ${JSON.stringify(want)}`);
      }
      expect(bad, `tokens that differ from the ZCode capture in ${theme}`).toEqual([]);
      expect(names.length).toBeGreaterThanOrEqual(100);
    }
  } finally {
    await closeApp(app);
  }
});

test("PX-141: the type scale, radius hierarchy, shadows and font stacks resolve to ZCode's measured values", async () => {
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-zdl-scale-"));
  const { app, page } = await launch(dataDir);
  try {
    await setTheme(page, "light");
    const m = await page.evaluate(() => {
      const root = getComputedStyle(document.documentElement);
      const probe = (css: string) => {
        const el = document.createElement("div");
        el.setAttribute("style", css);
        document.body.appendChild(el);
        const cs = getComputedStyle(el);
        const out = { fontSize: cs.fontSize, borderTopLeftRadius: cs.borderTopLeftRadius, boxShadow: cs.boxShadow, fontFamily: cs.fontFamily };
        el.remove();
        return out;
      };
      const size = (step: string) => probe(`font-size: var(--text-ui-${step})`).fontSize;
      return {
        uiFontSize: root.getPropertyValue("--ui-font-size").trim(),
        text: { xl: size("xl"), lg: size("lg"), base: size("base"), caption: size("caption"), sm: size("sm"), xs: size("xs") },
        radius: Object.fromEntries(["sm", "md", "lg", "xl", "2xl"].map((k) => [k, probe(`border-radius: var(--radius-${k})`).borderTopLeftRadius])),
        shadowMd: probe("box-shadow: var(--shadow-md)").boxShadow,
        sans: probe("font-family: var(--font-sans)").fontFamily,
        mono: probe("font-family: var(--font-mono)").fontFamily,
        bodyFont: getComputedStyle(document.body).fontFamily,
        bodySize: getComputedStyle(document.body).fontSize,
      };
    });
    expect(m.uiFontSize).toBe("14px");
    // DESIGN.md "UI font tokens": xl +4, lg +2, base 0, caption -1, sm -2, xs -4 of --ui-font-size.
    expect(m.text).toEqual({ xl: "18px", lg: "16px", base: "14px", caption: "13px", sm: "12px", xs: "10px" });
    expect(m.radius).toEqual({ sm: "4px", md: "6px", lg: "8px", xl: "12px", "2xl": "16px" });
    // The visible layers of ZCode's shadow-md, as measured on its real menu and dialog (the capture).
    const menu = computed["menu:light"]!.find((r) => r.slot === "dropdown-menu-content")!.style.boxShadow;
    expect(menu).toContain("rgba(0, 0, 0, 0.1) 0px 4px 6px -1px, rgba(0, 0, 0, 0.1) 0px 2px 4px -2px");
    expect(m.shadowMd).toBe("rgba(0, 0, 0, 0.1) 0px 4px 6px -1px, rgba(0, 0, 0, 0.1) 0px 2px 4px -2px");
    // The font stacks are ZCode's, byte for byte once whitespace is normalised.
    const flat = (s: string) => s.replace(/\s+/g, " ").trim();
    expect(flat(m.sans)).toBe(flat(resolved.light["--font-sans"]!));
    expect(flat(m.mono)).toBe(flat(resolved.light["--font-mono"]!));
    expect(flat(m.bodyFont)).toBe(flat(resolved.light["--font-sans"]!));
    expect(m.bodySize).toBe("14px");
  } finally {
    await closeApp(app);
  }
});

test("PX-141: the high-contrast theme still defines every ZCode token and keeps Modbit's own edges (ZDX-06)", async () => {
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-zdl-hc-"));
  const { app, page } = await launch(dataDir);
  try {
    await setTheme(page, "high-contrast");
    const names = ZDL_TOKENS.map((n) => `--color-${n}`);
    const got = (await page.evaluate(`(${TOKENS_SRC})(${JSON.stringify(names)})`)) as Record<string, number[]>;
    expect(Object.keys(got).length, "every token resolves in high contrast").toBe(names.length);
    expect(got["--color-foreground"]!.slice(0, 3), "the foreground is Modbit's high-contrast white").toEqual([255, 255, 255]);
    expect(got["--color-input-border"]!.slice(0, 3), "a control edge is the full-contrast edge").toEqual([255, 255, 255]);
  } finally {
    await closeApp(app);
  }
});

test("PX-141: the bundle carries no ZCode, Z.ai or Zai name or mark, and the third-party notices ship with the Apache-2.0 text and the upstream revision", () => {
  for (const file of ["renderer/index.js", "renderer/app.css", "renderer/index.html"]) {
    const text = readFileSync(join(appDir, "dist", file), "utf8");
    expect(text.match(/zcode|z\.ai|\bzai\b/gi), `${file} must not name the vendor`).toBeNull();
  }
  const dir = join(appDir, "..", "..", "third-party", "zcode-ui");
  for (const f of ["LICENSE", "NOTICE.md", "LUCIDE-LICENSE", "TAILWIND-LICENSE"]) expect(existsSync(join(dir, f)), f).toBe(true);
  expect(readFileSync(join(dir, "LICENSE"), "utf8")).toContain("Apache License");
  const notice = readFileSync(join(dir, "NOTICE.md"), "utf8");
  expect(notice).toContain("3c5b227649371507ddc4eb46ac5133afe6db1cab");
  expect(notice).toContain("Apache-2.0");
  expect(notice).toContain("does not use them");
});
