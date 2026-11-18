/**
 * The one decision path for a pending approval (REQ-PX-058, REQ-PX-205, REQ-PX-206). The task's card, home's waiting list and Inbox decide
 * through here: one typed preload call that names the intent hash of what was on screen, which the Core refuses for any other
 * (INTENT_MISMATCH). A list adds its own rule first: what `decidableFromList` refuses (an effect that cannot be undone, native
 * control, a budget raise) is never sent from a list, whatever asked for it; the list opens the task instead.
 */
import type { DockApprovalView } from "../../shared/control-types.ts";
import { decidableFromList } from "./model.ts";

export type Decided = { status: string };

/** Allow once (`approve`) or Skip, bound to the hash the card showed. */
export async function resolveShown(sessionId: string, a: Pick<DockApprovalView, "approvalId" | "intentHash">, approve: boolean, why: string): Promise<Decided> {
  return window.modbit.resolveApproval(sessionId, a.approvalId, approve, why, a.intentHash);
}

/** The lists away from a task that may decide: home's waiting list (REQ-PX-205) and Inbox's Needs you (REQ-PX-206). */
export type DecidingList = "home" | "inbox";
const LIST_NAME: Record<DecidingList, string> = { home: "home's waiting list", inbox: "Inbox" };

/** A decision made from a list away from the task. Refuses, without asking the Core, what only its task may decide. */
export async function decideFromList(sessionId: string, a: DockApprovalView, approve: boolean, list: DecidingList = "home"): Promise<Decided> {
  const rule = decidableFromList(a);
  if (!rule.ok) throw new Error(`NOT_FROM_A_LIST: ${rule.why}`);
  return resolveShown(sessionId, a, approve, `${approve ? "approved" : "skipped"} from ${LIST_NAME[list]}`);
}

/**
 * The one answer to the agent's mode proposal (REQ-PX-055, REQ-PX-206): the task's docked card and Inbox both answer through
 * here. The Core refuses a late answer (PROPOSAL_NOT_PENDING) and changes the mode only on Switch.
 */
export async function decideProposal(sessionId: string, taskId: string, proposalId: string, accept: boolean): Promise<void> {
  await window.modbit.decideModeProposal(sessionId, taskId, proposalId, accept);
}
