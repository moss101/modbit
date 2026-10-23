import { test } from "node:test";
import assert from "node:assert/strict";
import { COLOR_ROLES, PALETTES, PAIRS, checkPairs, conversationType, contrastRatio, failingPairs, resolveTheme, sharedVariables, themeVariables, tokensCss, type ColorRoles, type ThemeName } from "./index.ts";

const THEMES: ThemeName[] = ["light", "dark", "high-contrast"];

test("REQ-PX-044: every theme has exactly the same colour role set (a missing or extra role fails)", () => {
  for (const t of THEMES) {
    assert.deepEqual(Object.keys(PALETTES[t]).sort(), [...COLOR_ROLES].sort(), t);
    for (const [role, value] of Object.entries(PALETTES[t])) assert.match(value, /^#[0-9a-f]{6}$/, `${t}.${role} is an opaque hex colour`);
  }
  const keys = (t: ThemeName) => Object.keys(themeVariables(t)).sort();
  assert.deepEqual(keys("light"), keys("dark"));
  assert.deepEqual(keys("light"), keys("high-contrast"));
});

test("AFW-A12: every text/background and control pair meets WCAG AA in every theme", () => {
  assert.ok(PAIRS.length >= 70, "the pair list covers the primitives");
  for (const t of THEMES) {
    const bad = failingPairs(t).map((r) => `${t}: ${r.fg} on ${r.bg} (${r.use}) ${r.ratio.toFixed(2)} < ${r.min}`);
    assert.deepEqual(bad, []);
  }
});

test("AFW-A12: a token lowered below its ratio fails the contrast test", () => {
  const lowered: ColorRoles = { ...PALETTES.light, textMuted: "#9aa1ab" };
  const bad = failingPairs("light", lowered);
  assert.ok(bad.some((r) => r.fg === "textMuted"));
  // The old UI's primary button (white on #7aa2ff in the dark scheme) measured 2.49:1: it must fail.
  assert.ok(Math.abs(contrastRatio("#ffffff", "#7aa2ff") - 2.49) < 0.05);
  const old: ColorRoles = { ...PALETTES.dark, textOnAccent: "#ffffff", accent: "#7aa2ff" };
  assert.ok(failingPairs("dark", old).some((r) => r.fg === "textOnAccent" && r.bg === "accent"));
  // And Modbit's own primary button passes with room to spare.
  const primary = checkPairs("dark").find((r) => r.fg === "textOnAccent" && r.bg === "accent");
  assert.ok(primary && primary.ratio >= 7);
});

test("AFW-A12: the disabled text level is never used by a pair", () => {
  assert.ok(PAIRS.every((p) => p.fg !== "textDisabled" && p.bg !== "textDisabled"));
});

test("AFW-A11: the scale tokens follow the spec's grid", () => {
  const shared = sharedVariables();
  assert.equal(shared["--mb-space-1"], "4px");
  assert.equal(shared["--mb-space-0_5"], "2px");
  assert.equal(shared["--mb-space-1_5"], "6px");
  assert.equal(shared["--mb-radius-full"], "9999px");
  assert.equal(shared["--mb-control-md"], "28px");
  assert.equal(shared["--mb-type-chrome-size"], "13px");
  assert.equal(shared["--mb-type-chrome-line"], "18px");
  assert.equal(shared["--mb-motion-fast"], "100ms");
  assert.equal(shared["--mb-press-scale"], "0.98");
  assert.match(shared["--mb-font-sans"]!, /system-ui/);
  assert.equal(conversationType(100).size, 14);
  assert.equal(conversationType(1000).size, conversationType(140).size, "the Text size setting is clamped");
});

test("AFW-A14: the stylesheet collapses motion under prefers-reduced-motion and defines every theme", () => {
  const css = tokensCss();
  assert.match(css, /@media \(prefers-reduced-motion: reduce\)[\s\S]*animation: none !important[\s\S]*transition: none !important/);
  assert.match(css, /@media \(prefers-color-scheme: dark\)/);
  assert.match(css, /@media \(prefers-contrast: more\)/);
  assert.match(css, /@media \(forced-colors: active\)/);
  for (const t of THEMES) assert.ok(css.includes(`:root[data-theme="${t}"]`), t);
});

test("AFW-A13: Follow system resolves from what the OS reports; an explicit choice wins", () => {
  assert.equal(resolveTheme("system", { dark: true, moreContrast: false }), "dark");
  assert.equal(resolveTheme("system", { dark: false, moreContrast: false }), "light");
  assert.equal(resolveTheme("system", { dark: false, moreContrast: true }), "high-contrast");
  assert.equal(resolveTheme("light", { dark: true, moreContrast: true }), "light");
});
