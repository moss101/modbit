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

