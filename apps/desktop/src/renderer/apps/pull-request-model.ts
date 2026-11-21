/**
 * The Pull request pane's words (REQ-PX-202; DR-PX-2026-11-11-021 MWS-10), pure: each function words one answer of the Core and
 * returns nothing where the Core gave nothing. A request's state (open, merged, closed) and its checks are what the Core last read
 * through the forge adapter (REQ-PX-211, MWS-17): a state that was not read is not shown, and every state says when it was read.
 */
import type { PullRequestAckView, ReviewBundleView } from "../../preload/preload.ts";
import type { PullRequestStateView } from "../../shared/conversation-types.ts";

export type RequestTone = "open" | "merged" | "closed" | "draft" | "unread";
export type ChecksTone = "ok" | "bad" | "pending" | "none";

/**
 * A task's pull request in words (REQ-PX-211), from the Core's record and its latest read: the number, the state (or that it was
 * not read), the checks, and how the read was made. Null when the task opened none. The time of the read is the caller's to word
 * (it changes as the clock moves).
 */
export function requestStateWords(pr: PullRequestStateView | undefined | null): {
  number: string;
  state: string;
  tone: RequestTone;
  checks: string;
  checksTone: ChecksTone;
  failing: string;
  read: boolean;
  by: string;
} | null {
  if (!pr || pr.number <= 0) return null;
  const read = pr.readAtMs > 0 && pr.state !== "";
  const tone: RequestTone = !read ? "unread" : pr.state === "MERGED" ? "merged" : pr.state === "CLOSED" ? "closed" : pr.draft ? "draft" : "open";
  const state = { unread: "not read yet", merged: "merged", closed: "closed", draft: "draft", open: "open" }[tone];
  let checks = "";
  let checksTone: ChecksTone = "none";
  if (read) {
    switch (pr.checks) {
      case "NONE":
        checks = "no checks";
        break;
      case "PENDING":
        checks = `${pr.checksPending} of ${pr.checksTotal} ${pr.checksTotal === 1 ? "check" : "checks"} running`;
        checksTone = "pending";
        break;
      case "FAILING":
        checks = `${pr.checksFailing} of ${pr.checksTotal} ${pr.checksTotal === 1 ? "check" : "checks"} failing`;
        checksTone = "bad";
        break;
      case "PASSING":
        checks = `${pr.checksTotal === 1 ? "the check" : `all ${pr.checksTotal} checks`} passing`;
        checksTone = "ok";
        break;
      case "UNAVAILABLE":
        checks = "checks could not be read";
        break;
    }
  }
  return {
    number: `#${pr.number}`,
    state,
    tone,
    checks,
    checksTone,
    failing: pr.failingChecks.join(", "),
    read,
    by: pr.trigger === "SCHEDULE" ? "by the Core's schedule" : pr.trigger === "REQUEST" ? "when asked" : "",
  };
}

/** One line for a row's label, a card or a rail: "pull request #7, merged, all 2 checks passing"; "" when there is none. */
export function requestStateLine(pr: PullRequestStateView | undefined | null): string {
  const w = requestStateWords(pr);
  if (!w) return "";
  return [`pull request ${w.number}`, w.state, w.checks].filter(Boolean).join(", ");
}

/** Whether the task's changes can become a pull request now, or why not (the Core refuses one before the task is ready for review). */
export function canOpenRequest(taskState: string): true | string {
  return taskState === "ReadyForReview" || taskState === "Completed" ? true : "The task has to be ready for review before its changes can become a pull request.";
}

/** What the Core answered when the request was opened or updated from this window; null for any other answer. */
export function requestWords(pr: PullRequestAckView | null): { title: string; where: string; head: string; receipts: string } | null {
  if (!pr || (pr.status !== "OPENED" && pr.status !== "UPDATED")) return null;
  return {
    title: `${pr.status === "OPENED" ? "Opened" : "Updated"} as #${pr.number}`,
    where: pr.url,
    head: `${pr.branch ? `${pr.branch} · ` : ""}head ${pr.headSha.slice(0, 12)}`,
    receipts: `${pr.receipts} ${pr.receipts === 1 ? "receipt" : "receipts"}${pr.replayed ? ", answered from the record" : ""}`,
  };
}

export type CheckTone = "ok" | "bad" | "none";

/** One check run the Core recorded for the pushed commit, as the forge reported it. */
export function checkWords(c: ReviewBundleView["ciEvidence"][number]): { name: string; result: string; tone: CheckTone; detail: string } {
  const result = c.conclusion || c.status;
  return { name: c.name, result: result.replace(/_/g, " "), tone: c.conclusion === "success" ? "ok" : c.conclusion === "failure" || c.conclusion === "timed_out" || c.conclusion === "cancelled" ? "bad" : "none", detail: `run ${c.runId} · ${c.commit.slice(0, 12)}` };
}

/** A check run the Core refused as evidence, with its reason. */
export const refusedWords = (r: ReviewBundleView["ciRejected"][number]): string => `${r.name}: not counted, ${r.reason.replace(/_/g, " ").toLowerCase()} (run on ${r.headSha.slice(0, 12)})`;

/** What became of one comment: passed to the agent (and whether it has answered), or ignored with the Core's reason. */
export function commentWords(t: ReviewBundleView["reviewComments"][number]): { author: string; where: string; outcome: string } {
  const outcome = t.disposition === "STEERED" ? (t.answered ? "passed to the agent, which has answered" : "passed to the agent, not answered yet") : `ignored: ${t.reason.replace(/_/g, " ").toLowerCase()}`;
  return { author: `@${t.author}`, where: t.path ? `${t.path}${t.line !== "0" && t.line !== "" ? `:${t.line}` : ""}` : "", outcome: `${outcome}${t.reportedBack ? ", reported back on the request" : ""}` };
}

/** The Core's refusal to read the forge, in words; the code stays beside them. */
export function forgeErrorWords(message: string): string {
  const plain = message.replace(/^Error invoking remote method '[^']*': (Error: )?/, "");
  if (/NO_PULL_REQUEST/.test(plain)) return "The Core has no pull request for this task, so there is nothing to read yet (NO_PULL_REQUEST).";
  return plain;
}

/** An id as the key events and answers share (hex, with or without hyphens, either case). */
const idKey = (id: string): string => id.replace(/-/g, "").toLowerCase();

/**
 * What became of the approval the request waits on, read from the events this window was given: approved, denied, expired, or
 * null while no event of that approval has said. Nothing is asked of the Core for it.
 */
export function approvalOutcome(events: readonly { eventType: string; aggregateId: string; payload: unknown }[], approvalId: string): "approved" | "denied" | "expired" | null {
  if (!approvalId) return null;
  for (let i = events.length - 1; i >= 0; i--) {
    const e = events[i]!;
    if (idKey(e.aggregateId) !== idKey(approvalId)) continue;
    if (e.eventType === "ApprovalExpired") return "expired";
    if (e.eventType === "ApprovalResolved") return typeof e.payload === "object" && e.payload !== null && (e.payload as Record<string, unknown>)["approved"] === true ? "approved" : "denied";
  }
  return null;
}

/**
 * REQ-PX-211: recognises the Core's archive after merge among the events this window is given. The Core writes it only right after
 * the read that first found the task's request merged, on the same task, so an archive whose task's previous event is that read is
 * the Core's; any other archive is not. Returns the task id for such an archive, null for every other event. Presentation only:
 * the archive is the Core's record either way, and nothing is decided from this.
 */
export function mergeArchiveWatcher(): (raw: unknown) => string | null {
  const last = new Map<string, { eventType: string; merged: boolean }>();
  return (raw) => {
    const e = (raw ?? {}) as { taskId?: unknown; eventType?: unknown; payload?: unknown };
    if (typeof e.taskId !== "string" || !e.taskId || typeof e.eventType !== "string") return null;
    const before = last.get(e.taskId);
    const merged = e.eventType === "ForgePullRequestStateRead" && typeof e.payload === "object" && e.payload !== null && (e.payload as Record<string, unknown>)["state"] === "MERGED";
    last.delete(e.taskId);
    last.set(e.taskId, { eventType: e.eventType, merged });
    if (last.size > 256) last.delete(last.keys().next().value as string);
    return e.eventType === "ConversationArchived" && before?.eventType === "ForgePullRequestStateRead" && before.merged ? e.taskId : null;
  };
}
