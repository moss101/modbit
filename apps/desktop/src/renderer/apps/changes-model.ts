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

// ------------------------------------------- the actions (revert, stage, Commit and push)

/** What each typed refusal of a Changes action means for the person, in Modbit's words. The Core's own detail follows it on screen. */
const REFUSAL_WORDS: Record<string, string> = {
  STALE_REVISION: "The file changed since this view was read. Nothing was written; reload the changes and look again.",
  DIRTY_CONFLICT: "This would fight what git already holds (staged changes or an unfinished merge). Nothing was written.",
  PROTECTED_PATH: "That is a protected path; it is not changed or committed from here.",
  NO_SUCH_CHANGE: "That path has no change to act on any more.",
  UNKNOWN_HUNK: "That hunk is not in the change set any more; reload the changes.",
  UNSUPPORTED_HUNK: "This file's hunks cannot be reverted one by one; revert the whole file instead.",
  CHECKPOINT_FAILED: "No checkpoint could be taken, so nothing was reverted.",
  TASK_RUNNING: "The task is still running; it owns the files until it stops.",
  NO_REMOTE: "This repository has no remote to push to. Add one first.",
  NO_BRANCH: "There is no branch to push (HEAD is detached or the repository has no commit).",
  NOTHING_STAGED: "Nothing is staged and the remote lacks no commit.",
  STALE_STAGE: "The staged files are not the ones this commit was prepared for; reload the changes.",
  STALE_INTENT: "What was approved no longer matches the repository, so nothing was committed. Ask again.",
  PUSH_REJECTED: "The remote rejected the push (it holds commits this branch lacks). The commit stays local.",
  GIT_PUSH: "The push failed. The commit, if one was made, stays local.",
  CREDENTIAL_REFUSED: "The credential broker would not hand out the push token.",
  BAD_PAYLOAD: "That request was not acceptable.",
};

export function refusalWords(code: string): string {
  return REFUSAL_WORDS[code] ?? (code ? "The Core refused this." : "This could not be done.");
}

export type StageState = "none" | "staged" | "partial";

/** Whether a path is in the index in full, in part (it also has edits since) or not at all. */
export function stageState(p: { staged: string; unstaged: string }): StageState {
  if (p.staged === "") return "none";
  return p.unstaged === "" ? "staged" : "partial";
}

/** Whether the toolbar's action is a commit of the staged files or only a push of commits the remote lacks. */
export function commitMode(stagedCount: number, unpushed: number): "commit" | "push" | "none" {
  if (stagedCount > 0) return "commit";
  return unpushed > 0 ? "push" : "none";
}

export function commitLabel(mode: "commit" | "push" | "none", unpushed: number): string {
  if (mode === "push") return `Push ${unpushed} ${unpushed === 1 ? "commit" : "commits"}`;
  return "Commit and push";
}

/** A commit needs a message; a push alone does not. */
export function canCommitPush(mode: "commit" | "push" | "none", message: string, taskRunning: boolean): boolean {
  if (taskRunning || mode === "none") return false;
  return mode === "push" || message.trim().length > 0;
}

/** How an earlier Commit and push stands, in words: an authorization with no result is in doubt, never assumed done. */
export function effectWords(e: { state: string; commit: string; code: string }): string {
  switch (e.state) {
    case "COMPLETED":
      return `Committed ${e.commit.slice(0, 10)} and pushed.`;
    case "FAILED":
      return e.code ? `Failed (${e.code}).` : "Failed.";
    case "DENIED":
      return "Denied; nothing ran.";
    case "AUTHORIZED_IN_DOUBT":
      return "Authorized, and the Core stopped before it reported. Whether it finished is unknown; it is not retried by itself.";
    case "APPROVAL_PENDING":
      return "Waiting for your approval; nothing has run.";
    default:
      return e.state;
  }
}

/** A fresh idempotency key for one revert click: 32 hex characters. */
export function newCommandId(fill: (bytes: Uint8Array<ArrayBuffer>) => void = (b) => void crypto.getRandomValues(b)): string {
  const bytes = new Uint8Array(16);
  fill(bytes);
  return Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");
}
