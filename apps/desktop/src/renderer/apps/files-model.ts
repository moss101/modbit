/** Pure helpers for the Files app (REQ-PX-048), covered by node tests. */

/** The Core's typed refusal codes for a path, and what each says to a person. */
const REFUSALS: [string, string][] = [
  ["OUTSIDE_ROOT", "That path is outside the task's workspace, so it is not shown."],
  ["PROTECTED", "That file is protected by the workspace's path policy and is never opened here."],
  ["NOT_FOUND", "That file no longer exists in the workspace."],
  ["NOT_A_FILE", "That is not a regular file."],
  ["NO_WORKSPACE", "This task has no workspace to browse."],
  ["BAD_ARGUMENT", "That is not a workspace-relative path."],
];

export function refusalText(message: string): string {
  for (const [code, text] of REFUSALS) if (message.includes(code)) return text;
  return `The workspace could not be read: ${message}`;
}

export function formatBytes(n: number | string): string {
  const v = Number(n);
  if (v < 1024) return `${v} B`;
  if (v < 1024 * 1024) return `${(v / 1024).toFixed(1)} KiB`;
  return `${(v / (1024 * 1024)).toFixed(1)} MiB`;
}

/** What the viewer shows for a file the Core would not send as text. */
export function withheldText(status: string, size: string): string | null {
  if (status === "BINARY") return `Binary file (${formatBytes(size)}): not shown as text.`;
  if (status === "TOO_LARGE") return `Too large to show inline (${formatBytes(size)}): the Files app shows files up to 256 KiB.`;
  return null;
}

/** The directory part of a workspace-relative path ("" at the root). */
export function parentOf(path: string): string {
  const i = path.lastIndexOf("/");
  return i < 0 ? "" : path.slice(0, i);
}
