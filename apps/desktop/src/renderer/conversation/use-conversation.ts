/**
 * The conversation's data (REQ-PX-047): the Core's transcript projection for
 * one task, re-read when the task's events say it changed, and the live text of
 * the assistant streams that are still open, followed from the same event
 * subscription the rest of the renderer uses. Deltas never trigger a re-read
 * (a chatty stream would cost a full projection per piece); they grow the live
 * buffer, and the closing event or any other task event re-reads the
 * projection. A reload, a new window or a Core restart reaches the same state:
 * the projection from the log, plus what main has seen of any stream still open.
 */
import { useCallback, useEffect, useRef, useState } from "react";
import type { Density, PendingApprovalView, TranscriptRowView } from "../../shared/conversation-types.ts";
import { applyStreamEvent, dividerOffsetOf, reuseRows, seedStreams, splitTail, type LiveStreams, type StreamEventLike } from "./model.ts";

const PAGE = 500;
const MAX_PAGES = 40;
const REFETCH_HOLD_MS = 60;

interface WireEventLike extends StreamEventLike {
  sessionId?: string;
}

export interface ConversationData {
  rows: TranscriptRowView[];
  tail: TranscriptRowView | null;
  loaded: boolean;
  error: string | null;
  taskState: string;
  asOfOffset: string;
  lastOffset: string;
  readOffset: string;
  totalRows: number;
  live: LiveStreams;
  /** The protected effects of the session waiting for a decision, from the Core. */
  approvals: PendingApprovalView[];
  /** Wall-clock ms of the newest event or delta for this task (the stall ladder's clock). */
  lastActivityMs: number;
  /** Offset of the first unread row at open, kept after the person has read it. */
  dividerOffset: string | null;
  /** The Core's own tail-status wording was refreshed at this time. */
  refreshedAt: number;
  refetch: () => void;
}

const EMPTY: LiveStreams = new Map();

export function useConversation(taskId: string | null, sessionId: string | null, density: Density, connected: boolean): ConversationData {
  const [rows, setRows] = useState<TranscriptRowView[]>([]);
  const [tail, setTail] = useState<TranscriptRowView | null>(null);
  const [loaded, setLoaded] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [meta, setMeta] = useState({ taskState: "", asOfOffset: "0", lastOffset: "0", readOffset: "0", totalRows: 0 });
  const [live, setLive] = useState<LiveStreams>(EMPTY);
  const [approvals, setApprovals] = useState<PendingApprovalView[]>([]);
  const [lastActivityMs, setLastActivityMs] = useState(0);
  const [dividerOffset, setDividerOffset] = useState<string | null>(null);
  const [refreshedAt, setRefreshedAt] = useState(0);
  const prevRows = useRef<TranscriptRowView[]>([]);
  const inFlight = useRef(false);
  const again = useRef(false);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const key = useRef({ taskId, density, sessionId });
  key.current = { taskId, density, sessionId };
  const firstLoad = useRef(true);

  const load = useCallback(async () => {
    const { taskId: tid, density: d } = key.current;
    if (!tid) return;
    if (inFlight.current) {
      again.current = true;
      return;
    }
    inFlight.current = true;
    try {
      const all: TranscriptRowView[] = [];
      let after = 0;
      let asOf: string | undefined;
      let last: Awaited<ReturnType<typeof window.modbit.transcript>> | null = null;
      for (let i = 0; i < MAX_PAGES; i++) {
        // One consistent log across the pages of one read: the first page's offset is passed back.
        const page = await window.modbit.transcript(tid, { density: d, afterRow: after, limit: PAGE, ...(asOf ? { asOfOffset: asOf } : {}) });
        asOf ??= page.asOfOffset;
        all.push(...page.rows);
        last = page;
        if (!page.hasMore) break;
        after = page.nextAfterRow;
      }
      if (!last || key.current.taskId !== tid || key.current.density !== d) return;
      // The decisions this session is waiting on, with the exact intent each must name (a restart loses nothing: the Core lists them).
      const sid = key.current.sessionId;
      if (sid) {
        const pending = await window.modbit.pendingApprovals(sid).catch(() => null);
        if (pending) setApprovals(pending.filter((a) => a.taskId === tid));
      }
      const { body, tail: t } = splitTail(all);
      const reused = reuseRows(prevRows.current, body);
      prevRows.current = reused;
      if (firstLoad.current) {
        firstLoad.current = false;
        setDividerOffset(dividerOffsetOf(all));
      }
      setRows(reused);
      setTail(t);
      setMeta({ taskState: last.taskState, asOfOffset: last.asOfOffset, lastOffset: last.lastOffset, readOffset: last.readOffset, totalRows: last.totalRows });
      setLoaded(true);
      setError(null);
      setRefreshedAt(Date.now());
    } catch (e) {
      setError((e as Error).message.replace(/^Error invoking remote method '[^']*': (Error: )?/, ""));
    } finally {
      inFlight.current = false;
      if (again.current) {
        again.current = false;
        void load();
      }
    }
  }, []);

  const refetch = useCallback(() => {
    if (timer.current) return;
    timer.current = setTimeout(() => {
      timer.current = null;
      void load();
    }, REFETCH_HOLD_MS);
  }, [load]);

  // A different task or density starts a fresh read.
  useEffect(() => {
    prevRows.current = [];
    firstLoad.current = true;
    setRows([]);
    setTail(null);
    setLoaded(false);
    setError(null);
    setLive(EMPTY);
    setDividerOffset(null);
    setLastActivityMs(0);
  }, [taskId]);

  useEffect(() => {
    if (!taskId || !connected) return;
    let cancelled = false;
    void load().then(() => {
      // What main has seen of a stream that is still open (a reload mid-stream): seeded after the read, deduplicated by sequence.
      window.modbit
        .openStreams(taskId)
        .then((open) => {
          if (!cancelled && open.length > 0) setLive((l) => seedStreams(l, open, Date.now()));
        })
        .catch(() => {});
    });
    return () => {
      cancelled = true;
    };
  }, [taskId, density, connected, load]);

  useEffect(() => {
    if (!taskId) return;
    const off = window.modbit.onEvent((raw) => {
      const e = raw as WireEventLike;
      if (e.taskId !== taskId) return;
      setLastActivityMs(Date.now());
      if (e.aggregateType === "assistant_stream") {
        setLive((l) => applyStreamEvent(l, e, taskId));
        // The closing record changes the projection (the message is final or aborted); a piece does not.
        if (e.eventType !== "AssistantTextDelta") refetch();
        return;
      }
      refetch();
    });
    return () => {
      off();
      if (timer.current) clearTimeout(timer.current);
      timer.current = null;
    };
  }, [taskId, refetch]);

  return { rows, tail, loaded, error, ...meta, live, approvals, lastActivityMs, dividerOffset, refreshedAt, refetch };
}
