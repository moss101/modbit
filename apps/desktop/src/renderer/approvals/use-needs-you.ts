/**
 * Needs you (REQ-PX-206; the owner's review of 2026-11-17: "Fix the count first"): the one place that reads what waits on the
 * person and counts it. The task list's Inbox badge and Inbox's "Needs you · N" both take `count` from here, so they cannot
 * disagree, and `count` is exactly the number of rows Needs you draws. Every input is a Core read, never the renderer's task
 * store: the session's pending approvals (`approvals:dock`), the pending mode proposals of its live tasks (`modeproposals:list`)
 * and the Core's attention items (`attention:list`). How the rows are made, and so what is counted:
 *
 * - one row per pending approval (the Core's approval list carries what it will do);
 * - the Core's attention items of kind APPROVAL are those same approvals, so they are not listed or counted again;
 * - one row per other attention item (a question, a stall, a failure …), even when its task also has an approval;
 * - one row per pending mode proposal (the Core holds proposals outside its attention view).
 */
import { useCallback } from "react";
import type { AttentionItem } from "../../preload/preload.ts";
import type { DockApprovalView, ModeProposalInfo } from "../../shared/control-types.ts";
import { needsYouCount, needsYouOthers } from "./inbox-model.ts";
import { useDockApprovals } from "./use-dock-approvals.ts";
import { useSessionProposals } from "./use-inbox.ts";

export interface NeedsYou {
  approvals: DockApprovalView[];
  proposals: ModeProposalInfo[];
  /** The attention items listed beside the approvals. */
  others: AttentionItem[];
  /** The number of rows Needs you draws: the badge and the tab show this number. */
  count: number;
  skewMs: number;
  refresh: () => void;
}

/** An approval's own events, and a task's end (which closes what it was waiting on): the only events after which the list can differ. */
const approvalEvent = (e: { eventType?: string }): boolean => /^Approval|^Task(Completed|Cancelled|Failed|ReadyForReview)$/.test(e.eventType ?? "");

/** Read on the events that can change each list (see use-inbox.ts and `approvalEvent`), one read at a time, so it adds little to the bridge. */
export function useNeedsYou(sessionId: string | null, connected: boolean, attention: readonly AttentionItem[]): NeedsYou {
  const { approvals, refresh: refreshApprovals } = useDockApprovals(sessionId, null, connected, approvalEvent);
  const pendingSeen = useSessionProposals(sessionId, connected);
  const refreshProposals = pendingSeen.refresh;
  const refresh = useCallback(() => {
    refreshApprovals();
    refreshProposals();
  }, [refreshApprovals, refreshProposals]);
  const others = needsYouOthers(attention);
  return {
    approvals,
    proposals: pendingSeen.proposals,
    others,
    count: needsYouCount(attention, approvals, pendingSeen.proposals),
    skewMs: pendingSeen.skewMs,
    refresh,
  };
}

