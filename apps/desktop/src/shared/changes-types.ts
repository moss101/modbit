/**
 * The plain shapes the Changes app's actions cross the preload bridge as
 * (REQ-PX-048). Main converts the Core's protobuf messages into these (64-bit
 * counts as decimal strings, ids as hex); the renderer shows them and decides
 * none of it: whether a revert is stale, a path protected, a conflict dirty,
 * what a commit will contain and whether the push ran are the Core's answers
 * (docs/81). Main runs no Git.
 */

export interface ChangedPathInfo {
  path: string;
  /** The working tree against HEAD: A, M or D. */
  status: string;
  /** The index against HEAD: "" | A | M | D. */
  staged: string;
  /** The working tree against the index: "" | A | M | D. */
  unstaged: string;
  /** sha256 of the working-tree bytes; "" when the file is deleted. */
  fileRevision: string;
  unmerged: boolean;
  protectedPath: boolean;
}

export type ChangeEffectState = "APPROVAL_PENDING" | "AUTHORIZED_IN_DOUBT" | "COMPLETED" | "FAILED" | "DENIED";

export interface ChangeEffectInfo {
  toolCallId: string;
  state: ChangeEffectState | string;
  approvalId: string;
  intentHash: string;
  message: string;
  paths: string[];
  branch: string;
  remote: string;
  pushOnly: boolean;
  headBefore: string;
  commit: string;
  authorizedReceipt: string;
  resultReceipt: string;
  resultStatus: string;
  code: string;
  detail: string;
}

export interface ChangesStateInfo {
  workspaceRevision: string;
  branch: string;
  head: string;
  remote: string;
  remoteUrl: string;
  paths: ChangedPathInfo[];
  unpushed: number;
  taskRunning: boolean;
  mergeInProgress: boolean;
  stagedDigest: string;
  effects: ChangeEffectInfo[];
  identity: string;
}

export interface RevertInput {
  path: string;
  wholeFile: boolean;
  hunkIndex?: number;
  expectedFileRevision: string;
  commandId?: string;
}

export interface RevertAckInfo {
  path: string;
  action: string;
  hunkIndex: number;
  workspaceRevision: string;
  previousRevision: string;
  fileRevision: string;
  beforeHash: string;
  checkpointId: string;
  offset: string;
  replayed: boolean;
}

export interface StageAckInfo {
  paths: string[];
  staged: boolean;
  stagedDigest: string;
  offset: string;
}

export interface CommitPushInput {
  message: string;
  remote?: string;
  expectedHead?: string;
  expectedStagedDigest?: string;
}

export interface CommitPushAckInfo {
  status: "APPROVAL_PENDING" | "COMPLETED" | "FAILED" | "DENIED" | string;
  approvalId: string;
  intentHash: string;
  toolCallId: string;
  message: string;
  paths: string[];
  branch: string;
  remote: string;
  remoteUrl: string;
  headBefore: string;
  commit: string;
  pushed: boolean;
  pushOnly: boolean;
  effectReceiptIds: string[];
  replayed: boolean;
  code: string;
  detail: string;
  stagedDigest: string;
}

export interface DecideHunkInput {
  path: string;
  hunkIndex: number;
  decision: "ACCEPT" | "REJECT" | "AUTO_ACCEPT";
  expectedFileRevision: string;
  expectedHunkDigest?: string;
  reason?: string;
  commandId?: string;
}

/** The Core's typed answer to a per-hunk decision (PX-103): applied (ACCEPTED or REJECTED); a stale or refused decision arrives as a refusal with the Core's code. */
export interface DecideHunkAckInfo {
  result: "ACCEPTED" | "REJECTED" | string;
  path: string;
  hunkIndex: number;
  hunkDigest: string;
  decisionId: string;
  attributedCallId: string;
  attributedTool: string;
  runLive: boolean;
  /** What the Core will tell the agent at its next round boundary ("" for an accept). */
  agentNotice: string;
  fileRevision: string;
  beforeHash: string;
  checkpointId: string;
  workspaceRevision: string;
  offset: string;
  replayed: boolean;
}
