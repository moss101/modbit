/** Pure helpers for the Evidence app (REQ-PX-048), covered by node tests. */

/** Basis points as a percentage with one decimal ("12.5%"). */
export function percent(bp: number): string {
  return `${(bp / 100).toFixed(1)}%`;
}

/** A category id as a label ("tool_definitions" -> "Tool definitions"). */
export function categoryLabel(category: string): string {
  const words = category.replace(/[_-]+/g, " ").trim().toLowerCase();
  return words ? words[0]!.toUpperCase() + words.slice(1) : category;
}

export interface AccountingLike {
  totalTokens: string;
  totalSource: string;
  windowTokens: string;
  windowSource: string;
  usedBp: number;
  model: string;
}

/** The used-over-window line: the window is the registry's or the endpoint's, and unknown is said, not guessed. */
export function windowLine(a: AccountingLike): string {
  const total = `${a.totalTokens} tokens (${a.totalSource === "PROVIDER_REPORTED" ? "reported by the provider" : "estimated"})`;
  if (a.windowSource === "UNKNOWN" || a.windowTokens === "0") return `${total}${a.model ? ` for ${a.model}` : ""}; the model's context window is not known.`;
  return `${total} of a ${a.windowTokens}-token window (${percent(a.usedBp)})${a.model ? ` for ${a.model}` : ""}.`;
}

export interface RepairAttemptLike {
  ordinal: number;
  failureSignature: string;
  hypothesis: string;
  intendedFix: string;
  outcome: string;
  reverted: boolean;
  escalation: string;
}

/** One repair attempt as a line: what was tried and what came of it, never a result the Core did not record. */
export function repairLine(a: RepairAttemptLike): string {
  if (a.outcome === "ESCALATED") {
    return `Stopped after ${a.ordinal} ${a.ordinal === 1 ? "attempt" : "attempts"}: ${a.escalation || "the loop refused another attempt"}.`;
  }
  const state = a.outcome === "" ? "in progress" : a.outcome.toLowerCase();
  const tried = a.hypothesis || a.intendedFix || "no hypothesis recorded";
  return `Attempt ${a.ordinal} (${state}${a.reverted ? ", reverted" : ""}): ${tried}`;
}

/** The summary above the list: how many attempts ran, how many settled the failure. */
export function repairSummary(attempts: RepairAttemptLike[]): string {
  const ran = attempts.filter((a) => a.outcome !== "ESCALATED");
  const resolved = ran.filter((a) => a.outcome === "RESOLVED").length;
  const stopped = attempts.some((a) => a.outcome === "ESCALATED");
  const base = `${ran.length} repair ${ran.length === 1 ? "attempt" : "attempts"}, ${resolved} resolved`;
  return stopped ? `${base}; the loop stopped and asked for you` : base;
}
