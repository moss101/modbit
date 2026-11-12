/**
 * The Activity pane's view model (REQ-PX-202; DR-PX-2026-11-11-021 MWS-10), pure: the task's delivered transcript rows and
 * delivered events as one list in the order of the Core's log, each entry with a kind to filter by and words to find text in.
 * Nothing is derived that was not delivered: a row is worded from its own facts and text, an event from its own type and a few
 * named fields of its payload. An event the transcript already tells (its offset lies inside a listed row) is not listed twice.
 */
import type { TranscriptRowView } from "../../shared/conversation-types.ts";
import { humanTitle } from "../approvals/model.ts";
import { callStatus, toolVerb } from "../conversation/model.ts";
import type { DeliveredEvent } from "./delivered.ts";

export const ACTIVITY_KINDS = ["message", "edit", "command", "read", "approval", "check", "turn", "other"] as const;
export type ActivityKind = (typeof ACTIVITY_KINDS)[number];
export const KIND_LABEL: Record<ActivityKind, string> = { message: "Messages", edit: "Edits", command: "Commands", read: "Reads", approval: "Approvals", check: "Checks", turn: "Turns", other: "Other" };

export interface ActivityEntry {
  /** Stable across re-reads: the row's id, or the event's offset. */
  key: string;
  /** Where the entry was delivered from: the transcript the conversation read, or an event main pushed. */
  source: "transcript" | "event";
  /** The Core's log offset the entry stands at (its order). */
  offset: string;
  atMs: number;
  kind: ActivityKind;
  text: string;
  /** A row that needs the person, went wrong or went well, for its mark. */
  tone: "ask" | "bad" | "ok" | "none";
}

const firstLine = (text: string): string => {
  const line = text.split("\n").find((l) => l.trim() !== "") ?? "";
  return line.trim();
};
const cut = (text: string, max = 200): string => (text.length > max ? `${text.slice(0, max - 1)}…` : text);

const TOOL_KIND: Record<string, ActivityKind> = { EDIT: "edit", SHELL: "command", READ: "read", SEARCH: "read" };
const STATUS_WORDS: Record<string, string> = { FAILED: "failed", DENIED: "not allowed", CANCELLED: "cancelled", PENDING_APPROVAL: "waiting for approval", UNKNOWN_OUTCOME: "outcome unknown" };
const APPROVAL_WORDS: Record<string, string> = { PENDING: "Asked to", APPROVED: "Allowed to", DENIED: "Not allowed to", EXPIRED: "Not answered in time:", CANCELLED: "No longer asked to" };
const OUTCOME_WORDS: Record<string, string> = { COMPLETED: "Turn finished", INTERRUPTED: "Turn interrupted", FAILED: "Turn failed" };

function rowEntry(row: TranscriptRowView): ActivityEntry | null {
  const base = { key: row.rowId, source: "transcript" as const, offset: row.offset, atMs: row.atMs };
  const f = row.facts;
  switch (row.kind) {
    case "USER_MESSAGE":
      return { ...base, kind: "message", text: cut(`You: ${firstLine(row.text)}`), tone: "none" };
    case "ASSISTANT_MESSAGE": {
      const line = firstLine(row.text) || row.hints.shortText;
      return line ? { ...base, kind: "message", text: cut(`Agent: ${line}`), tone: "none" } : null;
    }
    case "TOOL_CARD": {
      if (f.type !== "tool") return null;
      const status = callStatus(row.hints.status, f.failureCode);
      const what = firstLine(row.text) || row.hints.shortText || f.paths[0] || f.toolName;
      const after = STATUS_WORDS[status];
      return { ...base, kind: TOOL_KIND[f.toolClass] ?? "other", text: cut(`${toolVerb(f.toolClass, status)} ${what}${after ? ` (${after})` : ""}`), tone: status === "FAILED" ? "bad" : status === "PENDING_APPROVAL" ? "ask" : "none" };
    }
    case "APPROVAL_CARD": {
      if (f.type !== "approval") return null;
      const title = humanTitle(f.toolName);
      const lead = APPROVAL_WORDS[f.state] ?? `${f.state}:`;
      return { ...base, kind: "approval", text: cut(`${lead} ${title.charAt(0).toLowerCase()}${title.slice(1)} (${f.toolName})`), tone: f.state === "PENDING" ? "ask" : f.state === "APPROVED" ? "ok" : "none" };
    }
    case "WORK_GROUP": {
      // Listed as one entry only where the Core delivered it folded, without its steps.
      const label = (f.type === "group" && f.label) || row.hints.shortText || firstLine(row.text);
      return label ? { ...base, kind: "other", text: cut(label), tone: "none" } : null;
    }
    case "TURN_FOOTER": {
      if (f.type !== "footer") return null;
      const words = OUTCOME_WORDS[f.outcome] ?? `Turn ${f.outcome.toLowerCase()}`;
      return { ...base, kind: "turn", text: `${words}${f.failureCode ? ` (${f.failureCode})` : ""}`, tone: f.outcome === "FAILED" ? "bad" : f.outcome === "COMPLETED" ? "ok" : "none" };
    }
    default:
      // Tail status, time boundaries and the unread divider are the conversation's furniture, not things that happened.
      return null;
  }
}

/** The rows that are entries, in order, with the steps of an unfolded group in place of the group. */
function flatten(rows: readonly TranscriptRowView[], out: { row: TranscriptRowView; entry: ActivityEntry }[]): void {
  for (const row of rows) {
    if (row.kind === "WORK_GROUP" && row.children.length > 0) {
      flatten(row.children, out);
      continue;
    }
    const entry = rowEntry(row);
    if (entry) out.push({ row, entry });
  }
}

/** `VerificationRunRecorded` -> `Verification run recorded`. */
export const eventTitle = (eventType: string): string => {
  const words = eventType.replace(/([a-z0-9])([A-Z])/g, "$1 $2").replace(/_/g, " ").trim().toLowerCase();
  return words ? words.charAt(0).toUpperCase() + words.slice(1) : "Event";
};

/** The payload fields an event's line may quote, in this order, when they hold a short word or number. */
const QUOTED = ["tool", "tool_name", "stage", "status", "verdict", "outcome", "mode", "code", "reason"] as const;

function eventDetail(payload: unknown): string {
  if (typeof payload !== "object" || payload === null || Array.isArray(payload)) return "";
  const p = payload as Record<string, unknown>;
  const parts: string[] = [];
  for (const k of QUOTED) {
    const v = p[k];
    if ((typeof v === "string" && v !== "" && v.length <= 80) || typeof v === "number") parts.push(String(v).replace(/_/g, " ").toLowerCase());
  }
  return parts.slice(0, 3).join(", ");
}

const EVENT_KIND: Record<string, ActivityKind> = { approval: "approval", verification: "check", checkpoint: "other" };

function eventKind(e: DeliveredEvent): ActivityKind {
  if (/^Approval/.test(e.eventType)) return "approval";
  if (/Verification|AcceptanceGate/.test(e.eventType)) return "check";
  return EVENT_KIND[e.aggregateType] ?? "other";
}

function eventEntry(e: DeliveredEvent): ActivityEntry {
  const detail = eventDetail(e.payload);
  const status = typeof e.payload === "object" && e.payload !== null ? String((e.payload as Record<string, unknown>)["status"] ?? (e.payload as Record<string, unknown>)["verdict"] ?? "") : "";
  return { key: `event:${e.offset}`, source: "event", offset: e.offset, atMs: e.occurredAtMs, kind: eventKind(e), text: cut(detail ? `${eventTitle(e.eventType)}: ${detail}` : eventTitle(e.eventType)), tone: /FAIL|REJECT|DENIED/i.test(status) ? "bad" : /PASS|ACCEPT/i.test(status) ? "ok" : e.eventType === "ApprovalRequested" ? "ask" : "none" };
}

const big = (offset: string): bigint => (/^\d+$/.test(offset) ? BigInt(offset) : 0n);

/**
 * The task's activity, oldest first: the delivered transcript's rows, and each delivered event that no listed row covers
 * (a row stands for the events from its first offset to its last).
 */
export function activityOf(rows: readonly TranscriptRowView[], events: readonly DeliveredEvent[]): ActivityEntry[] {
  const listed: { row: TranscriptRowView; entry: ActivityEntry }[] = [];
  flatten(rows, listed);
  const spans = listed.map(({ row }) => [big(row.offset), big(row.lastOffset || row.offset)] as const);
  const covered = (offset: bigint): boolean => spans.some(([from, to]) => offset >= from && offset <= to);
  const out = listed.map((l) => l.entry);
  for (const e of events) if (!covered(big(e.offset))) out.push(eventEntry(e));
  // Stable: entries at one offset keep the order they were listed in.
  return out.map((entry, i) => ({ entry, i })).sort((a, b) => (big(a.entry.offset) < big(b.entry.offset) ? -1 : big(a.entry.offset) > big(b.entry.offset) ? 1 : a.i - b.i)).map((x) => x.entry);
}

/** How many entries there are of each kind (a kind with none has no filter drawn). */
export function kindCounts(entries: readonly ActivityEntry[]): Partial<Record<ActivityKind, number>> {
  const out: Partial<Record<ActivityKind, number>> = {};
  for (const e of entries) out[e.kind] = (out[e.kind] ?? 0) + 1;
  return out;
}

/** The entries of one kind (or all) whose words hold every word of the query, whatever its case. */
export function filterActivity(entries: readonly ActivityEntry[], kind: ActivityKind | "all", query: string): ActivityEntry[] {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean);
  return entries.filter((e) => (kind === "all" || e.kind === kind) && words.every((w) => e.text.toLowerCase().includes(w)));
}

/** A time of day, to the second, for an entry's line; "" where the Core gave none. */
export function clock(atMs: number): string {
  if (!atMs) return "";
  const d = new Date(atMs);
  const two = (n: number) => String(n).padStart(2, "0");
  return `${two(d.getHours())}:${two(d.getMinutes())}:${two(d.getSeconds())}`;
}
