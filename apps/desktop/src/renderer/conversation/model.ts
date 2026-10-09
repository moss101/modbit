/**
 * The conversation's pure model (REQ-PX-047; docs/65 AFW-C01..C09). The rows
 * are the Core's projection of its event log (GetTranscript); this module only
 * combines them with the live deltas of the streams that are still open, and
 * decides what is safe to show as final. It never re-derives a row, a group
 * or a density: folding is the Core's, and a row that needs the person
 * (a pending approval, a question, a failure) is never left hidden inside a
 * fold.
 */
import type { OpenStreamView, TranscriptRowView } from "../../shared/conversation-types.ts";

/** A stream id as the stable key shared by events (hex) and rows (hyphenated UUID). */
export const streamKey = (id: string): string => id.replace(/-/g, "").toLowerCase();

// -------------------------------------------------------------- live streams

export type LivePhase = "OPEN" | "COMPLETED" | "ABORTED";

export interface LiveStream {
  key: string;
  taskId: string;
  text: string;
  /** Delta sequence of the last piece applied (from 1). */
  seq: number;
  phase: LivePhase;
  abortSource: string;
  abortCode: string;
  /** The ms time of the newest piece, for the stall ladder. */
  lastAtMs: number;
  /** Pieces that arrived ahead of a missing one, applied when it lands. */
  held: Map<number, string>;
}

export type LiveStreams = ReadonlyMap<string, LiveStream>;

export interface StreamEventLike {
  eventType: string;
  aggregateType: string;
  aggregateId: string;
  taskId: string | null;
  occurredAtMs: number;
  payload: unknown;
}

const MAX_LIVE_CHARS = 4 * 1024 * 1024;
const MAX_LIVE_STREAMS = 32;

function drain(s: LiveStream): void {
  for (let next = s.held.get(s.seq + 1); next !== undefined; next = s.held.get(s.seq + 1)) {
    s.held.delete(s.seq + 1);
    s.seq += 1;
    s.text += next;
  }
}

/** Applies one wire event to the live streams; returns the same map when the event is not a stream event for this task or changes nothing. */
export function applyStreamEvent(live: LiveStreams, ev: StreamEventLike, taskId: string): LiveStreams {
  if (ev.aggregateType !== "assistant_stream" || ev.taskId !== taskId) return live;
  const key = streamKey(ev.aggregateId);
  const p = (ev.payload ?? {}) as Record<string, unknown>;
  const cur = live.get(key);
  const next = new Map(live);
  if (ev.eventType === "AssistantTextDelta") {
    // Reasoning is folded away from the message text; it never joins the answer.
    if (p["kind"] !== "TEXT" || typeof p["text"] !== "string" || typeof p["sequence"] !== "number") return live;
    const seq = p["sequence"];
    const s: LiveStream = cur ? { ...cur, held: new Map(cur.held) } : { key, taskId, text: "", seq: 0, phase: "OPEN", abortSource: "", abortCode: "", lastAtMs: 0, held: new Map() };
    if (seq <= s.seq) return live; // a redelivery adds nothing
    if (s.text.length + (p["text"] as string).length > MAX_LIVE_CHARS) return live;
    s.lastAtMs = ev.occurredAtMs;
    if (seq === s.seq + 1) {
      s.seq = seq;
      s.text += p["text"] as string;
      drain(s);
    } else if (s.held.size < 64) {
      s.held.set(seq, p["text"] as string);
    }
    if (!cur && next.size >= MAX_LIVE_STREAMS) next.delete(next.keys().next().value as string);
    next.set(key, s);
    return next;
  }
  if (ev.eventType === "AssistantMessageCompleted" || ev.eventType === "AssistantMessageAborted") {
    if (!cur) return live;
    const done: LiveStream = { ...cur, phase: ev.eventType === "AssistantMessageCompleted" ? "COMPLETED" : "ABORTED", abortSource: String(p["source"] ?? ""), abortCode: String(p["code"] ?? ""), lastAtMs: ev.occurredAtMs };
    next.set(key, done);
    return next;
  }
  return live;
}

/** Seeds the live streams with what main has seen of the task's open streams (a reload, or opening mid-stream). Pieces already applied are kept. */
export function seedStreams(live: LiveStreams, open: readonly OpenStreamView[], nowMs: number): LiveStreams {
  let next: Map<string, LiveStream> | null = null;
  for (const o of open) {
    const key = streamKey(o.streamId);
    const cur = live.get(key);
    if (cur && cur.seq >= o.sequence) continue;
    next ??= new Map(live);
    const s: LiveStream = { key, taskId: o.taskId, text: o.text, seq: o.sequence, phase: cur?.phase ?? "OPEN", abortSource: cur?.abortSource ?? "", abortCode: cur?.abortCode ?? "", lastAtMs: nowMs, held: new Map(cur?.held ?? []) };
    for (const q of [...s.held.keys()]) if (q <= s.seq) s.held.delete(q);
    drain(s);
    next.set(key, s);
  }
  return next ?? live;
}

// -------------------------------------------------------------- presentation

export type MessageState = "streaming" | "complete" | "aborted";

export interface PresentedRow {
  row: TranscriptRowView;
  /** For an assistant message: the text to show and whether it is final. */
  text: string;
  message?: MessageState | undefined;
  /** Why an aborted stream stopped, in words. */
  abort?: string | undefined;
}

export const ABORT_WORDS: Record<string, string> = {
  USER_INTERRUPT: "You interrupted this response",
  RUNTIME: "Modbit stopped this response",
  PROVIDER: "The model provider ended this response",
  RECOVERY: "Modbit restarted while this response was streaming",
};

/** What an aborted stream says about itself: the typed source in words, with the code. */
export function abortWords(source: string, code: string): string {
  const base = ABORT_WORDS[source] ?? "This response stopped before it finished";
  return code ? `${base} (${code}).` : `${base}.`;
}

/** The longer of two texts of one stream: a streaming row only grows (AFW-C02). */
const longer = (a: string, b: string): string => (b.length >= a.length ? b : a);

/**
 * Presents one assistant row. The Core's phase is the truth for COMPLETED and
 * ABORTED; for an OPEN stream the live deltas supply the text. A stream whose
 * closing event the renderer has seen but whose row it has not yet re-read is
 * shown by the live phase, still never as final unless it completed.
 */
export function presentAssistant(row: TranscriptRowView, live: LiveStreams): PresentedRow {
  if (row.facts.type !== "stream") return { row, text: row.text };
  const l = live.get(streamKey(row.facts.streamId));
  const phase = row.facts.phase;
  if (phase === "COMPLETED") return { row, text: row.text, message: "complete" };
  if (phase === "ABORTED") {
    const partial = row.text || l?.text || "";
    return { row, text: partial, message: "aborted", abort: abortWords(row.facts.abortSource, row.facts.abortCode) };
  }
  // OPEN in the Core's projection.
  const text = l ? longer(row.text, l.text) : row.text;
  if (l?.phase === "COMPLETED") return { row, text, message: "complete" };
  if (l?.phase === "ABORTED") return { row, text, message: "aborted", abort: abortWords(l.abortSource, l.abortCode) };
  return { row, text, message: "streaming" };
}

/** A synthetic row for a stream the live events know and the last projection read does not yet. */
export function liveOnlyRow(l: LiveStream): TranscriptRowView {
  return {
    ordinal: 0,
    rowId: `msg:live:${l.key}`,
    kind: "ASSISTANT_MESSAGE",
    offset: "0",
    lastOffset: "0",
    atMs: l.lastAtMs,
    turnId: "",
    hints: { renderable: true, groupable: false, hasReasoning: false, durationMs: 0, shortText: "", linesAdded: 0, linesRemoved: 0, status: "STREAMING" },
    text: "",
    textTruncated: false,
    textRef: "",
    children: [],
    facts: { type: "stream", streamId: l.key, phase: "OPEN", firstOffset: "0", lastOffset: "0", deltaCount: 0, contentHash: "", abortSource: "", abortCode: "" },
  };
}

/** The top-level rows to draw, the tail separated, with the live-only streams appended where the projection has no row for them. */
export function splitTail(rows: readonly TranscriptRowView[]): { body: TranscriptRowView[]; tail: TranscriptRowView | null } {
  const tail = rows.find((r) => r.kind === "TAIL_STATUS") ?? null;
  return { body: rows.filter((r) => r.kind !== "TAIL_STATUS"), tail };
}

export function withLiveOnly(body: readonly TranscriptRowView[], live: LiveStreams): TranscriptRowView[] {
  const known = new Set<string>();
  const note = (r: TranscriptRowView) => {
    if (r.facts.type === "stream") known.add(streamKey(r.facts.streamId));
    r.children.forEach(note);
  };
  body.forEach(note);
  const extra = [...live.values()].filter((l) => l.phase === "OPEN" && !known.has(l.key) && l.text.length > 0).map(liveOnlyRow);
  return extra.length > 0 ? [...body, ...extra] : [...body];
}

// ---------------------------------------------------------------- attention

const TOOL_BAD = new Set(["FAILED", "DENIED", "UNKNOWN_OUTCOME", "CANCELLED"]);

/** Whether a row (or anything folded inside it) needs the person or reports a failure. Such a fold is shown open. */
export function needsEyes(r: TranscriptRowView): boolean {
  if (r.kind === "APPROVAL_CARD" && r.hints.status === "REQUESTED") return true;
  if (r.kind === "TOOL_CARD" && (TOOL_BAD.has(r.hints.status) || r.hints.status === "AWAITING_APPROVAL")) return true;
  if (r.facts.type === "stream" && r.facts.phase === "ABORTED") return true;
  if (r.facts.type === "footer" && r.facts.outcome === "FAILED") return true;
  return r.children.some(needsEyes);
}

/** Row kinds that count as a message for the "N new messages" pill. */
export const isMessage = (r: TranscriptRowView): boolean => r.kind === "USER_MESSAGE" || r.kind === "ASSISTANT_MESSAGE" || r.kind === "APPROVAL_CARD";

// ----------------------------------------------------------------- following

export interface Follow {
  /** The view follows the tail. */
  pinned: boolean;
  /** Messages that landed while the person was scrolled up. */
  unseen: number;
  /** The pill was dismissed (it returns when more arrive). */
  dismissed: boolean;
}

export const FOLLOW_SLACK_PX = 32;
export const INITIAL_FOLLOW: Follow = { pinned: true, unseen: 0, dismissed: false };

/** The person scrolled: within the slack of the bottom they are following; scrolling up freezes follow. */
export function onScroll(f: Follow, distanceFromBottom: number): Follow {
  const pinned = distanceFromBottom <= FOLLOW_SLACK_PX;
  return pinned ? { pinned: true, unseen: 0, dismissed: false } : f.pinned ? { ...f, pinned: false } : f;
}

/** Messages landed: counted only while scrolled up. */
export function onMessages(f: Follow, n: number): Follow {
  if (n <= 0 || f.pinned) return f;
  return { ...f, unseen: f.unseen + n, dismissed: false };
}
export const onDismiss = (f: Follow): Follow => ({ ...f, dismissed: true });
export const onJumpToBottom = (): Follow => INITIAL_FOLLOW;

export const pillText = (n: number): string => `${n} new ${n === 1 ? "message" : "messages"}`;

/** The ids of top-level message rows, to count what is new between two reads. */
export const messageIds = (rows: readonly TranscriptRowView[]): string[] => rows.filter(isMessage).map(identityOf);

/** A row's identity across reads: a stream is one message from its first live piece to its projected row. */
export const identityOf = (r: TranscriptRowView): string => (r.facts.type === "stream" ? `stream:${streamKey(r.facts.streamId)}` : r.rowId);

/** Reuses the previous row objects that did not change, so memoised row views skip them (a refetch otherwise renews every object). */
export function reuseRows(prev: readonly TranscriptRowView[], next: readonly TranscriptRowView[]): TranscriptRowView[] {
  const byId = new Map(prev.map((r) => [r.rowId, r]));
  return next.map((n) => {
    const p = byId.get(n.rowId);
    if (!p) return n;
    const children = reuseRows(p.children, n.children);
    const same = p.kind === n.kind && p.lastOffset === n.lastOffset && p.text === n.text && p.textTruncated === n.textTruncated && p.atMs === n.atMs && JSON.stringify(p.hints) === JSON.stringify(n.hints) && JSON.stringify(p.facts) === JSON.stringify(n.facts) && children.length === p.children.length && children.every((c, i) => c === p.children[i]);
    return same ? p : children.every((c, i) => c === n.children[i]) ? n : { ...n, children };
  });
}

// ------------------------------------------------------------- tail / stall

export interface StallLevel {
  level: 0 | 1 | 2 | 3;
  text: string;
}

/** The stall ladder (AFW-C03): the wording escalates with the silence (2 s, 8 s, 32 s); the STALL attention item itself is the Core's. */
export const STALL_STEPS_MS = [2_000, 8_000, 32_000] as const;

export function stallLevel(label: string, idleMs: number): StallLevel {
  const base = label || "Working";
  if (idleMs >= STALL_STEPS_MS[2]) return { level: 3, text: `${base}: no new activity for ${Math.floor(idleMs / 1000)} s. It may be stuck; you can steer or stop the task.` };
  if (idleMs >= STALL_STEPS_MS[1]) return { level: 2, text: `${base}: still waiting, ${Math.floor(idleMs / 1000)} s with no output.` };
  if (idleMs >= STALL_STEPS_MS[0]) return { level: 1, text: `${base}: taking a moment…` };
  return { level: 0, text: `${base}…` };
}

/** Phases in which the agent is acting, so the tail line has a clock and a shimmer. */
export const ACTIVE_PHASES: ReadonlySet<string> = new Set(["RUNNING"]);

/** Whether the unread divider needs adding: the Core's divider is lost when the person marks the task read, so the first read's position is kept. */
export function dividerOffsetOf(rows: readonly TranscriptRowView[]): string | null {
  const d = rows.find((r) => r.kind === "UNREAD_DIVIDER");
  return d ? d.offset : null;
}

/** Re-inserts a remembered unread divider ahead of the first top-level row at or after its offset. */
export function withDivider(rows: readonly TranscriptRowView[], dividerOffset: string | null, label: (n: number) => string): TranscriptRowView[] {
  if (dividerOffset === null || rows.some((r) => r.kind === "UNREAD_DIVIDER")) return [...rows];
  const at = BigInt(dividerOffset);
  const i = rows.findIndex((r) => BigInt(r.offset) >= at && r.kind !== "TIME_BOUNDARY");
  if (i <= 0) return [...rows];
  const unread = rows.slice(i).filter(isMessage).length;
  const divider: TranscriptRowView = { ordinal: 0, rowId: "divider:remembered", kind: "UNREAD_DIVIDER", offset: dividerOffset, lastOffset: dividerOffset, atMs: 0, turnId: "", hints: { renderable: true, groupable: false, hasReasoning: false, durationMs: 0, shortText: label(unread), linesAdded: 0, linesRemoved: 0, status: "" }, text: label(unread), textTruncated: false, textRef: "", children: [], facts: { type: "none" } };
  return [...rows.slice(0, i), divider, ...rows.slice(i)];
}
