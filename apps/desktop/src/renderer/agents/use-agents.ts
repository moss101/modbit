/**
 * The agent list's data: the Core's header projection (GetAgentHeaders) kept
 * current by the event stream. A relevant event schedules one coalesced
 * refresh (leading edge, so a Core event shows on the list within a frame or
 * two of the read), never a recompute in the renderer. The list also refreshes
 * when the Core (re)connects, so a restarted Core's recovered state is what is
 * shown. Archive and its undo are the Core's ArchiveTask events.
 */
import { useCallback, useEffect, useRef, useState } from "react";
import type { AgentHeaderView } from "../../shared/conversation-types.ts";

/** Event types that cannot change a header; everything else with a task id can. */
const IGNORED_EVENT_TYPES = new Set(["AssistantTextDelta"]);

interface WireEventLike {
  eventType?: string;
  aggregateType?: string;
  taskId?: string | null;
}

export interface AgentsState {
  headers: AgentHeaderView[];
  loaded: boolean;
  error: string | null;
  /** Wall-clock ms at which the last refresh finished (for the latency probe and the relative ages). */
  refreshedAt: number;
  refresh: () => void;
  archive: (taskId: string) => Promise<{ ok: true } | { ok: false; message: string }>;
  restore: (taskId: string) => Promise<{ ok: true } | { ok: false; message: string }>;
  markRead: (taskId: string, upToOffset?: string) => Promise<void>;
}

export function useAgents(sessionId: string | null, includeArchived: boolean, connected: boolean): AgentsState {
  const [headers, setHeaders] = useState<AgentHeaderView[]>([]);
  const [loaded, setLoaded] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [refreshedAt, setRefreshedAt] = useState(0);
  const inFlight = useRef(false);
  const again = useRef(false);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const key = useRef({ sessionId, includeArchived });
  key.current = { sessionId, includeArchived };
  const alive = useRef(true);
  useEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
    };
  }, []);

  const run = useCallback(async () => {
    const { sessionId: sid, includeArchived: inc } = key.current;
    if (!sid) return;
    if (inFlight.current) {
      again.current = true;
      return;
    }
    inFlight.current = true;
    try {
      const v = await window.modbit.agentHeaders(sid, inc);
      // The answer is for the session and filter that asked; a stale one is dropped.
      if (alive.current && key.current.sessionId === sid && key.current.includeArchived === inc) {
        setHeaders(v.headers);
        setLoaded(true);
        setError(null);
        setRefreshedAt(Date.now());
      }
    } catch (e) {
      if (alive.current) setError((e as Error).message);
    } finally {
      inFlight.current = false;
      if (again.current) {
        again.current = false;
        void run();
      }
    }
  }, []);

  const refresh = useCallback(() => {
    // Leading edge with a short hold: bursts of events (a turn emits many) cost one read.
    if (timer.current) return;
    timer.current = setTimeout(() => {
      timer.current = null;
      void run();
    }, 30);
  }, [run]);

  useEffect(() => {
    if (!sessionId || !connected) return;
    void run();
  }, [sessionId, includeArchived, connected, run]);

  useEffect(() => {
    const off = window.modbit.onEvent((raw) => {
      const e = raw as WireEventLike;
      // A project event carries no task, but it moves the project id a header shows (PX-063).
      if ((!e.taskId && e.aggregateType !== "project") || (e.eventType && IGNORED_EVENT_TYPES.has(e.eventType))) return;
      refresh();
    });
    return () => {
      off();
      if (timer.current) clearTimeout(timer.current);
      timer.current = null;
    };
  }, [refresh]);

  const markRead = useCallback(
    async (taskId: string, upToOffset?: string) => {
      if (!sessionId) return;
      await window.modbit.markRead(sessionId, taskId, upToOffset).catch(() => {});
      refresh();
    },
    [sessionId, refresh],
  );

  const setArchived = useCallback(
    async (taskId: string, archived: boolean): Promise<{ ok: true } | { ok: false; message: string }> => {
      if (!sessionId) return { ok: false, message: "No session" };
      // Archiving a task that is still stopping is refused by the Core (TASK_RUNNING): ask again briefly.
      for (let attempt = 0; attempt < 12; attempt++) {
        try {
          await window.modbit.archiveTask(sessionId, taskId, archived);
          refresh();
          return { ok: true };
        } catch (e) {
          const msg = (e as Error).message;
          if (archived && msg.includes("TASK_RUNNING") && attempt < 11) {
            await new Promise((r) => setTimeout(r, 250));
            continue;
          }
          return { ok: false, message: msg.replace(/^Error invoking remote method '[^']*': (Error: )?/, "") };
        }
      }
      return { ok: false, message: "TASK_RUNNING: the task is still stopping" };
    },
    [sessionId, refresh],
  );

  const archive = useCallback((taskId: string) => setArchived(taskId, true), [setArchived]);
  const restore = useCallback((taskId: string) => setArchived(taskId, false), [setArchived]);

  return { headers, loaded, error, refreshedAt, refresh, archive, restore, markRead };
}
