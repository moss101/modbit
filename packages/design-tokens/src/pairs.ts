/**
 * Every text/background and control/background pair the primitives and the
 * shell use, with the WCAG 2.2 AA minimum it must meet (AFW-A12): 4.5:1 for
 * text, 3:1 for control edges, icons and focus rings. `checkPairs` is the
 * computed contrast test: a token lowered below its ratio fails here, before
 * any browser accessibility run. `textDisabled` is deliberately absent
 * (disabled and decorative use only).
 */
import { contrastRatio } from "./contrast.ts";
import { PALETTES, type ColorRole, type ColorRoles, type ThemeName } from "./palette.ts";

export interface Pair {
  fg: ColorRole;
  bg: ColorRole;
  min: 4.5 | 3;
  use: string;
}

const SURFACES: ColorRole[] = ["bg", "panel", "raised", "sunken"];
const FILLS: ColorRole[] = [...SURFACES, "hover", "selected"];
const TINTS: [ColorRole, ColorRole][] = [
  ["ok", "okTint"],
  ["warn", "warnTint"],
  ["danger", "dangerTint"],
  ["info", "infoTint"],
];

function build(): Pair[] {
  const pairs: Pair[] = [];
  for (const bg of FILLS) {
    pairs.push({ fg: "text", bg, min: 4.5, use: "body text" });
    pairs.push({ fg: "textMuted", bg, min: 4.5, use: "secondary text" });
    pairs.push({ fg: "accentText", bg, min: 4.5, use: "links and accent text" });
  }
  for (const bg of SURFACES) {
    for (const fg of ["ok", "warn", "danger", "info", "modeAsk", "modePlan", "modeDebug", "modeMultitask"] as ColorRole[]) pairs.push({ fg, bg, min: 4.5, use: "status and mode text and icons" });
    pairs.push({ fg: "borderControl", bg, min: 3, use: "control edge" });
    pairs.push({ fg: "focus", bg, min: 3, use: "focus ring" });
    pairs.push({ fg: "accent", bg, min: 3, use: "solid accent control against its surface" });
  }
  pairs.push({ fg: "focus", bg: "selected", min: 3, use: "focus ring on a selected row" });
  pairs.push({ fg: "borderControl", bg: "selected", min: 3, use: "control edge on a selected row" });
  for (const [fg, bg] of TINTS) {
    pairs.push({ fg, bg, min: 4.5, use: "status text on its tint" });
    pairs.push({ fg: "text", bg, min: 4.5, use: "body text on a status tint" });
    pairs.push({ fg: "textMuted", bg, min: 4.5, use: "secondary text on a status tint" });
  }
  pairs.push({ fg: "textOnAccent", bg: "accent", min: 4.5, use: "primary button label" });
  pairs.push({ fg: "textOnAccent", bg: "accentHover", min: 4.5, use: "primary button label, hovered" });
  pairs.push({ fg: "textOnInverse", bg: "inverse", min: 4.5, use: "tooltip and inverse chip text" });
  return pairs;
}

export const PAIRS: readonly Pair[] = build();

export interface PairResult extends Pair {
  theme: ThemeName;
  ratio: number;
  ok: boolean;
}

/** All pairs for one theme (or a supplied role set, to prove a lowered token fails). */
export function checkPairs(theme: ThemeName, roles: ColorRoles = PALETTES[theme]): PairResult[] {
  return PAIRS.map((p) => {
    const ratio = contrastRatio(roles[p.fg], roles[p.bg]);
    return { ...p, theme, ratio, ok: ratio >= p.min };
  });
}

export function failingPairs(theme: ThemeName, roles?: ColorRoles): PairResult[] {
  return checkPairs(theme, roles).filter((r) => !r.ok);
}
