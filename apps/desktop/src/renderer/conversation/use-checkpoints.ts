import { useCallback, useEffect, useRef, useState } from "react";
import type { CheckpointListInfo } from "../../shared/control-types.ts";

interface WireEventLike {
  taskId: string | null;
  aggregateType: string;
}

const HOLD_MS = 250;

/** The task's checkpoints as the Core lists them, re-read when the task's events pass (a turn boundary records one, a restore records two). */
export function useCheckpoints(taskId: string | null, connected: boolean): { list: CheckpointListInfo | null; refresh: () => void; reload: () => Promise<void> } {
  const [list, setList] = useState<CheckpointListInfo | null>(null);
  const key = useRef(taskId);
  key.current = taskId;
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const inFlight = useRef(false);
  const again = useRef(false);

  const load = useCallback(async () => {
    const tid = key.current;
    if (!tid) return;
    if (inFlight.current) {
      again.current = true;
      return;
    }
    inFlight.current = true;
    try {
      const l = await window.modbit.checkpoints(tid);
      if (key.current === tid) setList(l);
    } catch {
      // The last list stays; a failed read never hides a checkpoint.
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
    setList(null);
  }, [taskId]);
  useEffect(() => {
    if (taskId && connected) void load();
  }, [taskId, connected, load]);
  useEffect(() => {
    if (!taskId) return;
    const off = window.modbit.onEvent((raw) => {
      const e = raw as WireEventLike;
      if (e.taskId === taskId && e.aggregateType !== "assistant_stream") refresh();
    });
    return () => {
      off();
      if (timer.current) clearTimeout(timer.current);
      timer.current = null;
    };
  }, [taskId, refresh]);
  return { list, refresh, reload: load };
}
