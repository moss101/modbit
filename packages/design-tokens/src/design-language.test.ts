import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { PALETTES, ZDL_DARK, ZDL_LIGHT, ZDL_TOKENS, over, parseColor, themeVariables, zdlColors, type ColorRoles, type ZdlColors } from "./index.ts";

const REF = (file: string) => JSON.parse(readFileSync(fileURLToPath(new URL(`../../../evidence/zcode-ui/reference/${file}`, import.meta.url)), "utf8"));
/** The retained capture of ZCode's real stylesheet (evidence/zcode-ui/reference, DR-PX-2026-10-24-016 section 6): canonical sRGB per token... */
const CAPTURE = REF("zcode-colors-rgba.json") as Record<"light" | "dark", Record<string, number[]>>;
/** ...and every custom property as the browser resolved it (the string ZCode declared, or the value a var() chain flattened to). */
const RESOLVED = REF("zcode-tokens-resolved.json") as Record<"light" | "dark", Record<string, string>>;
const norm = (c: string) => c.toLowerCase().replace(/\s+/g, "");

test("FG-1 (PX-141): every ZCode token in the table equals the retained capture in both themes", () => {
  assert.ok(ZDL_TOKENS.length >= 100, "the table carries the semantic tokens the renderer reads");
  for (const [theme, table] of [["light", ZDL_LIGHT], ["dark", ZDL_DARK]] as const) {
    for (const name of ZDL_TOKENS) {
      const declared = RESOLVED[theme][`--color-${name}`];
      const captured = CAPTURE[theme][`--color-${name}`];
      assert.ok(declared !== undefined && captured, `${theme}: --color-${name} is in the capture`);
      if (/^(#|rgba?\()/i.test(declared!)) {
        // ZCode declared a literal: the table holds exactly that literal.
        assert.equal(norm(table[name]), norm(declared!), `${theme}: ${name}`);
      } else {
        // An oklch() or color-mix() token is stored as the sRGB it resolves to. The capture reads translucent colours back through
        // an 8-bit canvas, so channels agree exactly when opaque and to a few steps when not; alpha agrees to one 8-bit step.
        const [r, g, b, a] = parseColor(table[name]);
        const tolerance = captured![3]! >= 1 ? 0 : 3;
        for (const [i, c] of [r, g, b].entries()) assert.ok(Math.abs(c - captured![i]!) <= tolerance, `${theme}: ${name} channel ${i}: ${c} vs ${captured![i]}`);
        assert.ok(Math.abs(a - captured![3]!) <= 1 / 255 + 1e-9, `${theme}: ${name} alpha ${a} vs ${captured![3]}`);
      }
    }
  }
});

test("FG-1 (PX-141): a one-channel change to a reference-equal token is detected by the same comparison (the gate can fail)", () => {
  const mutated: ZdlColors = { ...ZDL_LIGHT, background: "#f8f8f9" };
  assert.notEqual(norm(mutated.background), norm(RESOLVED.light["--color-background"]!));
  const [, , blue] = parseColor(mutated.background);
  assert.notEqual(blue, CAPTURE.light["--color-background"]![2]);
});

test("ZDX (PX-141): the light and dark roles are the ZCode tokens through one role map, composited over the surface they sit on", () => {
  for (const [theme, z] of [["light", ZDL_LIGHT], ["dark", ZDL_DARK]] as const) {
    const r: ColorRoles = PALETTES[theme];
    assert.equal(r.bg, z["background-win-alt"], theme);
    assert.equal(r.panel, z.background, theme);
    assert.equal(r.raised, z.card, theme);
    assert.equal(r.text, z.foreground, theme);
    assert.equal(r.textMuted, z["remote-foreground-muted"], `${theme}: ZDX-01 secondary text is ZCode's readable muted token, not foreground-subtle`);
    assert.equal(r.accent, z.primary, theme);
    assert.equal(r.border, over(z.border, z.background), theme);
    assert.equal(r.hover, over(z.hover, z.background), theme);
    assert.equal(r.selected, over(z.selected, z.background), theme);
    assert.equal(r.ok, z["success-surface-foreground"], `${theme}: ZDX-02 status text`);
    assert.equal(r.warn, z["warning-surface-foreground"], theme);
    assert.equal(r.danger, z["destructive-surface-foreground"], theme);
  }
});

test("ZDX-03/04/05 (PX-141): the declared exceptions really deviate from ZCode (a stale exception fails) and each is a documented value", () => {
  // ZDX-03: a control edge is never ZCode's own border, which is 1.25:1 (light) and 1.36:1 (dark).
  assert.notEqual(PALETTES.light.borderControl, over(ZDL_LIGHT.border, ZDL_LIGHT.card));
  assert.notEqual(PALETTES.dark.borderControl, over(ZDL_DARK.border, ZDL_DARK.card));
  // ZDX-04: focus is a visible ring colour; ZCode's input-border-focused is 1.39:1 / 1.60:1.
  assert.notEqual(PALETTES.light.focus, over(ZDL_LIGHT["input-border-focused"], ZDL_LIGHT.card));
  assert.notEqual(PALETTES.dark.focus, over(ZDL_DARK["input-border-focused"], ZDL_DARK.card));
  // ZDX-05: ZCode's dark destructive label is white at 3.03:1; Modbit's is the inverse (black) label.
  assert.equal(PALETTES.light.textOnDangerSolid, ZDL_LIGHT["destructive-foreground"], "light: ZCode's own white label already passes 4.5:1, so it is not an exception there");
  assert.notEqual(PALETTES.dark.textOnDangerSolid, ZDL_DARK["destructive-foreground"]);
});

test("ZDX-06 (PX-141): the high-contrast palette is Modbit's own and unchanged, and every ZCode token name is defined in it", () => {
  const before = {
    bg: "#000000", panel: "#000000", raised: "#0a0a0a", sunken: "#000000", inverse: "#ffffff", text: "#ffffff", textMuted: "#e6e6e6",
    textDisabled: "#8c8c8c", textOnAccent: "#000000", textOnInverse: "#000000", border: "#8c8c8c", borderControl: "#ffffff",
    focus: "#ffd400", accent: "#7fd1ff", accentHover: "#a8e0ff", accentText: "#7fd1ff", ok: "#6df28f", warn: "#ffd400", danger: "#ff9a93",
    info: "#7fd1ff", okTint: "#001f0b", warnTint: "#2a2200", dangerTint: "#2e0a08", infoTint: "#00192b", modeAsk: "#6df28f",
    modePlan: "#ffd400", modeDebug: "#ff9a93", modeMultitask: "#d0bfff", hover: "#262626", selected: "#1f3a5c", scrim: "#000000",
  };
  for (const [role, value] of Object.entries(before)) assert.equal(PALETTES["high-contrast"][role as keyof ColorRoles], value, `high-contrast ${role} is unchanged`);
  const hc = zdlColors("high-contrast");
  for (const name of ZDL_TOKENS) assert.ok(hc[name], `high contrast defines --color-${name}`);
  assert.equal(hc.foreground, PALETTES["high-contrast"].text);
  assert.equal(hc["input-border"], PALETTES["high-contrast"].borderControl);
});

test("PX-141: every theme emits every ZCode --color-* token and the same key set", () => {
  const keys = (t: "light" | "dark" | "high-contrast") => Object.keys(themeVariables(t)).filter((k) => k.startsWith("--color-")).sort();
  assert.deepEqual(keys("light"), keys("dark"));
  assert.deepEqual(keys("light"), keys("high-contrast"));
  assert.equal(keys("light").length, ZDL_TOKENS.length);
});

test("PX-141: the alpha-aware helpers composite exactly and refuse what they cannot parse", () => {
  assert.equal(over("rgba(13, 13, 13, 0.1)", "#ffffff"), "#e7e7e7");
  assert.equal(over("#123456", "#ffffff"), "#123456");
  assert.equal(over("rgba(0, 0, 0, 0)", "#abcdef"), "#abcdef");
  assert.deepEqual(parseColor("rgb(1, 2, 3)"), [1, 2, 3, 1]);
  assert.throws(() => parseColor("oklch(26.9% 0 0)"));
  assert.throws(() => parseColor("color-mix(in oklab, red 60%, transparent)"));
});
