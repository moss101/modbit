/**
 * The worktree list's data: the Core's ListWorktrees (it inspects the disk
 * itself; nothing is listed from a renderer-side scan), read again when a
 * workspace event or a task ending says it may have changed, and on demand.
 * A read that arrives after a newer request is dropped.
 */
import { useCallback, useEffect, useRef, useState } from "react";
import type { WorktreeListInfo } from "../../shared/project-types.ts";

interface WireEventLike {
  eventType?: string;
  aggregateType?: string;
}

/** Task events that end or start work and so change what a worktree is. */
const TASK_EDGES = /^Task(Started|Completed|Failed|Cancelled|CancelRequested)$/;

export interface WorktreesState {
  list: WorktreeListInfo | null;
  loaded: boolean;
  error: string | null;
  refresh: () => Promise<void>;
}

export function useWorktrees(connected: boolean): WorktreesState {
  const [list, setList] = useState<WorktreeListInfo | null>(null);
  const [loaded, setLoaded] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const seq = useRef(0);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const alive = useRef(true);
  useEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
    };
  }, []);

  const refresh = useCallback(async () => {
    const mine = ++seq.current;
    try {
      const l = await window.modbit.listWorktrees({});
      if (alive.current && mine === seq.current) {
        setList(l);
        setLoaded(true);
        setError(null);
      }
    } catch (e) {
      if (alive.current && mine === seq.current) setError((e as Error).message);
    }
  }, []);

  useEffect(() => {
    if (connected) void refresh();
  }, [connected, refresh]);

  useEffect(() => {
    const off = window.modbit.onEvent((raw) => {
      const e = raw as WireEventLike;
      if (e.aggregateType !== "workspace" && !(e.eventType && TASK_EDGES.test(e.eventType))) return;
      if (timer.current) return;
      timer.current = setTimeout(() => {
        timer.current = null;
        void refresh();
      }, 150);
    });
    return () => {
      off();
      if (timer.current) clearTimeout(timer.current);
      timer.current = null;
    };
  }, [refresh]);

  return { list, loaded, error, refresh };
}
