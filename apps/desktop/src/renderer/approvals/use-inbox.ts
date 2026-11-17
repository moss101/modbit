/**
 * Inbox's reads (REQ-PX-206). Nothing is kept here but the last answer: the updates are main's record with the Core's read
 * state, the proposals are the Core's. Each is read again when the Core's events pass, held briefly so a burst reads once.
 */
import { useCallback, useEffect, useRef, useState } from "react";
import type { ModeProposalInfo } from "../../shared/control-types.ts";
import type { UpdatesView } from "../../shared/inbox-types.ts";

const HOLD_MS = 150;
/** While Inbox is open, or a proposal is pending, the Core's proposals are read about once a second: one waits 15 seconds for an answer. */
const PROPOSALS_EVERY_MS = 1000;
/** The status classes of a task that is still running or waiting: only such a task can have a pending proposal. */
const LIVE = new Set(["RUNNING", "WAITING", "NEEDS_ATTENTION"]);

interface WireEventLike {
  taskId: string | null;
  aggregateType: string;
}

function useHeldRefresh(load: () => Promise<void>): () => void {
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  useEffect(
    () => () => {
      if (timer.current) clearTimeout(timer.current);
    },
    [],
  );
  return useCallback(() => {
    if (timer.current) return;
    timer.current = setTimeout(() => {
      timer.current = null;
      void load();
    }, HOLD_MS);
  }, [load]);
}

export interface InboxUpdates {
  view: UpdatesView | null;
  error: string | null;
  markRead: (id: string) => Promise<void>;
  markAllRead: () => Promise<void>;
  clear: () => Promise<void>;
}

export function useInboxUpdates(connected: boolean): InboxUpdates {
  const [view, setView] = useState<UpdatesView | null>(null);
  const [error, setError] = useState<string | null>(null);
  const live = useRef(true);
  useEffect(() => {
    live.current = true;
    return () => {
      live.current = false;
    };
  }, []);
  const take = useCallback((p: Promise<UpdatesView>) =>
    p.then(
      (v) => {
        if (live.current) {
          setView(v);
          setError(null);
        }
      },
      (e: Error) => {
        if (live.current) setError(e.message.replace(/^Error invoking remote method '[^']*': (Error: )?/, ""));
      },
    ), []);
  const load = useCallback(() => take(window.modbit.inboxUpdates()), [take]);
  const refresh = useHeldRefresh(load);
  useEffect(() => {
    if (connected) void load();
  }, [connected, load]);
  useEffect(() => {
    const offChanged = window.modbit.onInboxChanged(refresh);
    const offEvent = window.modbit.onEvent((raw) => {
      const e = raw as WireEventLike;
      if (e.taskId && e.aggregateType !== "assistant_stream") refresh();
    });
    return () => {
      offChanged();
      offEvent();
    };
  }, [refresh]);
  return {
    view,
    error,
    markRead: useCallback((id: string) => take(window.modbit.markUpdateRead(id)), [take]),
    markAllRead: useCallback(() => take(window.modbit.markAllUpdatesRead()), [take]),
    clear: useCallback(() => take(window.modbit.clearUpdates()), [take]),
  };
}

/** Every proposal the Core holds pending across the session's live tasks, with the skew of the Core's clock to this machine's. */
export function useSessionProposals(sessionId: string | null, connected: boolean, everyMs: number = PROPOSALS_EVERY_MS): { proposals: ModeProposalInfo[]; skewMs: number; refresh: () => void } {
  const [state, setState] = useState<{ proposals: ModeProposalInfo[]; skewMs: number }>({ proposals: [], skewMs: 0 });
  const inFlight = useRef(false);
  const load = useCallback(async () => {
    if (!sessionId || inFlight.current) return;
    inFlight.current = true;
    try {
      const headers = await window.modbit.agentHeaders(sessionId);
      const lists = await Promise.all(headers.headers.filter((h) => LIVE.has(h.statusClass)).map((h) => window.modbit.modeProposals(h.taskId).catch(() => null)));
      const proposals: ModeProposalInfo[] = [];
      let skewMs = 0;
      for (const l of lists) {
        if (!l) continue;
        skewMs = l.nowMs - Date.now();
        for (const p of l.proposals) if (p.status === "PENDING") proposals.push(p);
      }
      proposals.sort((a, b) => a.proposedAtMs - b.proposedAtMs);
      setState({ proposals, skewMs });
    } catch {
      // Unreachable Core: the last list stays.
    } finally {
      inFlight.current = false;
    }
  }, [sessionId]);
  const refresh = useHeldRefresh(load);
  useEffect(() => {
    if (!sessionId || !connected) return;
    void load();
    const t = setInterval(() => void load(), everyMs);
    return () => clearInterval(t);
  }, [sessionId, connected, load, everyMs]);
  // A proposal is recorded on its task's log: a task's event reads the list again at once.
  useEffect(() => {
    const off = window.modbit.onEvent((raw) => {
      const e = raw as WireEventLike;
      if (e.taskId && e.aggregateType !== "assistant_stream") refresh();
    });
    return off;
  }, [refresh]);
  return { ...state, refresh };
}
