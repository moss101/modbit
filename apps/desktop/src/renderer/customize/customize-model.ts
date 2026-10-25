/** Pure helpers for the Customize view (REQ-PX-106), covered by node tests. Nothing here decides: the Core refuses what it will not do. */
import type { ExtensionRowInfo, HookRow, SkillRow, SkillsView } from "../../shared/customize-types.ts";

/** Trust states in which a skill already reaches a model (or never can): there is nothing to trust. */
const NOT_TRUSTABLE = new Set(["SYSTEM", "SIGNED", "TRUSTED_BY_OWNER", "DEV_FLAG", "FORBIDDEN", "REVOKED"]);

/** Whether the view offers "Trust" on a skill. The control sends the hash this row showed; the Core binds the decision to it. */
export function offersTrust(s: Pick<SkillRow, "trust" | "scope">): boolean {
  return s.scope !== "SYSTEM" && !NOT_TRUSTABLE.has(s.trust);
}

/** Whether the view offers "Revoke": only the owner's own recorded decision can be withdrawn here. */
export function offersRevoke(s: Pick<SkillRow, "trust">): boolean {
  return s.trust === "TRUSTED_BY_OWNER";
}

const TRUST_WORDS: Record<string, string> = {
  SYSTEM: "Provisioned by the System scope",
  SIGNED: "Signed",
  TRUSTED_BY_OWNER: "Trusted by you",
  DEV_FLAG: "Allowed by a development flag",
  UNTRUSTED: "Not trusted: it does not reach the model",
  CHANGED_SINCE_TRUSTED: "Changed since you trusted it: not trusted again",
  REVOKED: "Revoked by the operator",
  FORBIDDEN: "Forbidden by the System scope",
};

export function trustWords(trust: string): string {
  return TRUST_WORDS[trust] ?? trust.toLowerCase().replace(/_/g, " ");
}

export function shortHash(h: string): string {
  return h.length > 12 ? h.slice(0, 12) : h;
}

/** The skill index's aggregate budget, in the Core's numbers. */
export function budgetText(v: Pick<SkillsView, "indexBudgetTokens" | "indexUsedTokens" | "indexOmitted">): string {
  const base = `${v.indexUsedTokens} of ${v.indexBudgetTokens} index tokens used`;
  return v.indexOmitted > 0 ? `${base}; ${v.indexOmitted} left out for want of budget` : base;
}

/** A skill's share of the budget, as the Core costed its index line. */
export function shareText(s: Pick<SkillRow, "indexTokens" | "indexed" | "indexForm">): string {
  if (!s.indexed) return s.indexForm === "NOT_INDEXED" || s.indexForm === "" ? "not in the index" : `not in the index (${s.indexForm.toLowerCase()})`;
  return `${s.indexTokens} tokens (${s.indexForm.toLowerCase()})`;
}

/** What the hook journal last said of a hook. */
export function lastResult(h: Pick<HookRow, "invocations" | "lastOutcome" | "lastDetail" | "lastDurationMs">): string {
  if (h.invocations === 0) return "never ran";
  const detail = h.lastDetail ? `: ${h.lastDetail}` : "";
  return `${h.lastOutcome} after ${h.lastDurationMs} ms (${h.invocations} ${h.invocations === 1 ? "run" : "runs"})${detail}`;
}

const SCOPE_WORDS: Record<string, string> = { DEVICE: "device policy", ADMIN: "administrator", PROJECT: "this repository", USER: "you", EXTENSION: "an extension" };
export function scopeWords(scope: string): string {
  return SCOPE_WORDS[scope] ?? scope.toLowerCase();
}

/** Why a hook cannot be switched off from here, in words; "" when it can. */
export function whyLocked(h: Pick<HookRow, "canDisable" | "scope">): string {
  return h.canDisable ? "" : `Declared by ${scopeWords(h.scope)}, which is above you: it cannot be switched off here.`;
}

export function serverWords(s: { state: string; credentialRequired: boolean; credentialHeld: boolean }): string {
  const cred = s.credentialRequired ? (s.credentialHeld ? "credential held" : "credential not held") : "no credential";
  return `${s.state === "INERT" ? "inert (never started)" : "configured (not probed)"}, ${cred}`;
}

export function extensionState(e: Pick<ExtensionRowInfo, "state" | "quarantineReason">): string {
  return e.state === "ACTIVE" ? "Active" : `Quarantined: ${e.quarantineReason || "inert until you trust this manifest"}`;
}

/** Words that would offer a catalog, an install from the network or an update flow. None may label a control in this view. */
const FORBIDDEN_AFFORDANCE = /\b(marketplace|catalog(ue)?|browse|install|download|update|upgrade|store|discover|publish)\b/i;
export function offersCatalog(labels: readonly string[]): string[] {
  return labels.filter((l) => FORBIDDEN_AFFORDANCE.test(l));
}

const REFUSAL_WORDS: Record<string, string> = {
  HASH_MISMATCH: "The skill is not the one you reviewed. Review the current content and trust that.",
  SKILL_FORBIDDEN: "The System scope forbids this skill; your trust cannot override it.",
  SKILL_NOT_TRUSTABLE: "This skill is provisioned by the System scope and needs no decision.",
  SKILL_REVOKED: "The operator revoked this skill.",
  SKILL_UNKNOWN: "That skill is not listed any more; refresh.",
  HOOK_SCOPE_FORBIDS: "A layer above you declared this hook; it cannot be switched off here.",
  HOOK_UNKNOWN: "That hook is not in force any more; refresh.",
  DIGEST_MISMATCH: "The manifest is not the one that was shown. Refresh and look at it again.",
  EXTENSION_TAMPERED: "This extension's signature does not verify; it is never released.",
  UNKNOWN_EXTENSION: "That extension is not loaded any more; refresh.",
  EXTENSION_NOT_QUARANTINED: "That extension is already active.",
};
export function refusalWords(code: string): string {
  return REFUSAL_WORDS[code] ?? (code ? "The Core refused this." : "This could not be done.");
}
