/**
 * Diff presentation helpers (PX-024): the split view of a unified hunk.
 */
/** Unified hunk lines → side-by-side rows: context on both sides, removals on
 *  the left, additions on the right, each run of removals paired with the
 *  run of additions that follows it. */
export function splitRows(lines: string[]): { left: string | null; right: string | null }[] {
  const rows: { left: string | null; right: string | null }[] = [];
  let i = 0;
  while (i < lines.length) {
    const l = lines[i]!;
    if (l.startsWith("-")) {
      const dels: string[] = [];
      while (i < lines.length && lines[i]!.startsWith("-")) dels.push(lines[i++]!);
      const adds: string[] = [];
      while (i < lines.length && lines[i]!.startsWith("+")) adds.push(lines[i++]!);
      for (let k = 0; k < Math.max(dels.length, adds.length); k++) rows.push({ left: dels[k] ?? null, right: adds[k] ?? null });
    } else if (l.startsWith("+")) {
      rows.push({ left: null, right: l });
      i++;
    } else {
      rows.push({ left: l, right: l });
      i++;
    }
  }
  return rows;
}

