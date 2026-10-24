/**
 * WCAG 2.x relative luminance and contrast ratio over opaque `#rrggbb` colours (AFW-A12). A translucent token (ZCode's borders,
 * hover and selected fills are alpha colours) is tested by compositing it over the surface it sits on first (`over`), so the
 * ratio is that of the pixels the user sees, never of an unrelated opaque stand-in.
 */

export function parseHex(hex: string): [number, number, number] {
  const m = /^#([0-9a-f]{6})$/i.exec(hex);
  if (!m) throw new Error(`not an opaque #rrggbb colour: ${hex}`);
  const n = parseInt(m[1]!, 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
}

export function toHex(rgb: readonly [number, number, number]): string {
  return `#${rgb.map((c) => Math.round(Math.min(255, Math.max(0, c))).toString(16).padStart(2, "0")).join("")}`;
}

/** Linear interpolation in sRGB: `t` 0 gives `a`, 1 gives `b`. Used to derive the neutral ladder from one base. */
export function mix(a: string, b: string, t: number): string {
  const x = parseHex(a);
  const y = parseHex(b);
  return toHex([x[0] + (y[0] - x[0]) * t, x[1] + (y[1] - x[1]) * t, x[2] + (y[2] - x[2]) * t]);
}

export function luminance(hex: string): number {
  const lin = parseHex(hex).map((v) => {
    const s = v / 255;
    return s <= 0.04045 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
  }) as [number, number, number];
  return 0.2126 * lin[0] + 0.7152 * lin[1] + 0.0722 * lin[2];
}

export function contrastRatio(fg: string, bg: string): number {
  const a = luminance(fg);
  const b = luminance(bg);
  return (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05);
}

/** A parsed colour: 8-bit channels and alpha 0..1. */
export type Rgba = [number, number, number, number];

/** Parses `#rrggbb`, `rgb(r, g, b)` and `rgba(r, g, b, a)` (the literals the ZCode token table holds). */
export function parseColor(color: string): Rgba {
  const hex = /^#([0-9a-f]{6})$/i.exec(color);
  if (hex) {
    const [r, g, b] = parseHex(color);
    return [r, g, b, 1];
  }
  const fn = /^rgba?\(\s*(\d+)\s*,\s*(\d+)\s*,\s*(\d+)\s*(?:,\s*([0-9.]+)\s*)?\)$/i.exec(color);
  if (!fn) throw new Error(`not a #rrggbb, rgb() or rgba() colour: ${color}`);
  return [Number(fn[1]), Number(fn[2]), Number(fn[3]), fn[4] === undefined ? 1 : Number(fn[4])];
}

/** The opaque `#rrggbb` a colour shows over an opaque backdrop (the backdrop itself when the colour is fully transparent). */
export function over(color: string, backdrop: string): string {
  const [r, g, b, a] = parseColor(color);
  const [br, bg, bb] = parseHex(backdrop);
  return toHex([r * a + br * (1 - a), g * a + bg * (1 - a), b * a + bb * (1 - a)]);
}
