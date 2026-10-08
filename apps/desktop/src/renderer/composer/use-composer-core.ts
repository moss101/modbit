/**
 * The composer's Core-held state for one task (REQ-PX-054..056): the mode and
 * execution posture, the durable input queue, the send behaviour and the
 * background terminals. Each is a read of a Core fact, re-read when the
 * task's events say something changed; nothing is kept that a read does not
 * replace, so a reload, a restart or another client's change reaches the same
 * picture. The model catalog and the slash inventory are read on demand.
 */
import { useCallback, useEffect, useRef, useState } from "react";
import type { TerminalViewJson } from "../../preload/preload.ts";
import type { ModelCatalogView, PostureView, QueueView, SendBehaviorState, SlashInventoryView } from "../../shared/composer-types.ts";

const HOLD_MS = 80;

interface EventLike {
  taskId?: string | null;
  eventType?: string;
  aggregateType?: string;
}

export interface ComposerCore {
  posture: PostureView | null;
  queue: QueueView | null;
  behavior: SendBehaviorState | null;
  terminals: TerminalViewJson[];
  /** The last read failed (the Core is away); the values above are the last good ones. */
  stale: boolean;
  refresh: () => void;
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
  const again = useRef(false);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const live = useRef(true);
  const idRef = useRef(taskId);
  idRef.current = taskId;

  const read = useCallback(async () => {
    if (inFlight.current) {
      again.current = true;
      return;
    }
    inFlight.current = true;
    const tid = idRef.current;
    try {
      const [p, q, b, t] = await Promise.all([window.modbit.composerPosture(tid), window.modbit.composerQueue(tid), window.modbit.composerSendBehavior(tid), window.modbit.terminalList(tid)]);
      if (!live.current || idRef.current !== tid) return;
      setPosture(p);
      setQueue(q);
      setBehavior(b);
      setTerminals(t.terminals.filter((x) => x.taskId === tid));
      setStale(false);
    } catch {
      if (live.current) setStale(true);
    } finally {
      inFlight.current = false;
      if (again.current) {
        again.current = false;
        void read();
      }
    }
  }, []);

  const refresh = useCallback(() => {
    if (timer.current) return;
    timer.current = setTimeout(() => {
      timer.current = null;
      void read();
    }, HOLD_MS);
  }, [read]);

  useEffect(() => {
    live.current = true;
    setPosture(null);
    setQueue(null);
    setBehavior(null);
    setTerminals([]);
    if (connected) void read();
    return () => {
      live.current = false;
    };
  }, [taskId, connected, read]);

  useEffect(() => {
    const off = window.modbit.onEvent((raw) => {
      const e = raw as EventLike;
      if (e.taskId !== taskId) return;
      // A stream's pieces change none of these; its closing record can (a turn ended: the queue drained).
      if (e.aggregateType === "assistant_stream" && e.eventType === "AssistantTextDelta") return;
      refresh();
    });
    return () => {
      off();
      if (timer.current) clearTimeout(timer.current);
      timer.current = null;
    };
  }, [taskId, refresh]);

  return { posture, queue, behavior, terminals, stale, refresh, setPosture, setBehavior };
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
