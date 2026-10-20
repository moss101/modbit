/**
 * The run-mode chooser's view model (REQ-PX-057; docs/65 AFW-F06..F08, AFW-F10):
 * Modbit's own words for the presets and the rules, and the one question the
 * renderer may ask itself: does this move approve more (so the warning dialog
 * comes first)? The Core answers the same question when it records the mode and
 * refuses a move that approves more without the acknowledgement, so this is a
 * convenience for the dialog and never the check.
 */
import type { AllowRuleInfo } from "../../shared/control-types.ts";

export const MODE_COPY: Record<string, { label: string; summary: string }> = {
  ASK: { label: "Ask", summary: "Every protected effect asks you first. This is the default." },
  ALLOWLIST: { label: "Allowlist", summary: "A command that a rule you made covers runs without asking. Everything else asks." },
  ALLOWLIST_SANDBOX: { label: "Allowlist with sandbox", summary: "Also runs without asking what is fully contained in a sandbox, and what a rule covers. Everything else asks." },
  RUN_EVERYTHING: { label: "Run everything (this session)", summary: "Runs every effect inside the task's envelope without asking, except the kinds that always ask. It lasts until Modbit's Core restarts." },
};

export const modeLabel = (mode: string): string => MODE_COPY[mode]?.label ?? mode.replace(/_/g, " ").toLowerCase();

/** The order the Core lists the modes in is least to most approving. */
export function rankOf(mode: string, modes: readonly string[]): number {
  const i = modes.indexOf(mode);
  return i < 0 ? 0 : i;
}

/** Whether moving from one mode to another lets more run without a person (and so needs the acknowledgement); the widest preset always does. */
export function approvesMore(from: string, to: string, modes: readonly string[]): boolean {
  if (to === "RUN_EVERYTHING") return true;
  return rankOf(to, modes) > rankOf(from, modes);
}

/** A pattern typed as words: split on white space. The Core validates it (no wrappers, no bare programs, no operators). */
export function patternFromText(text: string): string[] {
  return text.trim().split(/\s+/).filter((t) => t.length > 0);
}

export const EXPIRY_CHOICES = [
  { id: "never", label: "No expiry", ms: 0 },
  { id: "hour", label: "1 hour", ms: 3_600_000 },
  { id: "day", label: "1 day", ms: 86_400_000 },
  { id: "week", label: "7 days", ms: 7 * 86_400_000 },
  { id: "month", label: "30 days", ms: 30 * 86_400_000 },
] as const;

export const SCOPE_WORDS: Record<string, string> = { TASK: "this task", REPO: "this repository", USER: "all my tasks" };

/** The moment a rule expires, as text. */
export function expiryWords(expiresAtMs: number, now: number): string {
  if (expiresAtMs === 0) return "no expiry";
  if (expiresAtMs <= now) return "expired";
  return `expires ${new Date(expiresAtMs).toLocaleString()}`;
}

/** Rules in the order a person reads them: active first, newest first. */
export function orderRules(rules: readonly AllowRuleInfo[]): AllowRuleInfo[] {
  const rank = (r: AllowRuleInfo) => (r.state === "ACTIVE" ? 0 : 1);
  return [...rules].sort((a, b) => rank(a) - rank(b) || b.createdAtMs - a.createdAtMs);
}

/** A refusal from the Core in plain words (its typed codes keep their meaning). */
export function ruleRefusal(message: string): string {
  const m = message.replace(/^Error invoking remote method '[^']*': (Error: )?/, "").replace(/^BAD_ARGUMENT: /, "");
  return `The Core did not make the rule: ${m}`;
}
