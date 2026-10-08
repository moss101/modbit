/** Pure helpers for the Changes app (REQ-PX-048), covered by node tests. */

/** The Core takes a review decision only on a task that is ready for review (`DecideReview`: NOT_REVIEWABLE otherwise). */
export function isReviewable(taskState: string): boolean {
  return taskState === "ReadyForReview";
}

export function lineKind(line: string): "add" | "del" | "ctx" {
  if (line.startsWith("+")) return "add";
  if (line.startsWith("-")) return "del";
  return "ctx";
}

/** Whether 1-based `line` falls in any of the Core's changed ranges (a flat list of start,end pairs, inclusive). */
export function rangesContain(ranges: readonly number[], line: number): boolean {
  for (let i = 0; i + 1 < ranges.length; i += 2) if (line >= ranges[i]! && line <= ranges[i + 1]!) return true;
  return false;
}

export function changeSummary(files: readonly { hunks: readonly unknown[]; binary: boolean }[]): string {
  if (files.length === 0) return "No changes";
  const hunks = files.reduce((n, f) => n + f.hunks.length, 0);
  return `${files.length} ${files.length === 1 ? "file" : "files"}, ${hunks} ${hunks === 1 ? "hunk" : "hunks"}`;
}
