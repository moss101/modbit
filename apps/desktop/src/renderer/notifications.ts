/**
 * Notification model (PX-023; docs/39 "Notification model"): notifications
 * exist for Needs Attention reasons, completion into ReadyForReview and
 * failure — never routine progress. Reasons on one task collapse into one
 * notification with the single next action; a fleet-wide burst collapses
 * into a count. Every notification deep-links to the screen and state it
 * is about. Derived from the Core's attention view and the reducer's
 * cards; nothing here invents a reason.
 */
import type { TaskCard } from "./model.ts";
import type { AttentionLike } from "./screens.ts";

export type NotificationKind = "attention" | "completion" | "failure";

export interface Notification {
  /** `task:<id>` or `burst`. */
  id: string;
  kind: NotificationKind;
  taskId: string | null;
  title: string;
  reason: string;
  nextAction: string;
  /** Where acting on it lands. */
  deepLink: { screen: "task" | "review" | "fleet"; taskId: string | null };
  /** Reasons coalesced into this one (attention). */
  coalesced: number;
  /** Tasks collapsed into this one (a burst). */
  count?: number;
}

export interface NotificationPreferences {
  /** OS delivery per kind, off until opted in. */
  os: Record<NotificationKind, boolean>;
  /** Quiet hours in local time, `null` = none. */
  quietHours: { start: number; end: number } | null;
}

export const DEFAULT_PREFERENCES: NotificationPreferences = { os: { attention: false, completion: false, failure: false }, quietHours: null };

/** Bursts above this many tasks collapse into one count. */
export const BURST_THRESHOLD = 5;

/** The notifications a fleet warrants right now, one per task. */
export function deriveNotifications(tasks: TaskCard[], attention: AttentionLike[]): Notification[] {
  const out: Notification[] = [];
  const byTask = new Map<string, AttentionLike[]>();
  for (const a of attention) {
    if (!a.taskId) continue;
    const list = byTask.get(a.taskId) ?? [];
    list.push(a);
    byTask.set(a.taskId, list);
  }
  for (const card of tasks) {
    const items = byTask.get(card.taskId) ?? [];
    if (card.state === "Failed") {
      out.push({ id: `task:${card.taskId}`, kind: "failure", taskId: card.taskId, title: card.goalText || card.taskId.slice(0, 8), reason: card.diagnostic ? `${card.diagnostic.class}/${card.diagnostic.code}: ${card.diagnostic.detail}` : (card.nextAction ?? "the run failed"), nextAction: card.diagnostic?.userAction || "read the diagnostic", deepLink: { screen: "task", taskId: card.taskId }, coalesced: 1 });
      continue;
    }
    if (items.length > 0) {
      // Coalesced: every reason, one next action (the first item's — the
      // Core lists items oldest first, so the one that has waited longest).
      out.push({ id: `task:${card.taskId}`, kind: "attention", taskId: card.taskId, title: card.goalText || card.taskId.slice(0, 8), reason: items.map((i) => `${i.kind}: ${i.reason}`).join(" · "), nextAction: items[0]!.action, deepLink: { screen: "task", taskId: card.taskId }, coalesced: items.length });
      continue;
    }
    if (card.state === "ReadyForReview") {
      out.push({ id: `task:${card.taskId}`, kind: "completion", taskId: card.taskId, title: card.goalText || card.taskId.slice(0, 8), reason: card.latestEvidence ?? "the result is ready for review", nextAction: "review the result", deepLink: { screen: "review", taskId: card.taskId }, coalesced: 1 });
    }
  }
  if (out.length > BURST_THRESHOLD) {
    const kinds = new Set(out.map((n) => n.kind));
    return [{ id: "burst", kind: kinds.size === 1 ? out[0]!.kind : "attention", taskId: null, title: `${out.length} tasks need you`, reason: `${out.filter((n) => n.kind === "attention").length} need attention, ${out.filter((n) => n.kind === "completion").length} ready for review, ${out.filter((n) => n.kind === "failure").length} failed`, nextAction: "open the attention list", deepLink: { screen: "fleet", taskId: null }, coalesced: out.reduce((n, x) => n + x.coalesced, 0), count: out.length }];
  }
  return out;
}

/** The notifications that are new since `previous` (by id and reason): what OS delivery considers. */
export function newSince(previous: Notification[], current: Notification[]): Notification[] {
  const seen = new Map(previous.map((n) => [n.id, n.reason] as const));
  return current.filter((n) => seen.get(n.id) !== n.reason);
}

/** Whether OS delivery is due for this notification under the preferences, at `nowHour` (local). */
export function osDeliveryDue(n: Notification, prefs: NotificationPreferences, nowHour: number): boolean {
  if (!prefs.os[n.kind]) return false;
  const q = prefs.quietHours;
  if (!q) return true;
  const inQuiet = q.start <= q.end ? nowHour >= q.start && nowHour < q.end : nowHour >= q.start || nowHour < q.end;
  return !inQuiet;
}
