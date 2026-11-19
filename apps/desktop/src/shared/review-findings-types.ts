/**
 * The isolated reviewer's findings as the review bundle carries them (REQ-PX-214, DR-PX-2026-11-11-021 MWS-18): plain data of the
 * Core's ReviewFindingView and the reviewer fields of ReviewBundle, and the answers of the two commands on a finding. Every fact is
 * the Core's; in particular `verified` is the Core's own check against the tree, never the reviewer's word.
 */

/** One finding of the reviewer's latest recorded result. */
export interface ReviewerFindingView {
  /** The reviewer's id for it, else F<n> by position; the commands name a finding by it. */
  findingId: string;
  /** CRITICAL | HIGH | MEDIUM | LOW. */
  severity: string;
  category: string;
  path: string;
  /** 0 when the finding names no line. */
  line: number;
  claim: string;
  suggestedAction: string;
  /** The Core found the finding's path in the tree at the candidate revision. */
  verified: boolean;
  /** Why it was not verified; empty when it was. */
  unsupportedReason: string;
  dismissed: boolean;
  /** Who set it aside, as `user:<id>`. */
  dismissedBy: string;
  dismissedReason: string;
  /** How many times a person asked the agent to fix it. */
  fixRequests: number;
}

/** The reviewer's latest result on the task: "" (none), CURRENT, STALE (the candidate changed after it) or UNREADABLE (shows nothing). */
export interface ReviewerResultView {
  state: string;
  /** PASS | REVISE | BLOCK | UNCERTAIN; empty when unreadable. */
  verdict: string;
  confidence: number;
  summary: string;
  candidateRevision: string;
  /** The result's digest; the commands on a finding name it so they cannot act on a result the person did not see. */
  resultDigest: string;
  findings: ReviewerFindingView[];
  questions: string[];
  note: string;
}

export interface FixFindingAnswer {
  findingId: string;
  inputId: string;
  /** The task's state after the command; a candidate waiting for review goes back to work. */
  taskState: string;
}

export interface DismissFindingAnswer {
  findingId: string;
  dismissedBy: string;
  reason: string;
}
