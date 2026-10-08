import { useCallback, useEffect, useRef, useState } from "react";
import type { DockApprovalView } from "../../shared/control-types.ts";
import { deckOrder } from "./model.ts";

interface WireEventLike {
  taskId: string | null;
  aggregateType: string;
  eventType: string;
}

const HOLD_MS = 120;

/**
 * The approvals of the session still waiting for the person, as the Core lists them (so they are there after a
 * thread switch, a renderer reload and a Core restart: nothing is kept here but the last answer). The open task's come
 * first, then the other agents', each group oldest first. Re-read when an event passes, held briefly so a burst reads once.
 */
export function useDockApprovals(sessionId: string | null, taskId: string | null, connected: boolean): { approvals: DockApprovalView[]; loaded: boolean; refresh: () => void } {
  const [approvals, setApprovals] = useState<DockApprovalView[]>([]);
  const [loaded, setLoaded] = useState(false);
  const key = useRef({ sessionId, taskId });
  key.current = { sessionId, taskId };
  const inFlight = useRef(false);
  const again = useRef(false);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const load = useCallback(async () => {
    const { sessionId: sid, taskId: tid } = key.current;
    if (!sid || !tid) return;
    if (inFlight.current) {
      again.current = true;
      return;
    }
    inFlight.current = true;
    try {
      const list = await window.modbit.dockApprovals(sid);
      if (key.current.taskId === tid) {
        setApprovals(deckOrder(list, tid));
        setLoaded(true);
      }
    } catch {
      // Unreachable Core: the last list stays (a pending approval is never dropped because a read failed).
    } finally {
      inFlight.current = false;
      if (again.current) {
        again.current = false;
        void load();
      }
    }
  }, []);

  const refresh = useCallback(() => {
    if (timer.current) return;
    timer.current = setTimeout(() => {
      timer.current = null;
      void load();
    }, HOLD_MS);
  }, [load]);

  useEffect(() => {
    setApprovals([]);
    setLoaded(false);
  }, [taskId]);

  useEffect(() => {
    if (!sessionId || !taskId || !connected) return;
    void load();
  }, [sessionId, taskId, connected, load]);

  useEffect(() => {
    if (!taskId) return;
    const off = window.modbit.onEvent((raw) => {
      const e = raw as WireEventLike;
      if (!e.taskId || e.aggregateType === "assistant_stream") return;
      refresh();
    });
    return () => {
      off();
      if (timer.current) clearTimeout(timer.current);
      timer.current = null;
    };
  }, [taskId, refresh]);

  return { approvals, loaded, refresh };
}
