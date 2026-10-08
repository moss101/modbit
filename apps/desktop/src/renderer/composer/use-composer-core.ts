/**
 * The composer's Core-held state for one task (REQ-PX-054..056): the mode and
 * execution posture, the durable input queue, the send behaviour and the
 * background terminals. Each is a read of a Core fact, re-read when an event
 * that can change it arrives; nothing is kept that a read does not replace, so
 * a reload, a restart or another client's change reaches the same picture.
 * The model catalog and the slash inventory are read on demand.
 *
 * A streaming turn is chatty and none of its records (the assistant stream's,
 * a model call's start) changes any of these, so they cost no read: only the
 * records named in `kindsOf` do, and a burst of them is read once.
 */
import { useCallback, useEffect, useRef, useState } from "react";
import type { TerminalViewJson } from "../../preload/preload.ts";
import type { ModelCatalogView, PostureView, QueueView, SendBehaviorState, SlashInventoryView } from "../../shared/composer-types.ts";

// Events arrive in bursts; the reads happen once the burst has been quiet this long.
const QUIET_MS = 200;
/** A burst that never goes quiet (a long run of tool calls) is still read this often. */
const MAX_WAIT_MS = 1000;

type Part = "posture" | "queue" | "behavior" | "terminals";
const ALL: readonly Part[] = ["posture", "queue", "behavior", "terminals"];

interface EventLike {
  taskId?: string | null;
  eventType?: string;
  aggregateType?: string;
}

/** Which of the composer's facts an event can have changed. */
export function kindsOf(e: EventLike): Part[] {
  if (e.aggregateType === "assistant_stream") return [];
  const t = e.eventType ?? "";
  const out: Part[] = [];
  if (/^(TaskMode|TaskPosture|ExecutionPreference)/.test(t)) out.push("posture");
  if (/^(TaskInput|TaskInterrupt|TaskSteered|SendBehavior|Run(Started|Resumed|Suspended|Completed|Failed|Cancelled)|Task(Queued|Started|Resumed|Completed|Failed|Cancelled|Waiting|Paused|Suspended|NeedsAttention|ReadyForReview)|TurnCompleted|ProtocolStateResumed)/.test(t)) out.push("queue");
  if (t === "SendBehaviorSet") out.push("behavior");
  if (/^(Terminal|BackgroundProcess|BackgroundWake)/.test(t)) out.push("terminals");
  return out;
}

export interface ComposerCore {
  posture: PostureView | null;
  queue: QueueView | null;
  behavior: SendBehaviorState | null;
  terminals: TerminalViewJson[];
  /** The last read failed (the Core is away); the values above are the last good ones. */
  stale: boolean;
  /** Reads everything now (what the person's own action triggers: the answer should not wait). */
  refreshNow: () => void;
  setPosture: (p: PostureView) => void;
  setBehavior: (b: SendBehaviorState) => void;
}

export function useComposerCore(taskId: string, connected: boolean): ComposerCore {
  const [posture, setPosture] = useState<PostureView | null>(null);
  const [queue, setQueue] = useState<QueueView | null>(null);
  const [behavior, setBehavior] = useState<SendBehaviorState | null>(null);
  const [terminals, setTerminals] = useState<TerminalViewJson[]>([]);
  const [stale, setStale] = useState(false);
  const inFlight = useRef(false);
  const queued = useRef<Set<Part>>(new Set());
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const pendingSince = useRef(0);
  const live = useRef(true);
  const idRef = useRef(taskId);
  idRef.current = taskId;

  const read = useCallback(async (parts: readonly Part[]) => {
    for (const p of parts) queued.current.add(p);
    if (inFlight.current) return;
    inFlight.current = true;
    try {
      while (queued.current.size > 0) {
        const want = new Set(queued.current);
        queued.current.clear();
        const tid = idRef.current;
        try {
          const [p, q, b, t] = await Promise.all([
            want.has("posture") ? window.modbit.composerPosture(tid) : null,
            want.has("queue") ? window.modbit.composerQueue(tid) : null,
            want.has("behavior") ? window.modbit.composerSendBehavior(tid) : null,
            want.has("terminals") ? window.modbit.terminalList(tid) : null,
          ]);
          if (!live.current || idRef.current !== tid) return;
          if (p) setPosture(p);
          if (q) setQueue(q);
          if (b) setBehavior(b);
          if (t) setTerminals(t.terminals.filter((x) => x.taskId === tid));
          setStale(false);
        } catch {
          if (live.current) setStale(true);
        }
      }
    } finally {
      inFlight.current = false;
    }
  }, []);

  const later = useCallback(
    (parts: readonly Part[]) => {
      for (const p of parts) queued.current.add(p);
      const now = Date.now();
      if (!timer.current) pendingSince.current = now;
      else if (now - pendingSince.current >= MAX_WAIT_MS) return;
      else clearTimeout(timer.current);
      timer.current = setTimeout(() => {
        timer.current = null;
        void read([]);
      }, QUIET_MS);
    },
    [read],
  );

  useEffect(() => {
    live.current = true;
    setPosture(null);
    setQueue(null);
    setBehavior(null);
    setTerminals([]);
    if (connected) void read(ALL);
    return () => {
      live.current = false;
    };
  }, [taskId, connected, read]);

  useEffect(() => {
    const off = window.modbit.onEvent((raw) => {
      const e = raw as EventLike;
      if (e.taskId !== taskId) return;
      const parts = kindsOf(e);
      if (parts.length > 0) later(parts);
    });
    return () => {
      off();
      if (timer.current) clearTimeout(timer.current);
      timer.current = null;
    };
  }, [taskId, later]);

  const refreshNow = useCallback(() => void read(ALL), [read]);
  return { posture, queue, behavior, terminals, stale, refreshNow, setPosture, setBehavior };
}

/** The model catalog (with the task's own allow list applied), read when asked for and again when the Core says a preference changed. */
export function useCatalog(taskId: string, active: boolean, tick: string): { catalog: ModelCatalogView | null; error: string | null } {
  const [catalog, setCatalog] = useState<ModelCatalogView | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    if (!active) return;
    let cancelled = false;
    window.modbit
      .composerVariants(taskId)
      .then((c) => !cancelled && (setCatalog(c), setError(null)))
      .catch((e: Error) => !cancelled && setError(e.message));
    return () => {
      cancelled = true;
    };
  }, [taskId, active, tick]);
  return { catalog, error };
}

/** The slash inventory, read each time the menu opens: it is rebuilt from disk by the Core, so a skill trusted a moment ago is there. */
export function useSlashInventory(taskId: string, open: boolean): SlashInventoryView | null {
  const [inv, setInv] = useState<SlashInventoryView | null>(null);
  useEffect(() => {
    if (!open) return;
    let cancelled = false;
    window.modbit
      .composerSlash(taskId)
      .then((v) => !cancelled && setInv(v))
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [taskId, open]);
  return inv;
}
