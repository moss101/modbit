/**
 * Inbox's view model (REQ-PX-206; DR-PX-2026-11-11-021 MWS-13): pure words and counts over what the Core says. Nothing here
 * decides anything, and no count is taken from the renderer's own task store: Needs you counts the Core's attention items and
 * the Core's pending proposals; Inbox's count in the task list is the Core's attention count (tasks with an open item); the
 * unread count of Updates is the Core's read markers against main's record.
 */
import type { IconName } from "@modbit/ui";
import type { DockApprovalView, ModeProposalInfo } from "../../shared/control-types.ts";
import type { UpdateKind } from "../../shared/inbox-types.ts";
import { commandLine, humanTitle, isIrreversible } from "./model.ts";
import { modeWords } from "./proposal-model.ts";

/** Inbox's count in the task list: the tasks the Core lists an attention item for. */
export function inboxCount(attention: readonly { taskId: string }[]): number {
  return new Set(attention.map((i) => i.taskId)).size;
}

/** Needs you's count: every item it lists, each one the Core's (a pending approval, another attention item, a proposal still pending). */
export function needsYouCount(attention: readonly { kind: string }[], approvals: readonly unknown[], proposals: readonly unknown[]): number {
  // Approvals are listed from the Core's approval list; the attention items that name them are not counted twice.
  return approvals.length + attention.filter((i) => i.kind !== "APPROVAL").length + proposals.length;
}

/** What an approval asks, as a question in words. */
export function askWords(a: Pick<DockApprovalView, "toolName" | "intent">): string {
  if (a.intent.argv && a.intent.argv.length > 0) return "Run this command?";
  return `${humanTitle(a.toolName)}?`;
}

/** The exact thing it will do, where there is one: the command line. */
export function exactLine(a: Pick<DockApprovalView, "intent">): string {
  return a.intent.argv && a.intent.argv.length > 0 ? commandLine(a.intent.argv) : a.intent.command;
}

const EFFECT_TAG: Record<string, string> = {
  ReadOnly: "Reads only",
  ReversibleWrite: "Changes files",
  ProtectedWrite: "Protected change",
  ExternalSideEffect: "Reaches outside",
  SecretAccess: "Uses a secret",
  Destructive: "Cannot be undone",
};
/** The effect in plain words (the Core's class stays on the card as data). */
export function effectTag(effectClass: string): string {
  if (EFFECT_TAG[effectClass]) return EFFECT_TAG[effectClass]!;
  return isIrreversible(effectClass) ? "Cannot be undone" : "Asks first";
}

/** A mode proposal as a question: "Switch to Plan mode?". */
export function proposalQuestion(p: Pick<ModeProposalInfo, "toMode">): string {
  return `Switch to ${modeWords(p.toMode)} mode?`;
}

export const UPDATE_WORDS: Record<UpdateKind, { title: string; icon: IconName }> = {
  READY_FOR_REVIEW: { title: "Ready to review", icon: "git-compare" },
  CHILD_FINISHED: { title: "Child task finished", icon: "list-tree" },
  AUTOMATION_RAN: { title: "Automation ran", icon: "clock" },
  FAILED: { title: "Task failed", icon: "x" },
};

/** When, in a few characters: "now", "14m", "6h", "Yesterday", then the date. */
export function whenWords(atMs: number, nowMs: number): string {
  const d = Math.max(0, nowMs - atMs);
  if (d < 60_000) return "now";
  if (d < 3_600_000) return `${Math.floor(d / 60_000)}m`;
  const at = new Date(atMs);
  const now = new Date(nowMs);
  const dayStart = new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime();
  if (atMs >= dayStart) return `${Math.floor(d / 3_600_000)}h`;
  if (atMs >= dayStart - 86_400_000) return "Yesterday";
  return at.toLocaleDateString(undefined, { month: "short", day: "numeric" });
}

/** The full time, for the accessible name and the tooltip. */
export function fullTime(atMs: number): string {
  return new Date(atMs).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" });
}

export type InboxView = "all" | "needs" | "updates";
