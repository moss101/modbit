/**
 * Main's side of the reviewer's findings (REQ-PX-214, DR-PX-2026-11-11-021 MWS-18): two typed handlers, one per command on a
 * finding. Each validates what the renderer sent (a finding id of plain characters, the 64-hex digest of the result the person was
 * looking at, a reason of at most 500 characters), takes the session lease and forwards one typed Core command. Reading the findings
 * is not here: they ride the review bundle (`review:bundle`). Nothing is decided and nothing is kept here: whether a finding exists,
 * whether it was dismissed, whether the result is the latest and what the queued input says are the Core's.
 */
import type { DismissFindingAnswer, FixFindingAnswer } from "../shared/review-findings-types.ts";
import type { Registrar } from "./conversation-ipc.ts";

const bad = (what: string): never => {
  throw new Error(`BAD_ARGUMENT: ${what}`);
};

const FINDING_ID = /^[A-Za-z0-9._~-]{1,48}$/;
const DIGEST = /^[0-9a-f]{64}$/;
/** The longest reason a dismissal records, in characters (the Core's own bound). */
export const MAX_REASON = 500;

export function requireFindingId(v: unknown): string {
  if (typeof v !== "string" || !FINDING_ID.test(v)) return bad("finding id must be 1..48 letters, digits or . _ ~ -");
  return v;
}
export function requireResultDigest(v: unknown): string {
  if (typeof v !== "string" || !DIGEST.test(v)) return bad("result digest must be 64 hex characters");
  return v;
}
export function requireReason(v: unknown): string {
  if (typeof v !== "string") return bad("a reason is required");
  const t = v.trim();
  if (t.length === 0) return bad("a dismissal records why: a reason is required");
  if (t.length > MAX_REASON) return bad(`the reason is at most ${MAX_REASON} characters`);
  return t;
}

export function registerReviewFindingsHandlers(r: Registrar): void {
  r.handle("review:fixFinding", async (...a): Promise<FixFindingAnswer> => {
    const sid = r.sessionId(a[0]);
    const tid = r.taskId(a[1]);
    const id = requireFindingId(a[2]);
    const digest = requireResultDigest(a[3]);
    const c = r.client();
    await r.lease(c, sid);
    const v = await c.fixReviewFinding(sid, tid, id, digest);
    return { findingId: v.findingId, inputId: v.inputId, taskState: v.taskState };
  });
  r.handle("review:dismissFinding", async (...a): Promise<DismissFindingAnswer> => {
    const sid = r.sessionId(a[0]);
    const tid = r.taskId(a[1]);
    const id = requireFindingId(a[2]);
    const digest = requireResultDigest(a[3]);
    const reason = requireReason(a[4]);
    const c = r.client();
    await r.lease(c, sid);
    const v = await c.dismissReviewFinding(sid, tid, id, digest, reason);
    return { findingId: v.findingId, dismissedBy: v.dismissedBy, reason: v.reason };
  });
}
