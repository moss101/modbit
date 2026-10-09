/**
 * The apply-back flow's pure model (REQ-PX-068; docs/65 AFW-J08): which step
 * the Core's answer puts the modal in, which choices it offers, what must be
 * typed before an overwrite, and which choices may be remembered. Nothing
 * here decides what an apply does to the checkout. The Core classifies every
 * path, lists the options with their flags and refuses an unconfirmed
 * overwrite whatever this file did; the model only reads those facts and
 * keeps Cancel the default, so a person who presses Enter changes nothing.
 */
import type { ApplyAckInfo, ApplyConflictInfo, ApplyInputArgs, ApplyOptionInfo } from "../../shared/project-types.ts";

export type FlowKind = "apply" | "undo";

export type Step =
  | { kind: "working"; what: string }
  /** The Core asked for the person's approval of exactly this intent; nothing has been written. */
  | { kind: "approve"; flow: FlowKind; ack: ApplyAckInfo; request: ApplyInputArgs }
  /** The Core found conflicts and made no change: it lists the paths and the options it offers. */
  | { kind: "conflict"; ack: ApplyAckInfo; conflict: ApplyConflictInfo; refusal: string }
  /** The checkout was changed (or restored) and the Core says how. */
  | { kind: "done"; flow: FlowKind; ack: ApplyAckInfo; /** Undo and apply: the undo is done and the apply is still to be asked for. */ reapply: boolean }
  /** The Core stopped without changing the checkout, and says why. */
  | { kind: "stopped"; flow: FlowKind; ack: ApplyAckInfo; reason: "STALE" | "DENIED" | "CANCELLED" | "REFUSED" | "UNDO_CONFLICT" | "UNKNOWN" }
  /** The command itself was refused with a typed code (no worktree, the task is running, ...). */
  | { kind: "error"; code: string; detail: string };

/** The step the Core's answer puts the flow in. A conflict view with no change is a conflict step; an approval is asked for before anything is written. */
export function stepOf(ack: ApplyAckInfo, flow: FlowKind, request: ApplyInputArgs): Step {
  switch (ack.status) {
    case "APPROVAL_PENDING":
      // Undo-and-apply asks first for the approval of the undo.
      return { kind: "approve", flow: request.option === "UNDO_AND_APPLY" ? "undo" : flow, ack, request };
    case "CONFLICT":
      return ack.conflict ? { kind: "conflict", ack, conflict: ack.conflict, refusal: "" } : { kind: "stopped", flow, ack, reason: "UNKNOWN" };
    case "REFUSED":
      // An overwrite the Core would not take (the files were not typed): back to the choices with its reason, nothing changed.
      return ack.conflict ? { kind: "conflict", ack, conflict: ack.conflict, refusal: ack.detail || ack.code } : { kind: "stopped", flow, ack, reason: "REFUSED" };
    case "UNDONE":
      return { kind: "done", flow: "undo", ack, reapply: request.option === "UNDO_AND_APPLY" };
    case "APPLIED":
    case "NOTHING_TO_APPLY":
      return { kind: "done", flow, ack, reapply: false };
    case "STALE":
    case "DENIED":
    case "CANCELLED":
    case "UNDO_CONFLICT":
      return { kind: "stopped", flow, ack, reason: ack.status };
    default:
      return { kind: "stopped", flow, ack, reason: "UNKNOWN" };
  }
}

/** The options as the Core listed them, in its order; Cancel is always first and the default. */
export function optionsOf(c: ApplyConflictInfo): ApplyOptionInfo[] {
  const cancel = c.options.find((o) => o.option === "CANCEL");
  return [...(cancel ? [cancel] : []), ...c.options.filter((o) => o.option !== "CANCEL")];
}

export const OPTION_WORDS: Record<string, { label: string; what: string }> = {
  CANCEL: { label: "Cancel", what: "Change nothing. Your checkout stays exactly as it is." },
  MERGE_MANUALLY: { label: "Merge manually", what: "Apply what merges cleanly and leave conflict markers in the conflicting files for you to resolve." },
  STASH: { label: "Stash, then apply", what: "Set your uncommitted work aside in a Git stash, apply the task's result, and tell you how to bring your work back." },
  OVERWRITE: { label: "Overwrite conflicting files", what: "Replace the conflicting files with the task's version. Undo can bring your bytes back." },
  FULL_OVERWRITE: { label: "Full overwrite", what: "Replace every file the apply and your uncommitted work touch with the task's result. Undo can bring your bytes back." },
  UNDO_AND_APPLY: { label: "Undo and apply", what: "Undo the earlier apply of this worktree first, then apply the task's result." },
};

export const optionLabel = (o: string): string => OPTION_WORDS[o]?.label ?? o.replace(/_/g, " ").toLowerCase();

/** The choice selected when the modal opens, and the one Enter takes. It is Cancel whatever the Core's list says: a destructive default fails the qualification. */
export const DEFAULT_OPTION = "CANCEL";

/** What the person must type to confirm an option: the Core's list of paths, or nothing. */
export const requiredPaths = (o: ApplyOptionInfo): string[] => (o.needsConfirmation ? o.confirmPaths : []);

/** The paths typed (one per line; surrounding space ignored; a trailing slash style does not matter). */
export function typedPaths(text: string): string[] {
  return text
    .split(/\r?\n/)
    .map((l) => l.trim().replace(/\\/g, "/"))
    .filter((l) => l.length > 0);
}

/** The required paths still not typed. Only a guard for the button: the Core compares them itself and refuses what is missing. */
export function missingPaths(required: readonly string[], text: string): string[] {
  const typed = new Set(typedPaths(text));
  return required.filter((p) => !typed.has(p.replace(/\\/g, "/")));
}

/** A non-destructive choice that is not Cancel or Undo-and-apply may be remembered for the task; a destructive one never is. */
export const canRemember = (o: ApplyOptionInfo): boolean => !o.destructive && o.option !== "CANCEL" && o.option !== "UNDO_AND_APPLY";

/** The reasons the Core stopped, in words; its own detail follows. */
export const STOP_WORDS: Record<string, string> = {
  STALE: "The checkout or the worktree changed since the plan you were shown, so nothing was written. Review it again.",
  DENIED: "The approval was denied. Nothing was written.",
  CANCELLED: "Cancelled. Nothing was changed.",
  REFUSED: "The Core refused to apply this. Nothing was changed.",
  UNDO_CONFLICT: "A file was edited after the apply, so the undo changed nothing. The files that diverged are listed.",
  UNKNOWN: "The Core gave an answer this window does not know. Nothing was assumed.",
};

/** The one-line result of a finished apply or undo. */
export function doneWords(flow: FlowKind, ack: ApplyAckInfo): string {
  if (ack.status === "NOTHING_TO_APPLY") return ack.detail || "Your checkout already holds every change the task made.";
  if (flow === "undo") return `Restored ${ack.appliedPaths.length} ${ack.appliedPaths.length === 1 ? "file" : "files"} to the bytes they had before the apply.`;
  const n = ack.appliedPaths.length;
  const base = `Applied ${n} ${n === 1 ? "file" : "files"} to your checkout.`;
  if (ack.unresolvedPaths.length > 0) return `${base} ${ack.unresolvedPaths.length} ${ack.unresolvedPaths.length === 1 ? "file has" : "files have"} conflict markers to resolve.`;
  return base;
}
