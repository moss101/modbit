/**
 * A row's facts from the task's card and the Core's attention view (doc 10 "Cards
 * show"; docs/32 M6.6). Nothing here decides a status class. A card built from a
 * snapshot has not seen the events behind its phase, so the default phase is not
 * shown: a row says a phase only when an event said it.
 */
import { PHASE_LABEL, type TaskCard } from "../model.ts";
import type { AttentionLike } from "../screens.ts";
import type { RowFacts } from "./list-model.ts";

const SAID_BY_STATUS = new Set(["Review the result"]);

export function rowFactsOf(card: TaskCard, attention: readonly AttentionLike[]): RowFacts {
  const d = card.diagnostic;
  const stopped = card.state === "Failed" || card.state === "Waiting";
  const item = attention.find((a) => a.taskId === card.taskId);
  const said = stopped && d ? d.detail || d.userAction || d.code : (item?.reason ?? card.nextAction ?? "");
  const live = card.state === "Running" || card.state === "Waiting" || (card.state === "ReadyForReview" && card.phase !== "reviewing");
  return {
    next: SAID_BY_STATUS.has(said) ? "" : said,
    phase: live && card.phase !== "drafting" && card.phase !== "done" ? PHASE_LABEL[card.phase] : "",
    activeAgents: card.agents.running + card.agents.background,
    totalAgents: card.agents.total,
    risk: card.risk ?? "",
    evidence: card.latestEvidence ?? "",
  };
}
