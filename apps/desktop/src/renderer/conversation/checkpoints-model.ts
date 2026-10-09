/**
 * The checkpoint surface's view model (REQ-PX-062; docs/65 AFW-G01..G07): pure
 * functions over the Core's checkpoint list and rewind preview. The Core records
 * a checkpoint at each turn boundary, previews what a restore would change,
 * restores exactly (recording the state before it, which is what Redo restores)
 * and refuses what is no longer exact. This file maps a message or a turn to a
 * checkpoint the Core listed, counts what a preview says and words it.
 */
import type { CheckpointInfo, CheckpointListInfo, RewindEntryInfo, RewindPreviewInfo } from "../../shared/control-types.ts";

/** A turn id as 32 lowercase hex characters (the transcript names it hyphenated, the checkpoint list as bytes). */
export const normTurn = (id: string): string => id.replace(/-/g, "").toLowerCase();

const isCommitted = (c: CheckpointInfo): boolean => c.status === "CURRENT" || c.status === "SUPERSEDED";

/** The checkpoint that is the boundary of each turn, by turn id; the latest wins if a turn has more than one. */
export function byTurn(list: CheckpointListInfo | null): Map<string, CheckpointInfo> {
  const out = new Map<string, CheckpointInfo>();
  for (const c of list?.checkpoints ?? []) {
    if (!c.turnId || !isCommitted(c)) continue;
    const prev = out.get(normTurn(c.turnId));
    if (!prev || c.epoch > prev.epoch) out.set(normTurn(c.turnId), c);
  }
  return out;
}

/**
 * The checkpoint that was current when a message was sent: the newest turn-boundary checkpoint recorded before the
 * message's log offset. Restoring it puts the workspace back to how it was when the message was sent. Null for the first
 * message of a task (nothing was recorded before it).
 */
export function checkpointBefore(list: CheckpointListInfo | null, messageOffset: string): CheckpointInfo | null {
  let at: bigint;
  try {
    at = BigInt(messageOffset);
  } catch {
    return null;
  }
  let best: CheckpointInfo | null = null;
  for (const c of list?.checkpoints ?? []) {
    if (!c.turnId || !isCommitted(c)) continue;
    let o: bigint;
    try {
      o = BigInt(c.eventOffset);
    } catch {
      continue;
    }
    if (o < at && (!best || c.epoch > best.epoch)) best = c;
  }
  return best;
}

/**
 * The pre-restore checkpoint a Redo would restore: the newest one the Core keeps for that reason, provided no turn has
 * been recorded since (work that moved on makes the redo inexact, and the Core refuses it with a typed reason either way).
 * `used` are the ones this window already redid.
 */
export function redoCandidate(list: CheckpointListInfo | null, used: ReadonlySet<string>): CheckpointInfo | null {
  const all = (list?.checkpoints ?? []).filter(isCommitted);
  const pre = all.filter((c) => c.retention.includes("PRE_RESTORE") && !used.has(c.checkpointId)).sort((a, b) => b.epoch - a.epoch)[0];
  if (!pre) return null;
  const moved = all.some((c) => c.turnId && c.epoch > pre.epoch);
  return moved ? null : pre;
}

/**
 * The checkpoint "Undo all" restores: the earliest turn boundary the Core recorded, provided it recorded no changed or
 * removed file, which is the state before the agent changed anything. When the first turn already changed files there
 * is no recorded state before the agent started, and Undo all is not offered.
 */
export function undoAllTarget(list: CheckpointListInfo | null): CheckpointInfo | null {
  const first = (list?.checkpoints ?? []).filter((c) => c.turnId && isCommitted(c)).sort((a, b) => a.epoch - b.epoch)[0];
  return first && first.files === 0 && first.removed === 0 ? first : null;
}

export interface ChangeCounts {
  /** Files written back to the checkpoint's content. */
  written: number;
  /** Files removed (they did not exist at the checkpoint, or the checkpoint has them deleted). */
  deleted: number;
  /** Files put back to the last commit. */
  reverted: number;
  unchanged: number;
}

export function changeCounts(entries: readonly RewindEntryInfo[]): ChangeCounts {
  const c: ChangeCounts = { written: 0, deleted: 0, reverted: 0, unchanged: 0 };
  for (const e of entries) {
    if (e.action === "WRITE") c.written++;
    else if (e.action === "DELETE" || e.action === "REMOVE_UNTRACKED") c.deleted++;
    else if (e.action === "REVERT_TO_HEAD") c.reverted++;
    else c.unchanged++;
  }
  return c;
}

export const changedEntries = (entries: readonly RewindEntryInfo[]): RewindEntryInfo[] => entries.filter((e) => e.action !== "UNCHANGED");

const plural = (n: number, one: string, many: string): string => `${n} ${n === 1 ? one : many}`;

/** What a restore will do, in a sentence, from the Core's preview. */
export function restoreSentence(counts: ChangeCounts): string {
  const parts: string[] = [];
  if (counts.written) parts.push(`${plural(counts.written, "file is", "files are")} put back to the checkpoint's content`);
  if (counts.reverted) parts.push(`${plural(counts.reverted, "file is", "files are")} put back to the last commit (a file that is new since then is removed)`);
  if (counts.deleted) parts.push(`${plural(counts.deleted, "file is", "files are")} deleted`);
  if (parts.length === 0) return "Nothing in the workspace differs from this checkpoint, so nothing would change.";
  return `${parts.join("; ")}.`;
}

export const ACTION_WORDS: Record<string, string> = {
  WRITE: "put back to the checkpoint",
  DELETE: "deleted",
  REMOVE_UNTRACKED: "deleted (it did not exist then)",
  REVERT_TO_HEAD: "put back to the last commit (removed if it is new)",
  UNCHANGED: "unchanged",
};

/** Typed refusals of a preview or a restore, in words (the codes are the Core's). */
export function refusalWords(code: string, detail: string): string {
  const base: Record<string, string> = {
    HASH_MISMATCH: "A file changed after this dialog was opened, so nothing was restored. Open it again to see the current state.",
    STALE_EPOCH: "The task's checkpoints moved on after this dialog was opened, so nothing was restored. Open it again.",
    TASK_RUNNING: "The agent is working. Stop or pause it before restoring files.",
    REDO_SUPERSEDED: "Work was done after that restore, so redoing it would no longer give back exactly the state it replaced. Nothing was changed.",
    NO_PRE_RESTORE: "There is no recorded state to redo.",
    HEAD_DRIFT: "The repository moved to another commit since this checkpoint, so it can no longer be restored exactly. Nothing was changed.",
    NO_WORKSPACE: "This task has no workspace to restore.",
    RESTORE_IN_FLIGHT: "Another restore is running. Try again in a moment.",
  };
  const known = base[code];
  return known ? `${known}${detail && !known.includes(detail) ? ` (${detail})` : ""}` : `The Core refused (${code.toLowerCase().replace(/_/g, " ")})${detail ? `: ${detail}` : ""}.`;
}

/** The files that need the person's choice before a restore may overwrite them: edited since the agent's last turn and about to change. */
export function editedAndAffected(p: RewindPreviewInfo): string[] {
  const touched = new Set(changedEntries(p.entries).map((e) => e.path));
  return p.userEdited.filter((path) => touched.has(path));
}

/** The expected-content hashes a restore sends: what the dialog showed at each path it will change. */
export function expectedHashes(p: RewindPreviewInfo): { path: string; contentHash: string }[] {
  return changedEntries(p.entries).map((e) => ({ path: e.path, contentHash: e.currentHash }));
}

export type KeepChoice = "keep" | "overwrite";

/** The paths to keep as they are, from the person's per-file choices; every edited file needs a choice before Continue. */
export function keepPathsOf(choices: Readonly<Record<string, KeepChoice | undefined>>): string[] {
  return Object.entries(choices)
    .filter(([, c]) => c === "keep")
    .map(([p]) => p);
}

export const allChosen = (edited: readonly string[], choices: Readonly<Record<string, KeepChoice | undefined>>): boolean => edited.every((p) => choices[p] !== undefined);

/** A command id as 32 hex characters, kept by the dialog so a retried Continue restores once. */
export function newCommandId(random: (n: number) => Uint8Array = (n) => crypto.getRandomValues(new Uint8Array(n))): string {
  return [...random(16)].map((b) => b.toString(16).padStart(2, "0")).join("");
}
