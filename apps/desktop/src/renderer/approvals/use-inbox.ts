/**
 * Inbox's reads (REQ-PX-206). Nothing is kept here but the last answer: the updates are main's record with the Core's read
 * state, the proposals are the Core's. Each is read again when the Core's events pass, held briefly so a burst reads once.
 */
import { useCallback, useEffect, useRef, useState } from "react";
import type { ModeProposalInfo } from "../../shared/control-types.ts";
import type { UpdatesView } from "../../shared/inbox-types.ts";

const HOLD_MS = 150;
/** While a proposal is pending the Core's proposals are read about once a second: one waits 15 seconds for an answer. */
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
    // A task's lifecycle moves what is noticed; the read marker moves what is read. Nothing else changes the list.
    const offEvent = window.modbit.onEvent((raw) => {
      const e = raw as WireEventLike & { eventType?: string };
      if (e.taskId && (/^Task/.test(e.eventType ?? "") || e.eventType === "ConversationRead")) refresh();
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

/** The events after which the Core's list of proposals can differ (recorded on the task's log; an expiry is a decision too). */
export const PROPOSAL_EVENTS = new Set(["ModeSwitchProposed", "ModeSwitchDecided"]);
/** With no proposal pending the list is read again only on those events, and as a safety net this often. */
const PROPOSALS_IDLE_MS = 30_000;

/**
 * Every proposal the Core holds pending across the session's live tasks, with the skew of the Core's clock to this machine's.
 * Read on a proposal's own events; about once a second while one is pending (its 15 s are shown and its expiry seen); otherwise
 * every 30 s. One read at a time: a request while one is in flight is kept for one more read after it (trailing edge).
 */
export function useSessionProposals(sessionId: string | null, connected: boolean): { proposals: ModeProposalInfo[]; skewMs: number; refresh: () => void } {
  const [state, setState] = useState<{ proposals: ModeProposalInfo[]; skewMs: number }>({ proposals: [], skewMs: 0 });
  const inFlight = useRef(false);
  const again = useRef(false);
  const load = useCallback(async () => {
    if (!sessionId) return;
    if (inFlight.current) {
      again.current = true;
      return;
    }
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
      if (again.current) {
        again.current = false;
        void load();
      }
    }
  }, [sessionId]);
  const refresh = useHeldRefresh(load);
  const pending = state.proposals.length > 0;
  useEffect(() => {
    if (!sessionId || !connected) return;
    void load();
    const t = setInterval(() => void load(), pending ? PROPOSALS_EVERY_MS : PROPOSALS_IDLE_MS);
    return () => clearInterval(t);
  }, [sessionId, connected, load, pending]);
  useEffect(() => {
    const off = window.modbit.onEvent((raw) => {
      const e = raw as WireEventLike & { eventType?: string };
      if (e.taskId && PROPOSAL_EVENTS.has(e.eventType ?? "")) refresh();
    });
    return off;
  }, [refresh]);
  return { ...state, refresh };
}
