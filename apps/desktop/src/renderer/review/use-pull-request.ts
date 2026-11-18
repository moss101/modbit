/**
 * The pull request from a task's candidate, and the forge's checks and comments for it (PX-007, PX-127), as the Review and the
 * Pull request pane (REQ-PX-202) both drive them: one protected effect behind the Core's approval, and two reads the Core makes
 * of the forge when asked. The renderer calls no forge. What is known of the request is what the Core answered to this window:
 * the Core has no read of a request's state (PX-211), so nothing is said about one this window did not open or update.
 */
import { useCallback, useEffect, useRef, useState } from "react";
import type { PullRequestAckView } from "../../preload/preload.ts";

/**
 * The request the Core last answered with for each task in this window, once it was opened or updated (memory only; gone on
 * reload). It lets the Review and the pane say the same thing; it is the Core's answer at that moment, not the request's state now.
 */
const answered = new Map<string, PullRequestAckView>();
const remember = (taskId: string, ack: PullRequestAckView) => {
  if (ack.status !== "OPENED" && ack.status !== "UPDATED") return;
  answered.delete(taskId);
  answered.set(taskId, ack);
  if (answered.size > 64) answered.delete(answered.keys().next().value as string);
};

const plain = (e: unknown): string => (e as Error).message.replace(/^Error invoking remote method '[^']*': (Error: )?/, "");

export interface PullRequestFlow {
  /** APPROVAL_PENDING (decide the intent shown), OPENED or UPDATED (the number and URL), DENIED (the branch stays local), FAILED; null before any answer. */
  pr: PullRequestAckView | null;
  busy: boolean;
  error: string | null;
  /** Asks the Core to open (or update) the request from `revision`: the first answer is the approval to decide. */
  open: (update: boolean, revision: string) => Promise<void>;
  /** Decides the pending approval with its exact intent; approved, the same call then pushes and opens under it. */
  decide: (approve: boolean, fallbackRevision: string) => Promise<void>;
  /**
   * The pending approval was answered somewhere else (the docked approval card): approved, the same call now pushes and opens
   * under it; denied, the attempt ends and the branch stays local. Nothing is decided here.
   */
  settled: (approved: boolean, fallbackRevision: string) => Promise<void>;
}

export function usePullRequest(sessionId: string, taskId: string, from: string): PullRequestFlow {
  const [pr, setPrState] = useState<PullRequestAckView | null>(() => answered.get(taskId) ?? null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  // The revision the pull request names: the one first asked with, which is the one an acceptance records as the candidate.
  const candidateRevision = useRef<string | null>(null);
  useEffect(() => setPrState(answered.get(taskId) ?? null), [taskId]);
  const setPr = useCallback(
    (ack: PullRequestAckView) => {
      remember(taskId, ack);
      setPrState(ack);
    },
    [taskId],
  );
  const open = async (update: boolean, revision: string) => {
    if (busy) return;
    setBusy(true);
    setError(null);
    try {
      candidateRevision.current ??= revision;
      setPr(await window.modbit.openPullRequest(sessionId, taskId, candidateRevision.current, update));
    } catch (e) {
      setError(plain(e));
    } finally {
      setBusy(false);
    }
  };
  const finish = async (approved: boolean, fallbackRevision: string, pending: PullRequestAckView, denial: string) => {
    if (approved) {
      // Approved: the same call now pushes and opens, under that approval.
      setPr(await window.modbit.openPullRequest(sessionId, taskId, candidateRevision.current ?? fallbackRevision, false));
    } else {
      // Denied: terminal for this attempt; the branch stays local. A later "Try again" is a new attempt with a new approval.
      setPr({ ...pending, status: "DENIED", detail: denial, receipts: 0 });
    }
  };
  const decide = async (approve: boolean, fallbackRevision: string) => {
    if (!pr || pr.status !== "APPROVAL_PENDING" || busy) return;
    setBusy(true);
    setError(null);
    try {
      const r = await window.modbit.resolveApproval(sessionId, pr.approvalId, approve, approve ? `approved from ${from}` : `denied from ${from}`, pr.intentHash);
      await finish(approve, fallbackRevision, pr, `approval ${r.approvalId.slice(0, 8)} denied at offset ${r.offset}: the branch stays local, nothing reached the forge`);
    } catch (e) {
      setError(plain(e));
    } finally {
      setBusy(false);
    }
  };
  const settled = async (approved: boolean, fallbackRevision: string) => {
    if (!pr || pr.status !== "APPROVAL_PENDING" || busy) return;
    setBusy(true);
    setError(null);
    try {
      await finish(approved, fallbackRevision, pr, `approval ${pr.approvalId.slice(0, 8)} was denied: the branch stays local, nothing reached the forge`);
    } catch (e) {
      setError(plain(e));
    } finally {
      setBusy(false);
    }
  };
  return { pr, busy, error, open, decide, settled };
}

export interface ForgeReads {
  busy: boolean;
  /** What the last read recorded, in counts. */
  note: string | null;
  error: string | null;
  /** Asks the Core to read the forge's check runs or the request's comments, then runs `reload`. */
  ingest: (what: "ci" | "comments") => Promise<boolean>;
}

/** PX-127: the forge's CI runs and the pull request's comments. The Core reads the forge; the surface asks it to and shows what it recorded. */
export function useForgeReads(sessionId: string, taskId: string, reload: () => Promise<void>): ForgeReads {
  const [busy, setBusy] = useState(false);
  const [note, setNote] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const ingest = async (what: "ci" | "comments"): Promise<boolean> => {
    if (busy) return false;
    setBusy(true);
    setError(null);
    setNote(null);
    try {
      if (what === "ci") {
        const r = await window.modbit.ingestCiResults(sessionId, taskId);
        setNote(`read ${r.checks} check run(s) for ${r.commit.slice(0, 12)}${r.refused > 0 ? `, refused ${r.refused} for another commit` : ""} (offset ${r.offset})`);
      } else {
        const r = await window.modbit.ingestReviewComments(sessionId, taskId);
        setNote(`${r.steered} comment(s) steer the task, ${r.ignored} ignored, ${r.alreadyTaken} already taken (offset ${r.offset})`);
      }
      await reload();
      return true;
    } catch (e) {
      setError((e as Error).message);
      return false;
    } finally {
      setBusy(false);
    }
  };
  return { busy, note, error, ingest };
}
