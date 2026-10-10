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
