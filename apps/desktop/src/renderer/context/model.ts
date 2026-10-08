/**
 * The context ring, the usage summary and the docked trays' view model
 * (REQ-PX-060; docs/65 AFW-H01..H06): pure functions over the Core's accounting.
 * The Core counts the tokens, apportions them over the nine categories so they
 * sum to the total, and says which window the model has; this file formats
 * them and decides only what to say and when a tray is worth docking.
 */
import type { AccountingInfo } from "../../shared/control-types.ts";

export const CATEGORY_LABELS: Record<string, { label: string; detail: string }> = {
  SYSTEM: { label: "System instructions", detail: "Modbit's own instructions to the model" },
  TOOLS: { label: "Tool definitions", detail: "the tools the model may call" },
  RULES: { label: "Rules and instructions", detail: "project and user rules in force" },
  SKILLS: { label: "Skills", detail: "skills loaded for this turn" },
  MCP: { label: "External tool definitions", detail: "tools from connected servers" },
  MEMORY: { label: "Memory", detail: "remembered items injected into the request" },
  SUMMARY: { label: "Summarised conversation", detail: "earlier turns folded into summaries" },
  SUBAGENTS: { label: "Subagent definitions", detail: "the agents this one may delegate to" },
  CONVERSATION: { label: "Conversation", detail: "the messages and tool results themselves" },
};

export const categoryLabel = (c: string): string => CATEGORY_LABELS[c]?.label ?? c.replace(/_/g, " ").toLowerCase();

export const formatCount = (s: string | number | bigint): string => {
  try {
    return BigInt(s).toLocaleString("en-US");
  } catch {
    return String(s);
  }
};

export type RingLevel = "unknown" | "ok" | "warn" | "danger";

/** The ring's level: unknown until the Core has compiled a request and knows the window. */
export function ringLevel(a: AccountingInfo | null): RingLevel {
  if (!a || !a.available || a.windowSource === "UNKNOWN" || a.windowTokens === "0") return "unknown";
  if (a.usedBp >= 9500) return "danger";
  if (a.usedBp >= 8000) return "warn";
  return "ok";
}

/** Whole percent of the window used, or null when the window is unknown. */
export function percentUsed(a: AccountingInfo | null): number | null {
  return ringLevel(a) === "unknown" ? null : Math.min(100, Math.round(a!.usedBp / 100));
}

export function ringLabel(a: AccountingInfo | null): string {
  const p = percentUsed(a);
  if (a && a.available && p === null) return `${formatCount(a.totalTokens)} tokens, window unknown`;
  if (p === null) return "no request yet";
  const level = ringLevel(a);
  return `${p}% of ${formatCount(a!.windowTokens)} tokens${level === "danger" ? ", nearly full" : level === "warn" ? ", getting full" : ""}`;
}

/** The categories' tokens summed: the Core promises this equals the total. */
export function categorySum(a: AccountingInfo): bigint {
  return a.categories.reduce((s, c) => s + BigInt(c.tokens), 0n);
}

export const sumsToTotal = (a: AccountingInfo): boolean => categorySum(a) === BigInt(a.totalTokens);

/** A share of the total in whole tenths of a percent, for the bars. */
export const shareLabel = (bp: number): string => `${(bp / 100).toFixed(bp % 100 === 0 ? 0 : 1)}%`;

export interface BudgetBar {
  label: string;
  used: bigint;
  cap: bigint;
  /** 0..100, rounded down; a cap of 0 is "none". */
  percent: number;
  unit: "minor" | "ms";
}

/** The Core's budget caps beside the context numbers; a cap of 0 is no cap. */
export function budgetBars(a: AccountingInfo | null): BudgetBar[] {
  const b = a?.budgets;
  if (!b) return [];
  const bars: BudgetBar[] = [];
  const cost = BigInt(b.maxCostMinor);
  if (cost > 0n) bars.push({ label: "Cost cap", used: BigInt(b.spentMinor), cap: cost, percent: pct(BigInt(b.spentMinor), cost), unit: "minor" });
  const wall = BigInt(b.maxWallMs);
  if (wall > 0n) bars.push({ label: "Time cap", used: BigInt(b.wallMsUsed), cap: wall, percent: pct(BigInt(b.wallMsUsed), wall), unit: "ms" });
  return bars;
}

const pct = (used: bigint, cap: bigint): number => (cap === 0n ? 0 : Number(used >= cap ? 100n : (used * 100n) / cap));

/** Near a limit: the context window or a budget cap is at 80% or more. */
export function nearLimit(a: AccountingInfo | null): boolean {
  if (!a) return false;
  if (ringLevel(a) === "warn" || ringLevel(a) === "danger") return true;
  return budgetBars(a).some((b) => b.percent >= 80);
}

export type UsageSetting = "auto" | "always" | "never";

/** The usage summary shows near a limit (auto), always, or never (AFW-H06). */
export const usageVisible = (setting: UsageSetting, near: boolean): boolean => setting === "always" || (setting === "auto" && near);

export function formatWall(ms: string | number | bigint): string {
  const n = Number(ms);
  if (n < 1000) return `${n} ms`;
  const s = Math.round(n / 1000);
  return s < 60 ? `${s} s` : `${Math.floor(s / 60)} min ${s % 60} s`;
}

// ---------------------------------------------------------------- the trays

export interface TrayInputs {
  coreState: "starting" | "connected" | "restarting" | "failed";
  /** The Core has been connected at least once in this window (a start-up is not "offline"). */
  everConnected: boolean;
  coreReason: string;
  /** The open task's typed diagnostic (its card's, or the Core's status), or null. */
  diagnostic: { code: string; detail: string; userAction: string } | null;
  taskId: string | null;
}

export type TraySpec =
  | { kind: "offline"; id: "core-offline"; title: string; text: string }
  | { kind: "budget"; id: "budget-exhausted"; title: string; text: string; taskId: string }
  | { kind: "policy"; id: "policy-blocked"; title: string; text: string; userAction: string };

/** Which attention trays are due. The Core's typed codes decide; nothing is inferred from prose. */
export function trayPlan(i: TrayInputs): TraySpec[] {
  const out: TraySpec[] = [];
  if (i.coreState !== "connected" && (i.everConnected || i.coreState === "failed")) {
    out.push({
      kind: "offline",
      id: "core-offline",
      title: i.coreState === "failed" ? "Modbit's Core is not running" : "Reconnecting to Modbit's Core",
      text: `${i.coreState === "failed" ? "The Core stopped and has not come back." : "The Core is restarting."} ${i.coreReason ? `(${i.coreReason}) ` : ""}Open conversations stay readable. Anything that needs the Core fails until it is back, and nothing shown is new progress.`,
    });
  }
  const d = i.diagnostic;
  if (d && i.taskId && d.code === "BUDGET_EXHAUSTED") {
    out.push({ kind: "budget", id: "budget-exhausted", title: "This task reached a limit", text: d.detail || "A budget ran out and the run paused with its partial work kept.", taskId: i.taskId });
  }
  // The policy tray is due for the Core's typed MODEL_NOT_ALLOWED and for an organisation block the router reports as a refused route
  // (the Core gives that refusal no code of its own yet, so its own words are the only marker).
  if (d && (d.code === "MODEL_NOT_ALLOWED" || (d.code === "ROUTE_REFUSED" && /blocked by organization policy/i.test(d.detail)))) {
    out.push({ kind: "policy", id: "policy-blocked", title: "Policy blocks the selected model", text: d.detail || "Your organisation's policy does not allow the model this task was set to use. The turn did not start, and nothing was sent.", userAction: d.userAction });
  }
  return out;
}

/** The decimal count a budget form sends: whole digits only, or null when the text is not a count. */
export function countFrom(text: string): string | null {
  const t = text.trim();
  return /^\d{1,15}$/.test(t) ? t : null;
}
