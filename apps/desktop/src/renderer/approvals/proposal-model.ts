/**
 * The mode-switch proposal's view model (REQ-PX-055; docs/65 AFW-D06): pure
 * functions over the Core's views. The words are Modbit's own. Nothing here
 * decides anything: the agent's request is data the Core recorded, only the
 * person's answer (or the Core's 15 s expiry, which is always a skip) ends it,
 * and the mode changes only in the Core. This file chooses what to say, and
 * reads the time left against the Core's clock, not this machine's.
 */
import type { ModeProposalInfo, ModeProposalListInfo } from "../../shared/control-types.ts";

/** The window the Core gives the person (docs/65 AFW-D06). */
export const PROPOSAL_WINDOW_S = 15;

/** A settled proposal stays on screen as a note for this long after the Core decided it. */
export const RESULT_VISIBLE_MS = 10 * 60 * 1000;

const MODE_WORDS: Record<string, string> = { AGENT: "Agent", PLAN: "Plan", DEBUG: "Debug", MULTITASK: "Multitask", ASK: "Ask" };

export const modeWords = (m: string): string => MODE_WORDS[m] ?? m.toLowerCase();

export const pendingOf = (list: readonly ModeProposalInfo[]): ModeProposalInfo | null => [...list].reverse().find((p) => p.status === "PENDING") ?? null;

/** The most recent proposal the Core has settled, while it is recent by the Core's clock. */
export function latestSettled(l: ModeProposalListInfo | null): ModeProposalInfo | null {
  if (!l) return null;
  const done = l.proposals.filter((p) => p.status !== "PENDING");
  const last = done[done.length - 1];
  if (!last) return null;
  return l.nowMs - last.decidedAtMs <= RESULT_VISIBLE_MS ? last : null;
}

/** Whole seconds left, read against the Core's clock (`skewMs` = the Core's now minus this machine's, at the read). Never negative. */
export function secondsLeft(p: ModeProposalInfo, localNowMs: number, skewMs: number): number {
  return Math.max(0, Math.ceil((p.expiresAtMs - (localNowMs + skewMs)) / 1000));
}

export function proposalTitle(p: ModeProposalInfo): string {
  return `The agent suggests switching to ${modeWords(p.toMode)}`;
}

export interface Settled {
  headline: string;
  detail: string;
  tone: "ok" | "warn";
}

/** What a settled proposal says, in the Core's typed reason. A skip is never worded as an acceptance. */
export function settledWords(p: ModeProposalInfo): Settled {
  const from = modeWords(p.fromMode);
  const to = modeWords(p.toMode);
  if (p.status === "ACCEPTED") return { headline: `Switched to ${to}.`, detail: "You accepted the agent's suggestion. The Core applies the new mode at the agent's next safe step.", tone: "ok" };
  if (p.status === "DECLINED") return { headline: `Kept ${from}.`, detail: "You skipped the suggestion. The mode did not change.", tone: "ok" };
  switch (p.outcomeReason) {
    case "UNANSWERED_15S":
      return { headline: "Skipped.", detail: `No answer within ${PROPOSAL_WINDOW_S} seconds, so the suggestion was skipped. The mode is still ${from}.`, tone: "warn" };
    case "CORE_RESTARTED":
      return { headline: "Skipped.", detail: `The Core restarted before you answered, so the suggestion was skipped. The mode is still ${from}.`, tone: "warn" };
    case "TASK_ENDED":
      return { headline: "Skipped.", detail: "The task ended before you answered, so the suggestion was skipped.", tone: "warn" };
    case "MODE_CHANGED":
      return { headline: "Dropped.", detail: `The mode had already changed from ${from} before you answered, so the suggestion no longer applied.`, tone: "warn" };
    default:
      return { headline: "Skipped.", detail: `The suggestion ended without a change. The mode is still ${from}.`, tone: "warn" };
  }
}

/** A human line for a refused answer, in the Core's typed words. */
export function proposalRefusalWords(message: string): string {
  const m = message.replace(/^Error invoking remote method '[^']*': (Error: )?/, "");
  if (/PROPOSAL_NOT_PENDING/.test(m)) return "That suggestion already ended, so it can no longer be answered. Nothing changed.";
  if (/MODE_CHANGED/.test(m)) return "The mode changed before your answer, so the suggestion was dropped. Nothing changed.";
  if (/UNKNOWN_PROPOSAL/.test(m)) return "The Core does not know that suggestion any more.";
  return `The Core refused: ${m.replace(/^BAD_ARGUMENT: /, "")}`;
}
