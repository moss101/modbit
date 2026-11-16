/**
 * The one decision path for a pending approval (REQ-PX-058, REQ-PX-205). The task's card and home's waiting list both decide
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

/** A decision made from a list away from the task. Refuses, without asking the Core, what only its task may decide. */
export async function decideFromList(sessionId: string, a: DockApprovalView, approve: boolean): Promise<Decided> {
  const rule = decidableFromList(a);
  if (!rule.ok) throw new Error(`NOT_FROM_A_LIST: ${rule.why}`);
  return resolveShown(sessionId, a, approve, approve ? "approved from home's waiting list" : "skipped from home's waiting list");
}
