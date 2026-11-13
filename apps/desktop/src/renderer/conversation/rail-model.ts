/**
 * The details rail's view model (REQ-PX-201; DR-PX-2026-11-11-021 MWS-09): pure functions over what the Core answered.
 * Nothing here knows a fact of its own: each function words one read (the task's header, its run mode, its accounting,
 * its economics, its checkpoints, the session's extensions, the terminal list), and returns null where the Core has not
 * answered, so the rail leaves the row out instead of drawing a default.
 */
import type { AccountingInfo, CheckpointInfo, CheckpointListInfo } from "../../shared/control-types.ts";
import type { ExtensionRowInfo } from "../../shared/customize-types.ts";
import type { TaskEconomicsSummary } from "../../preload/preload.ts";
import { formatCount, formatWall } from "../context/model.ts";

/** The preload methods a value of the rail can come from (the spec reads these names off the page and finds each in the bridge's log). */
export type RailSource = "agentHeaders" | "changesState" | "runMode" | "pendingApprovals" | "contextAccounting" | "customizeHooks" | "taskEconomics" | "checkpoints" | "transcript" | "sessionSnapshot" | "terminalList";

/**
 * Where the Core says the task runs, in the person's words: this machine by its kind, or the isolated cloud. Null until the
 * header is read: a location is never assumed. A word the desktop does not know is shown as the Core wrote it.
 */
export function locationWords(executionLocation: string | undefined, machine: string): string | null {
  if (!executionLocation) return null;
  if (executionLocation === "local") return machine;
  if (executionLocation === "cloud") return "Isolated cloud";
  return executionLocation;
}

/** A folder as its name and the path it is in (either separator; a trailing one is ignored). */
export function folderWords(root: string): { name: string; parent: string } | null {
  const clean = root.replace(/[\\/]+$/, "");
  if (!clean) return null;
  const at = Math.max(clean.lastIndexOf("/"), clean.lastIndexOf("\\"));
  return at < 0 ? { name: clean, parent: "" } : { name: clean.slice(at + 1), parent: clean.slice(0, at) || clean.slice(0, at + 1) };
}

/** The size of the change as the header counts it. */
export function changeWords(h: { filesChanged: number; linesAdded: number; linesRemoved: number }): { added: string; removed: string; files: string } | null {
  if (h.filesChanged <= 0) return null;
  return { added: `+${h.linesAdded}`, removed: `−${h.linesRemoved}`, files: `${h.filesChanged} ${h.filesChanged === 1 ? "file" : "files"}` };
}

/** The latest verification result from the task's economics: how many checks ran and passed, and whether the Core counts the task verified. */
export function verificationWords(e: TaskEconomicsSummary | null): string | null {
  if (!e) return null;
  const total = e.checksPassed + e.checksFailed;
  if (total === 0) return e.verified ? "verified" : "not run yet";
  const counts = `${e.checksPassed} of ${total} passed`;
  return e.verified ? `verified, ${counts}` : counts;
}

/** What the task has cost at catalog list prices, or that its model has no price. Null before the Core answers. */
export function spendWords(e: TaskEconomicsSummary | null): string | null {
  if (!e) return null;
  return e.pricingKnown ? `$${e.costUsd.toFixed(e.costUsd >= 1 ? 2 : 4)}` : "not priced";
}

export const modelCallWords = (n: number): string => `${n} model ${n === 1 ? "call" : "calls"}`;

/** `PROTECTED_WRITE` or `targeted` as lower-case words. */
const lower = (code: string): string => code.replace(/_/g, " ").toLowerCase();

const GATE_WORDS: Record<string, string> = { ACCEPT: "passed", REJECT: "did not pass", INCONCLUSIVE: "could not decide" };

/**
 * The task's latest evidence line in a person's words. The Core (and the window's own fold of its events) writes the line in its
 * own terms: "delegated child-a (BACKGROUND)", "targeted verification PASSED", "acceptance gate ACCEPT". The facts are kept (the
 * child's name, the stage, the result); only the wording changes. A line of no known shape is shown as it is.
 */
export function evidenceWords(raw: string): string {
  const delegated = /^delegated (.+?) \(([A-Za-z_]*)\)$/.exec(raw);
  if (delegated) return `Started a child task${delegated[2] === "BACKGROUND" ? " in the background" : delegated[2] ? ` (${lower(delegated[2])})` : ""}: ${delegated[1]}`;
  const refused = /^delegation refused: (\S*) at (\S*)$/.exec(raw);
  if (refused) return `A child task could not be started: ${lower(refused[1] ?? "") || "refused"}${refused[2] ? ` (at ${lower(refused[2])})` : ""}`;
  const protectedEffect = /^child (.*?) reached a protected effect: (\S*) \((.*)\)$/.exec(raw);
  if (protectedEffect) return `A child task${protectedEffect[1] ? ` (${protectedEffect[1]})` : ""} is waiting for approval to use ${protectedEffect[2]}`;
  const child = /^child ([a-z_]+): ([\s\S]*)$/.exec(raw);
  if (child) {
    const status = child[1] === "completed" ? "finished" : lower(child[1] ?? "");
    // The Core repeats the status where the child left no summary ("child cancelled: cancelled"): said once.
    return (child[2] ?? "").trim().toLowerCase() === child[1] || !(child[2] ?? "").trim() ? `A child task ${status === "finished" ? status : `was ${status}`}` : `A child task ${status}: ${child[2]}`;
  }
  const verification = /^(\S+) verification (\S+)$/.exec(raw);
  if (verification) return `${(verification[1] ?? "").charAt(0).toUpperCase()}${lower((verification[1] ?? "").slice(1))} checks ${lower(verification[2] ?? "")}`;
  const gate = /^acceptance gate (\S+)$/.exec(raw);
  if (gate) return `Final check before review: ${GATE_WORDS[gate[1] ?? ""] ?? lower(gate[1] ?? "")}`;
  const escalated = /^escalated to (.+)$/.exec(raw);
  if (escalated) return `Moved to another model: ${escalated[1]}`;
  return raw;
}

/**
 * The task's economics as a short list of facts (what Details shows in full): each line is one or two numbers of the Core's
 * with the words a person would use for them. Nothing is computed beyond the sum of two counts the Core gave.
 */
export function economicsFacts(e: TaskEconomicsSummary): { id: string; label: string; value: string }[] {
  const checks = e.checksPassed + e.checksFailed;
  const requests = e.prefixCacheHits + e.prefixCacheMisses;
  const out = [
    { id: "checks", label: "Checks", value: checks === 0 ? (e.verified ? "verified" : "none run yet") : `${e.checksPassed} of ${checks} passed${e.verified ? ", verified" : ", not verified yet"}` },
    { id: "model", label: "Model", value: `${modelCallWords(e.modelCalls)}${e.model ? ` to ${e.model}` : ""}` },
    { id: "tokens", label: "Tokens", value: `${formatCount(e.inputTokens)} in (${formatCount(e.cachedInputTokens)} reused from the cache), ${formatCount(e.outputTokens)} out` },
    { id: "cost", label: "Cost", value: e.pricingKnown ? `$${e.costUsd.toFixed(4)} at list prices` : "no price is known for this model" },
    { id: "tools", label: "Tool calls", value: String(e.toolCalls) },
    { id: "time", label: "Time", value: `${formatWall(e.wallMs)} in all, ${formatWall(e.modelMs)} in the model, ${formatWall(e.toolMs)} in tools` },
  ];
  if (requests > 0) out.push({ id: "reuse", label: "Prompt reuse", value: `${e.prefixCacheHits} of ${requests} ${requests === 1 ? "request" : "requests"} reused the start of the prompt` });
  return out;
}

/**
 * The task's caps beside what it has used, in the Core's own units: the cost cap is in minor currency units (the Core names
 * no currency), the time cap in wall-clock time. Empty where no cap is set.
 */
export function budgetLines(a: AccountingInfo | null): { kind: "cost" | "time"; text: string; percent: number }[] {
  const b = a?.budgets;
  if (!b) return [];
  const out: { kind: "cost" | "time"; text: string; percent: number }[] = [];
  const pct = (used: bigint, cap: bigint): number => Number(used >= cap ? 100n : (used * 100n) / cap);
  const cost = BigInt(b.maxCostMinor);
  if (cost > 0n) out.push({ kind: "cost", text: `${formatCount(b.spentMinor)} of ${formatCount(cost)} minor units`, percent: pct(BigInt(b.spentMinor), cost) });
  const wall = BigInt(b.maxWallMs);
  if (wall > 0n) out.push({ kind: "time", text: `${formatWall(b.wallMsUsed)} of ${formatWall(wall)}`, percent: pct(BigInt(b.wallMsUsed), wall) });
  return out;
}

/** The tool servers the session's extensions declare, as the Core lists them. The Core does not probe a server: it is configured or inert, never "connected". */
export function toolServers(extensions: readonly ExtensionRowInfo[] | null): { configured: number; inert: number; words: string } | null {
  if (!extensions) return null;
  const all = extensions.flatMap((e) => e.servers);
  if (all.length === 0) return null;
  const inert = all.filter((s) => s.state === "INERT").length;
  const configured = all.length - inert;
  const parts = [configured > 0 ? `${configured} configured` : "", inert > 0 ? `${inert} inert` : ""].filter(Boolean);
  return { configured, inert, words: parts.join(", ") };
}

/** A task state after which nothing of the task runs: a child in one of these is finished, and has no Stop. */
const ENDED = new Set(["Completed", "Cancelled", "Failed", "ReadyForReview"]);
export const childEnded = (state: string): boolean => ENDED.has(state);

/** The task's child tasks and terminals split into what still runs (each can be stopped) and what has finished (counted). */
export function backgroundOf<C extends { state: string }, T extends { state: string }>(children: readonly C[], terminals: readonly T[]): { liveChildren: C[]; liveTerminals: T[]; endedChildren: C[]; endedTerminals: T[]; finished: number } {
  const liveChildren = children.filter((c) => !childEnded(c.state));
  const endedChildren = children.filter((c) => childEnded(c.state));
  const liveTerminals = terminals.filter((t) => t.state.toLowerCase() === "running");
  const endedTerminals = terminals.filter((t) => t.state.toLowerCase() !== "running");
  return { liveChildren, liveTerminals, endedChildren, endedTerminals, finished: endedChildren.length + endedTerminals.length };
}

/** The checkpoints a person can restore from the rail: the committed turn boundaries, newest first (the ones the conversation marks on its turns). */
export function restorable(list: CheckpointListInfo | null): CheckpointInfo[] {
  const byTurn = new Map<string, CheckpointInfo>();
  for (const c of list?.checkpoints ?? []) {
    if (!c.turnId || (c.status !== "CURRENT" && c.status !== "SUPERSEDED")) continue;
    const prev = byTurn.get(c.turnId);
    if (!prev || c.epoch > prev.epoch) byTurn.set(c.turnId, c);
  }
  return [...byTurn.values()].sort((a, b) => b.epoch - a.epoch);
}

export const checkpointLabel = (c: CheckpointInfo): string => `${c.turnOrdinal > 0 ? `Turn ${c.turnOrdinal}` : "This turn"}${c.name ? ` · ${c.name}` : ""}`;
export const checkpointHeading = (c: CheckpointInfo): string => `at the end of ${c.turnOrdinal > 0 ? `turn ${c.turnOrdinal}` : "this turn"}`;
export const countWords = (n: number, one: string, many: string): string => `${n} ${n === 1 ? one : many}`;

