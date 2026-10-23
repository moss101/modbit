/** Pure keyboard navigation for the roving-focus primitives (menu, tabs, list). */

export type Orientation = "vertical" | "horizontal";

/**
 * The index a navigation key moves to, or null when the key is not a
 * navigation key for this orientation. Arrow keys wrap (menus, tabs); lists
 * pass `wrap: false`.
 */
export function navTarget(key: string, current: number, count: number, orientation: Orientation, wrap = true): number | null {
  if (count <= 0) return null;
  const next = orientation === "vertical" ? "ArrowDown" : "ArrowRight";
  const prev = orientation === "vertical" ? "ArrowUp" : "ArrowLeft";
  const clamp = (i: number) => (wrap ? (i + count) % count : Math.min(count - 1, Math.max(0, i)));
  if (key === next) return clamp(current < 0 ? 0 : current + 1);
  if (key === prev) return clamp(current < 0 ? count - 1 : current - 1);
  if (key === "Home") return 0;
  if (key === "End") return count - 1;
  return null;
}

/** Type-ahead: the next enabled label starting with the typed character after the current one (wrapping). */
export function typeAheadTarget(char: string, current: number, labels: readonly string[], disabled: readonly boolean[] = []): number | null {
  const c = char.toLowerCase();
  if (c.length !== 1) return null;
  for (let step = 1; step <= labels.length; step++) {
    const i = (current + step + labels.length) % labels.length;
    if (!disabled[i] && labels[i]!.toLowerCase().startsWith(c)) return i;
  }
  return null;
}

/** The next enabled index in a direction from `from` (wrapping), or -1 when none is enabled. */
export function nextEnabled(from: number, dir: 1 | -1, disabled: readonly boolean[]): number {
  const n = disabled.length;
  for (let step = 1; step <= n; step++) {
    const i = (((from + dir * step) % n) + n) % n;
    if (!disabled[i]) return i;
  }
  return -1;
}
