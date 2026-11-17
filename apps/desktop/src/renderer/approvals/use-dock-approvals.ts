import { useCallback, useEffect, useRef, useState } from "react";
import type { DockApprovalView, ExpiredApprovalView } from "../../shared/control-types.ts";
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
 * With no task open (home's waiting list, REQ-PX-205) every pending approval of the session is listed, oldest first, and
 * nothing is said about expiries: those are named on the card of the task that saw them.
 */
export function useDockApprovals(sessionId: string | null, taskId: string | null, connected: boolean, relevant?: (e: { eventType?: string }) => boolean): { approvals: DockApprovalView[]; expired: ExpiredApprovalView[]; loaded: boolean; refresh: () => void } {
  const [approvals, setApprovals] = useState<DockApprovalView[]>([]);
  // REQ-PX-058: approvals this card saw waiting that the Core has since closed as expired. They are named, never silently dropped.
  const [expired, setExpired] = useState<ExpiredApprovalView[]>([]);
  const seen = useRef(new Set<string>());
  const [loaded, setLoaded] = useState(false);
  // REQ-PX-206: a reader that knows which events can change the list (Inbox's count in the shell) reads on those only.
  const relevantRef = useRef(relevant);
  relevantRef.current = relevant;
  const key = useRef({ sessionId, taskId });
  key.current = { sessionId, taskId };
  const inFlight = useRef(false);
  const again = useRef(false);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const load = useCallback(async () => {
    const { sessionId: sid, taskId: tid } = key.current;
    if (!sid) return;
    if (inFlight.current) {
      again.current = true;
      return;
    }
    inFlight.current = true;
    try {
      const list = await window.modbit.dockApprovals(sid);
      for (const a of list) seen.current.add(a.approvalId);
      const closed = tid && seen.current.size > 0 ? await window.modbit.expiredApprovals(sid, tid) : [];
      if (key.current.taskId === tid) {
        setApprovals(deckOrder(list, tid));
        setExpired(closed.filter((e) => seen.current.has(e.approvalId)));
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
    setExpired([]);
    seen.current = new Set();
    setLoaded(false);
  }, [taskId]);

  useEffect(() => {
    if (!sessionId || !connected) return;
    void load();
  }, [sessionId, taskId, connected, load]);

  useEffect(() => {
    const off = window.modbit.onEvent((raw) => {
      const e = raw as WireEventLike;
      if (!e.taskId || e.aggregateType === "assistant_stream") return;
      if (relevantRef.current && !relevantRef.current(e)) return;
      refresh();
    });
    return () => {
      off();
      if (timer.current) clearTimeout(timer.current);
      timer.current = null;
    };
  }, [taskId, refresh]);

  return { approvals, expired, loaded, refresh };
}
