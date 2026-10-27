/**
 * The fidelity comparator (DR-PX-2026-10-24-016 section 6, PX-142): compares one probe record of ZCode's real component
 * (evidence/zcode-ui/reference) with the record of Modbit's matching element, property by property, and reports every
 * deviation together with the declared exception that covers it. Pure data in, data out, so the gate itself can be unit-tested
 * and mutation-tested; the Electron spec only collects the records and the pixels.
 */

/** One probe record, as `tools/zcode-reference/scripts/probe-lib.mjs` writes it for ZCode and for Modbit alike. */
export interface ProbeRecord {
  id: string;
  spec: string;
  slot: string;
  variant: string;
  size: string;
  tag: string;
  text: string;
  rect: { x?: number; y?: number; w: number; h: number };
  style: Record<string, string>;
}

/** The properties compared. Layout-only properties (display, alignment) are covered by the compared size. */
export const COMPARED_PROPS = [
  "color", "backgroundColor", "borderTopColor", "borderRightColor", "borderBottomColor", "borderLeftColor",
  "fontFamily", "fontSize", "fontWeight", "lineHeight", "letterSpacing", "textTransform", "textDecorationLine",
  "gap", "paddingTop", "paddingRight", "paddingBottom", "paddingLeft",
  "borderTopWidth", "borderRightWidth", "borderBottomWidth", "borderLeftWidth",
  "borderTopLeftRadius", "borderTopRightRadius", "borderBottomRightRadius", "borderBottomLeftRadius",
  "boxShadow", "opacity", "cursor", "backdropFilter", "backgroundImage",
  "transitionProperty", "transitionDuration", "transitionTimingFunction",
] as const;
export type ComparedProp = (typeof COMPARED_PROPS)[number];

/** A declared exception (DR section 5.2): the records it covers, the properties it may differ in, and why. */
export interface Exception {
  id: `ZDX-0${1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9}`;
  /** Matches the reference record key (`spec|slot|variant|size`) and the theme. */
  applies: (key: string, theme: "light" | "dark") => boolean;
  props: readonly ComparedProp[];
  why: string;
}

export interface Deviation {
  key: string;
  theme: "light" | "dark";
  index: number;
  prop: ComparedProp | "width" | "height" | "text";
  reference: string;
  actual: string;
  /** The exception that covers it, or null: an undeclared deviation fails the gate. */
  exception: Exception["id"] | null;
}

const px = (v: string): number | null => (/^-?\d+(\.\d+)?(e[+-]?\d+)?px$/.test(v) ? Number.parseFloat(v) : null);
const flat = (v: string) => v.replace(/\s+/g, " ").trim();
/** A shadow without the transparent zero-size layers Tailwind's variable stack always adds: they paint nothing. */
const noZeroLayers = (v: string) => flat(v.replace(/rgba\(0, 0, 0, 0\) 0px 0px 0px 0px,? ?/g, "")).replace(/,$/, "");

/** Two computed values agree: lengths to half a pixel, colours and everything else exactly (after whitespace). */
export function sameValue(a: string, b: string, prop?: string): boolean {
  if (prop === "boxShadow") return noZeroLayers(a) === noZeroLayers(b);
  const x = px(a);
  const y = px(b);
  // Chromium clamps an unbounded radius (a pill) to a build-dependent float ceiling (1.67772e+07 or 3.35544e+07); both paint the same pill.
  if (x !== null && y !== null && x >= 1e6 && y >= 1e6 && /Radius$/.test(prop ?? "")) return true;
  if (x !== null && y !== null) return Math.abs(x - y) <= 0.5;
  return flat(a) === flat(b);
}

/** All deviations between one reference record and the matching Modbit record. */
export function compareRecord(reference: ProbeRecord, actual: ProbeRecord, theme: "light" | "dark", exceptions: readonly Exception[], key: string, index: number, ignore: readonly string[] = []): Deviation[] {
  const out: Deviation[] = [];
  const covered = (prop: ComparedProp | "width" | "height" | "text"): Exception["id"] | null => {
    for (const e of exceptions) if (e.applies(key, theme) && (e.props as readonly string[]).includes(prop)) return e.id;
    return null;
  };
  const push = (prop: Deviation["prop"], ref: string, got: string) => out.push({ key, theme, index, prop, reference: ref, actual: got, exception: covered(prop) });
  if (!ignore.includes("width") && Math.abs(reference.rect.w - actual.rect.w) > 0.5) push("width", String(reference.rect.w), String(actual.rect.w));
  if (!ignore.includes("height") && Math.abs(reference.rect.h - actual.rect.h) > 0.5) push("height", String(reference.rect.h), String(actual.rect.h));
  if (!ignore.includes("text") && reference.text.replace(/\s+/g, "") !== actual.text.replace(/\s+/g, "")) push("text", reference.text, actual.text);
  for (const prop of COMPARED_PROPS) {
    if (ignore.includes(prop)) continue;
    const a = reference.style[prop] ?? "";
    const b = actual.style[prop] ?? "";
    // A border colour paints only where the border has width (ZCode's preflight leaves a 0 px currentColor border on everything).
    const side = /^border(Top|Right|Bottom|Left)Color$/.exec(prop)?.[1];
    if (side && reference.style[`border${side}Width`] === "0px" && actual.style[`border${side}Width`] === "0px") continue;
    // A gap only lays out two or more children, and a button's own flex gap is not visible with one.
    if (prop === "gap" && (reference.style[prop] === "normal" || actual.style[prop] === "normal") && reference.rect.w === actual.rect.w && reference.text === "") continue;
    if (!sameValue(a, b, prop)) push(prop, a, b);
  }
  return out;
}

/** The key of a record without its `#n` suffix: `spec|slot|variant|size`. */
export const keyOf = (id: string): string => id.slice(0, id.lastIndexOf("#"));
