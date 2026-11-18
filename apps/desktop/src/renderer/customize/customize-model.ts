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

/** The scopes a private memory may be shared into (PX-188); the Core refuses any other and any that is not wider than the item's own. */
export const MEMORY_SHARE_SCOPES = ["space", "organization", "repository"] as const;

/** Whether a memory is private to a person or a session: those are the ones a share widens. Only a hint for the button; the Core decides ownership. */
export function offersShare(row: { scope: string; status: string }): boolean {
  const kind = row.scope.split(":", 1)[0] ?? "";
  return (kind === "user" || kind === "session" || kind === "run") && (row.status === "proposed" || row.status === "curated");
}

/** One line of where a memory lives and where it came from, as the Memory view prints it. */
export function memoryWhere(row: { scope: string; source: string; author: string; status: string }): string {
  const kind = (row.scope.split(":", 1)[0] ?? "").replace("_", " ");
  return `${kind} scope · from ${row.source.replace(/_/g, " ")} by ${row.author || "unknown"} · ${row.status}`;
}

/** What the Core said about a share, in words. */
export function shareWords(r: { outcome: string; refusalCode: string; scope: string; existed: boolean }): string {
  if (r.outcome === "shared") return r.existed ? `Already shared into ${r.scope}.` : `Shared into ${r.scope} with personal detail removed; it is a proposal there until promoted.`;
  return `Not shared (${r.refusalCode || r.outcome}).`;
}

// ------------------------------------------------------------------ REQ-PX-207: the page's words, Rules and the one search

/** Where a skill comes from, in words (the Core's scope). */
const SKILL_SCOPE_WORDS: Record<string, string> = { USER: "yours", PROJECT: "this repository", EXTENSION: "from an extension", SYSTEM: "provisioned by the System scope" };
export function skillScopeWords(scope: string): string {
  return SKILL_SCOPE_WORDS[scope] ?? scope.toLowerCase().replace(/_/g, " ");
}

/** Who may call a skill. */
export function invocationWords(invocation: string): string {
  return invocation === "USER_ONLY" ? "only you can call it" : "you or the model can call it";
}

/** A skill's state in two words, for the row's tag: what reaches the model is the Core's `enabled`. */
export function skillTag(s: Pick<SkillRow, "trust" | "enabled">): { text: string; tone: "ok" | "warn" | "danger" } {
  if (s.trust === "FORBIDDEN" || s.trust === "REVOKED") return { text: s.trust === "FORBIDDEN" ? "Forbidden" : "Revoked", tone: "danger" };
  if (s.enabled) return { text: s.trust === "TRUSTED_BY_OWNER" ? "Trusted" : s.trust === "SIGNED" ? "Signed" : "In use", tone: "ok" };
  return { text: s.trust === "CHANGED_SINCE_TRUSTED" ? "Changed" : "Not trusted", tone: "warn" };
}

const POINT_WORDS: Record<string, string> = {
  before_run: "Before a run",
  after_run: "After a run",
  before_model: "Before each model call",
  after_model: "After each model call",
  before_tool: "Before a tool runs",
  after_tool: "After a tool runs",
  before_change: "Before a file change",
  after_change: "After a file change",
  before_change_commit: "Before a change is committed",
  before_verification: "Before verification",
  after_verification: "After verification",
  before_compaction: "Before the context is compacted",
  after_compaction: "After the context is compacted",
  permission_request: "When an approval is asked",
  task_complete: "When a task completes",
  subagent_start: "When a child task starts",
  subagent_stop: "When a child task stops",
  notification: "On a notification",
  session_end: "When the session ends",
};

/** When a hook runs, what it may do and what its failure does, in words (the Core's point, mode and fail policy). */
export function hookWhen(h: Pick<HookRow, "point" | "mode" | "failPolicy">): string {
  const when = POINT_WORDS[h.point] ?? h.point.replace(/_/g, " ");
  const may = h.mode === "intercept" ? "can stop or change the step" : "only watches";
  const fails = h.failPolicy === "open" ? "if it fails the step goes ahead" : "if it fails the step does not go ahead";
  return `${when} · ${may} · ${fails}`;
}

/** A hook's name: its id after the source prefix (`config:user/rec` → `rec`). */
export function hookName(id: string): string {
  return id.split("/").slice(1).join("/") || id;
}

/** An extension's signature, in words (the Core's label `VERIFIED:<key>`, `UNSIGNED`, `UNKNOWN_KEY:<key>`, `INVALID:<key>`). */
export function signatureWords(signature: string): string {
  const [kind, key] = [signature.split(":", 1)[0] ?? "", signature.slice(signature.indexOf(":") + 1)];
  if (kind === "UNSIGNED") return "not signed";
  if (kind === "VERIFIED") return `signed by ${key}`;
  if (kind === "UNKNOWN_KEY") return `signed by a key this machine does not know (${key})`;
  if (kind === "INVALID") return `signature does not verify (${key})`;
  return signature.toLowerCase().replace(/_/g, " ");
}

/** The shape of one instruction layer as the preload carries it (REQ-PX-207). */
export interface RuleLayer {
  layer: string;
  id: string;
  source: string;
  contentHash: string;
  reason: string;
  loaded: boolean;
  notLoadedReason: string;
  bytes: string;
  truncated: boolean;
  scannerFindings: string[];
}

/** What Rules lists: only the layers the Core recorded as loaded, in its order; the files it found and did not load apart. */
export function rulesOf(instructions: readonly RuleLayer[]): { loaded: RuleLayer[]; notLoaded: RuleLayer[] } {
  return { loaded: instructions.filter((l) => l.loaded), notLoaded: instructions.filter((l) => !l.loaded) };
}

/** A layer's name in words. */
export function layerWords(layer: string): string {
  if (layer === "project") return "This repository's rules";
  if (layer === "repo-instructions") return "This repository's instructions";
  if (layer === "user") return "Your rules, in every project";
  if (layer.startsWith("extension:")) return `From the extension ${layer.slice("extension:".length)}`;
  return layer.replace(/[-_]/g, " ");
}

/** Why a rule is in force, in words (`GLOBAL`, or `PATH:<path> matched <glob>`). */
export function ruleWhen(reason: string): string {
  if (reason === "GLOBAL" || reason === "") return "Always in force";
  const m = /^PATH:(.*) matched (.*)$/.exec(reason);
  if (m) return `In force because ${m[1]} matches ${m[2]}`;
  return reason.toLowerCase().replace(/_/g, " ");
}

/** A rule's size as the Core measured it. */
export function ruleSize(r: Pick<RuleLayer, "bytes" | "truncated">): string {
  const n = Number(r.bytes);
  const size = !Number.isFinite(n) || n <= 0 ? "" : n < 1024 ? `${n} bytes` : `${(n / 1024).toFixed(1)} KB`;
  return [size, r.truncated ? "cut to fit the prompt" : ""].filter(Boolean).join(", ");
}

/** The file name at the end of a source path, for a row's title. */
export function baseName(source: string): string {
  const parts = source.split(/[\\/]/).filter(Boolean);
  return parts[parts.length - 1] ?? source;
}

/** The sections of the page, in the order the section list shows them. */
export const CUSTOMIZE_SECTIONS = [
  { id: "skills", label: "Skills" },
  { id: "rules", label: "Rules" },
  { id: "hooks", label: "Hooks" },
  { id: "extensions", label: "Extensions" },
  { id: "memory", label: "Memory" },
] as const;
export type SectionId = (typeof CUSTOMIZE_SECTIONS)[number]["id"];

/** One search result: the section it opens, the key of its row there, and what the result says. */
export interface Hit {
  section: SectionId;
  key: string;
  title: string;
  detail: string;
}

/** What the search reads: the lists the page already has from the Core, nothing else. */
export interface Searchable {
  skills: readonly SkillRow[];
  rules: readonly RuleLayer[];
  hooks: readonly HookRow[];
  extensions: readonly ExtensionRowInfo[];
  memory: readonly { id: string; topic: string; content: string; scope: string }[];
}

/** The key a skill's row carries (a name can exist in more than one scope). */
export const skillKey = (s: Pick<SkillRow, "scope" | "name">): string => `${s.scope}/${s.name}`;

/**
 * One search across the sections (MWS-14): every row whose words contain each of the query's words, case-insensitively, with the
 * section it belongs to. It filters what the page holds; it keeps no index and asks nothing of anyone.
 */
export function customizeHits(query: string, d: Searchable): Hit[] {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean);
  if (words.length === 0) return [];
  const match = (...fields: string[]) => {
    const text = fields.join(" \n ").toLowerCase();
    return words.every((w) => text.includes(w));
  };
  const out: Hit[] = [];
  for (const s of d.skills) if (match(s.name, s.description, s.provenanceAuthor, s.provenanceSource, skillScopeWords(s.scope))) out.push({ section: "skills", key: skillKey(s), title: s.name, detail: s.description });
  for (const r of d.rules) if (r.loaded && match(r.source, r.id, layerWords(r.layer), ruleWhen(r.reason))) out.push({ section: "rules", key: r.source, title: baseName(r.source), detail: `${layerWords(r.layer)} · ${r.source}` });
  for (const h of d.hooks) if (match(h.id, h.command.join(" "), hookWhen(h), h.tools.join(" "))) out.push({ section: "hooks", key: h.id, title: hookName(h.id), detail: hookWhen(h) });
  for (const e of d.extensions) if (match(e.name, e.publisher, e.source, e.servers.map((s) => s.name).join(" "), e.commands.join(" "))) out.push({ section: "extensions", key: e.extensionId, title: e.name, detail: e.servers.length ? `tool servers ${e.servers.map((s) => s.name).join(", ")}` : "extension" });
  for (const m of d.memory) if (match(m.topic, m.content)) out.push({ section: "memory", key: m.id, title: m.topic, detail: m.content.length > 120 ? `${m.content.slice(0, 120)}…` : m.content });
  return out;
}
