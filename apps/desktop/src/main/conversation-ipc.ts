/**
 * Main's side of the agent list and the conversation (REQ-PX-046, REQ-PX-047):
 * the typed, argument-validated handlers over the Core's read projections
 * (GetAgentHeaders, GetTranscript, SearchConversations) and its two curation
 * commands (MarkRead, ArchiveTask). Every handler validates what the renderer
 * sent before it is forwarded (REQ-EV-0103); every result is converted to the
 * plain shapes of shared/conversation-types.ts. No state lives here: the Core
 * decides the status class, the density grouping, the unread marker and the
 * search hits, and the renderer shows them.
 */
import type { AgentHeader, AgentHeaders, ConversationSearchResults, TranscriptPage, TranscriptRow } from "@modbit/surface-protocol";
import { AgentStatusClass, SnippetSource, TranscriptDensity, TranscriptRowKind } from "@modbit/surface-protocol";
import type { CoreClient } from "@modbit/ide-adapter-core";
import { DENSITIES, STATUS_CLASSES, type PendingApprovalView, type OpenStreamView, type AgentHeadersView, type ConversationSearchView, type Density, type RowFacts, type RowKind, type StatusClass, type TranscriptPageView, type TranscriptRowView } from "../shared/conversation-types.ts";

const hexOf = (b: Uint8Array | undefined): string => Buffer.from(b ?? []).toString("hex");
const msOf = (t: { seconds: bigint; nanos: number } | undefined): number => (t ? Number(t.seconds) * 1000 + Math.floor(t.nanos / 1_000_000) : 0);

export function statusClassOf(c: AgentStatusClass): StatusClass {
  const name = AgentStatusClass[c];
  // The enum's names are the Core's: an unknown value is shown as the lowest class, never invented.
  return (STATUS_CLASSES as readonly string[]).includes(name ?? "") ? (name as StatusClass) : "ARCHIVED";
}

function densityName(d: TranscriptDensity): Density {
  const name = TranscriptDensity[d];
  return (DENSITIES as readonly string[]).includes(name ?? "") ? (name as Density) : "COMPACT";
}

function factsOf(r: TranscriptRow): RowFacts {
  const f = r.facts;
  switch (f.case) {
    case "user":
      return { type: "user", inputId: f.value.inputId, source: f.value.source, mode: f.value.mode, provenance: f.value.provenance, untrusted: f.value.untrusted };
    case "stream":
      return { type: "stream", streamId: f.value.streamId, phase: f.value.phase, firstOffset: f.value.firstOffset.toString(), lastOffset: f.value.lastOffset.toString(), deltaCount: f.value.deltaCount, contentHash: f.value.contentHash, abortSource: f.value.abortSource, abortCode: f.value.abortCode };
    case "tool":
      return { type: "tool", toolCallId: f.value.toolCallId, toolName: f.value.toolName, toolClass: f.value.toolClass, effectClass: f.value.effectClass, state: f.value.state, failureCode: f.value.failureCode, paths: f.value.paths };
    case "approval":
      return { type: "approval", kind: f.value.kind, approvalId: f.value.approvalId, toolCallId: f.value.toolCallId, toolName: f.value.toolName, effectClass: f.value.effectClass, state: f.value.state, resolver: f.value.resolver };
    case "group":
      return { type: "group", turnId: f.value.turnId, label: f.value.label, steps: f.value.steps, reads: f.value.reads, searches: f.value.searches, commands: f.value.commands, edits: f.value.edits, others: f.value.others, linesAdded: f.value.linesAdded, linesRemoved: f.value.linesRemoved, durationMs: Number(f.value.durationMs), open: f.value.open };
    case "footer":
      return { type: "footer", turnId: f.value.turnId, runId: f.value.runId, outcome: f.value.outcome, failureCode: f.value.failureCode, durationMs: Number(f.value.durationMs), inputTokens: f.value.inputTokens.toString(), outputTokens: f.value.outputTokens.toString() };
    case "tail":
      return { type: "tail", phase: f.value.phase, label: f.value.label, detail: f.value.detail };
    case "boundary":
      return { type: "boundary", gapMs: Number(f.value.gapMs), day: f.value.day };
    default:
      return { type: "none" };
  }
}

export function rowView(r: TranscriptRow): TranscriptRowView {
  const h = r.hints;
  return {
    ordinal: r.ordinal,
    rowId: r.rowId,
    kind: (TranscriptRowKind[r.kind] ?? "UNSPECIFIED") as RowKind,
    offset: r.offset.toString(),
    lastOffset: r.lastOffset.toString(),
    atMs: msOf(r.at),
    turnId: r.turnId,
    hints: { renderable: h?.renderable ?? true, groupable: h?.groupable ?? false, hasReasoning: h?.hasReasoning ?? false, durationMs: Number(h?.durationMs ?? 0n), shortText: h?.shortText ?? "", linesAdded: h?.linesAdded ?? 0, linesRemoved: h?.linesRemoved ?? 0, status: h?.status ?? "" },
    text: r.text,
    textTruncated: r.textTruncated,
    textRef: r.textRef,
    children: r.children.map(rowView),
    facts: factsOf(r),
  };
}

export function pageView(taskId: string, p: TranscriptPage): TranscriptPageView {
  return {
    taskId,
    density: densityName(p.density),
    rows: p.rows.map(rowView),
    totalRows: p.totalRows,
    nextAfterRow: p.nextAfterRow,
    hasMore: p.hasMore,
    asOfOffset: p.asOfOffset.toString(),
    lastOffset: p.lastOffset.toString(),
    taskState: p.taskState,
    readOffset: p.readOffset.toString(),
    eventsRead: p.eventsRead.toString(),
  };
}

function headerView(h: AgentHeader) {
  return {
    taskId: hexOf(h.taskId?.value),
    sessionId: hexOf(h.sessionId?.value),
    workspaceRoot: h.workspaceRoot,
    title: h.title,
    subtitle: h.subtitle,
    createdAtMs: msOf(h.createdAt),
    updatedAtMs: msOf(h.updatedAt),
    statusClass: statusClassOf(h.statusClass),
    statusLabel: h.statusLabel,
    unread: h.unread,
    pendingApproval: h.pendingApproval,
    pendingPlan: h.pendingPlan,
    contextPercent: h.contextPercent,
    filesChanged: h.filesChanged,
    linesAdded: h.linesAdded,
    linesRemoved: h.linesRemoved,
    lastCheckpointAtMs: msOf(h.lastCheckpointAt),
    subagent: h.subagent,
    archived: h.archived,
    executionLocation: h.executionLocation,
    origin: h.origin,
    taskState: h.taskState,
    lastOffset: h.lastOffset.toString(),
    readOffset: h.readOffset.toString(),
    attentionItems: h.attentionItems,
    projectId: hexOf(h.projectId?.value),
  };
}

export function headersView(sessionId: string, v: AgentHeaders): AgentHeadersView {
  return { sessionId, headers: v.headers.map(headerView), lastOffset: v.lastOffset.toString(), eventsRead: v.eventsRead.toString(), objectsRead: v.objectsRead.toString() };
}

export function searchView(sessionId: string, v: ConversationSearchResults): ConversationSearchView {
  return {
    sessionId,
    query: v.query,
    totalHits: v.totalHits,
    hasMore: v.hasMore,
    asOfOffset: v.asOfOffset.toString(),
    hits: v.hits.map((h) => ({
      taskId: hexOf(h.taskId?.value),
      title: h.title,
      statusClass: statusClassOf(h.statusClass),
      statusLabel: h.statusLabel,
      archived: h.archived,
      titleMatched: h.titleMatched,
      matchedRows: h.matchedRows,
      score: h.score,
      lastOffset: h.lastOffset.toString(),
      snippets: h.snippets.map((s) => ({ rowId: s.rowId, kind: (TranscriptRowKind[s.kind] ?? "UNSPECIFIED") as RowKind, source: SnippetSource[s.source] ?? "UNSPECIFIED", offset: s.offset.toString(), turnId: s.turnId, text: s.text, cutBefore: s.cutBefore, cutAfter: s.cutAfter, matches: s.matches.map((m) => ({ start: m.start, end: m.end })), score: s.score })),
    })),
  };
}

// ---------------------------------------------------------------- validation

const bad = (what: string): never => {
  throw new Error(`BAD_ARGUMENT: ${what}`);
};

export function validDensity(v: unknown): TranscriptDensity {
  if (v === undefined || v === null) return TranscriptDensity.COMPACT;
  if (typeof v !== "string" || !(DENSITIES as readonly string[]).includes(v)) return bad("density must be COMPACT, BALANCED or DETAILED");
  return TranscriptDensity[v as Density];
}

/** An optional unsigned count: undefined/null = 0; otherwise an integer in 0..max. */
function count(v: unknown, what: string, max: number): number {
  if (v === undefined || v === null) return 0;
  if (typeof v !== "number" || !Number.isInteger(v) || v < 0 || v > max) return bad(`${what} must be an integer 0..${max}`);
  return v;
}

/** An optional log offset as a decimal string. */
function offsetOf(v: unknown, what: string): bigint {
  if (v === undefined || v === null || v === "") return 0n;
  if (typeof v !== "string" || !/^\d{1,19}$/.test(v)) return bad(`${what} must be a decimal offset`);
  return BigInt(v);
}

// ------------------------------------------------------- open stream text

/**
 * The text of assistant streams that are still open, seen by this process as
 * the Core's events passed through it (docs/65 AFW-C02). The transcript
 * projection carries a stream's text only once it completes, so a renderer
 * that opens (or reloads into) a conversation mid-stream asks main for what
 * the open stream said so far, then follows the deltas live. It is a view
 * buffer, not a store: an entry lives only until the stream's closing event,
 * is bounded like the Core's own stream limit, and the Core stays the only
 * place a message exists. The text is model output: data, never instruction.
 */
const MAX_OPEN_STREAMS = 32;
const MAX_STREAM_CHARS = 4 * 1024 * 1024;
const openStreams = new Map<string, OpenStreamView>();

export function observeStreamEvent(ev: { eventType: string; aggregateType: string; aggregateId: string; taskId: string | null; payload: unknown }): void {
  if (ev.aggregateType !== "assistant_stream") return;
  if (ev.eventType === "AssistantMessageCompleted" || ev.eventType === "AssistantMessageAborted") {
    openStreams.delete(ev.aggregateId);
    return;
  }
  if (ev.eventType !== "AssistantTextDelta" || !ev.taskId) return;
  const p = ev.payload as { kind?: unknown; sequence?: unknown; text?: unknown } | null;
  if (!p || p.kind !== "TEXT" || typeof p.text !== "string" || typeof p.sequence !== "number") return;
  let s = openStreams.get(ev.aggregateId);
  if (!s) {
    if (openStreams.size >= MAX_OPEN_STREAMS) openStreams.delete(openStreams.keys().next().value as string);
    s = { streamId: ev.aggregateId, taskId: ev.taskId, sequence: 0, text: "" };
    openStreams.set(ev.aggregateId, s);
  }
  // A redelivered or out-of-order piece never rewrites what was already said.
  if (p.sequence !== s.sequence + 1 || s.text.length + p.text.length > MAX_STREAM_CHARS) return;
  s.sequence = p.sequence;
  s.text += p.text;
}

export function openStreamsOf(taskId: string): OpenStreamView[] {
  return [...openStreams.values()].filter((s) => s.taskId === taskId).map((s) => ({ ...s }));
}

export interface Registrar {
  handle: (channel: string, fn: (...args: unknown[]) => unknown) => void;
  client: () => CoreClient;
  sessionId: (v: unknown) => string;
  taskId: (v: unknown) => string;
  /** Takes the session lease before a mutating command, as the other task commands do. */
  lease: (c: CoreClient, sessionId: string) => Promise<void>;
}

/** The commands of the agent list and the conversation. */
export function registerConversationHandlers(r: Registrar): void {
  r.handle("agents:headers", async (...a) => {
    const [sessionId, includeArchived] = a;
    const sid = r.sessionId(sessionId);
    if (includeArchived !== undefined && typeof includeArchived !== "boolean") bad("includeArchived must be a boolean");
    return headersView(sid, await r.client().getAgentHeaders(sid, includeArchived === true));
  });
  r.handle("conversation:transcript", async (...a) => {
    const [taskId, options] = a;
    const tid = r.taskId(taskId);
    const o = (options ?? {}) as Record<string, unknown>;
    if (typeof o !== "object" || Array.isArray(o)) bad("options must be an object");
    const page = await r.client().getTranscript(tid, { density: validDensity(o.density), afterRow: count(o.afterRow, "afterRow", 1_000_000), limit: count(o.limit, "limit", 500), asOfOffset: offsetOf(o.asOfOffset, "asOfOffset") });
    return pageView(tid, page);
  });
  r.handle("conversation:approvals", async (...a) => {
    const sid = r.sessionId(a[0]);
    const list = await r.client().listApprovals(sid);
    return list.approvals
      .filter((v) => v.status === "REQUESTED")
      .map((v): PendingApprovalView => ({ approvalId: hexOf(v.approvalId?.value), taskId: hexOf(v.taskId?.value), toolCallId: hexOf(v.toolCallId?.value), toolName: v.toolName, effectClass: v.effectClass, intentHash: v.intentHash, requestedAtMs: Number(v.requestedAtMs) }));
  });
  r.handle("conversation:openStreams", (...a) => openStreamsOf(r.taskId(a[0])));
  r.handle("conversation:search", async (...a) => {
    const [sessionId, query, options] = a;
    const sid = r.sessionId(sessionId);
    if (typeof query !== "string" || query.trim().length === 0 || query.length > 256) bad("query must be 1..256 characters");
    const o = (options ?? {}) as Record<string, unknown>;
    if (typeof o !== "object" || Array.isArray(o)) bad("options must be an object");
    if (o.includeArchived !== undefined && typeof o.includeArchived !== "boolean") bad("includeArchived must be a boolean");
    const taskId = o.taskId === undefined || o.taskId === "" ? undefined : r.taskId(o.taskId);
    const res = await r.client().searchConversations(sid, (query as string).trim(), { limit: count(o.limit, "limit", 100), maxSnippets: count(o.maxSnippets, "maxSnippets", 10), includeArchived: o.includeArchived === true, ...(taskId ? { taskId } : {}) });
    return searchView(sid, res);
  });
  r.handle("conversation:markRead", async (...a) => {
    const [sessionId, taskId, upTo] = a;
    const sid = r.sessionId(sessionId);
    const tid = r.taskId(taskId);
    const up = offsetOf(upTo, "upToOffset");
    const c = r.client();
    await r.lease(c, sid);
    const res = await c.markRead(sid, tid, up);
    return { taskId: tid, readOffset: res.readOffset.toString(), offset: res.offset.toString() };
  });
  r.handle("conversation:archive", async (...a) => {
    const [sessionId, taskId, archived] = a;
    const sid = r.sessionId(sessionId);
    const tid = r.taskId(taskId);
    if (typeof archived !== "boolean") bad("archived must be a boolean");
    const c = r.client();
    await r.lease(c, sid);
    const res = await c.archiveTask(sid, tid, archived === true);
    return { taskId: tid, archived: res.archived, offset: res.offset.toString() };
  });
}
