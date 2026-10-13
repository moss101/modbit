import { useCallback, useEffect, useRef, useState } from "react";
import type { ModeProposalListInfo } from "../../shared/control-types.ts";

interface WireEventLike {
  taskId: string | null;
  aggregateType: string;
  eventType: string;
}

const HOLD_MS = 120;
const norm = (id: string): string => id.replace(/-/g, "").toLowerCase();
/** While a proposal is pending the Core is asked again about once a second, so its expiry is seen even if an event were missed. */
const WHILE_PENDING_MS = 1000;

/**
 * The open task's mode-switch proposals as the Core lists them, with the skew
 * between the Core's clock and this machine's at the moment of the read. Nothing
 * is kept here but the last answer: a thread switch, a reload and a Core restart
 * find the same facts because the Core lists them.
 */
export function useModeProposals(taskId: string | null, connected: boolean): { list: ModeProposalListInfo | null; skewMs: number; refresh: () => void } {
  const [state, setState] = useState<{ list: ModeProposalListInfo | null; skewMs: number }>({ list: null, skewMs: 0 });
  const key = useRef(taskId);
  key.current = taskId;
  const inFlight = useRef(false);
  const again = useRef(false);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const load = useCallback(async () => {
    const tid = key.current;
    if (!tid) return;
    if (inFlight.current) {
      again.current = true;
      return;
    }
    inFlight.current = true;
    try {
      const list = await window.modbit.modeProposals(tid);
      if (key.current === tid) setState({ list, skewMs: list.nowMs - Date.now() });
    } catch {
      // Unreachable Core: the last list stays.
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
    setState({ list: null, skewMs: 0 });
    if (taskId && connected) void load();
  }, [taskId, connected, load]);

  useEffect(() => {
    if (!taskId) return;
    const off = window.modbit.onEvent((raw) => {
      const e = raw as WireEventLike;
      if (!e.taskId || norm(e.taskId) !== norm(taskId) || e.aggregateType === "assistant_stream") return;
      refresh();
    });
    return () => {
      off();
      if (timer.current) clearTimeout(timer.current);
      timer.current = null;
    };
  }, [taskId, refresh]);

  const pending = state.list?.proposals.some((p) => p.status === "PENDING") ?? false;
  useEffect(() => {
    if (!pending || !connected) return;
    const t = setInterval(() => void load(), WHILE_PENDING_MS);
    return () => clearInterval(t);
  }, [pending, connected, load]);

  return { list: state.list, skewMs: state.skewMs, refresh };
}
