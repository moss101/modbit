import { useCallback, useEffect, useRef, useState } from "react";
import type { ComputerCardView, ComputerPresenceInfo } from "../../shared/computer-types.ts";
import { callKey } from "./model.ts";

interface WireEventLike {
  taskId: string | null;
  aggregateType: string;
  eventType: string;
}

const FAST_MS = 1000;
const IDLE_MS = 5000;

/**
 * What the Core says about native control right now: who holds the machine,
 * the actuator and the permissions it reports. Read again when a computer
 * event passes, when the window regains focus (a permission granted in System
 * Settings is noticed on the way back, CUC-E05) and on a short beat; nothing
 * is kept but the last answer. While the Core is not connected nobody holds
 * the machine - the actuator stops with the Core's pipe - so the answer is
 * cleared, never left standing.
 */
export function useComputerPresence(connected: boolean): { info: ComputerPresenceInfo | null; refresh: () => Promise<void>; error: string } {
  const [info, setInfo] = useState<ComputerPresenceInfo | null>(null);
  const [error, setError] = useState("");
  const live = useRef(true);
  const inFlight = useRef(false);
  const again = useRef(false);

  const refresh = useCallback(async () => {
    if (inFlight.current) {
      again.current = true;
      return;
    }
    inFlight.current = true;
    try {
      const p = await window.modbit.computerPresence();
      if (live.current) {
        setInfo(p);
        setError(p === null ? "The Core is not answering." : "");
      }
    } catch (e) {
      if (live.current) setError((e as Error).message);
    } finally {
      inFlight.current = false;
      if (again.current) {
        again.current = false;
        void refresh();
      }
    }
  }, []);

  useEffect(() => {
    live.current = true;
    return () => {
      live.current = false;
    };
  }, []);

  useEffect(() => {
    if (!connected) {
      setInfo(null);
      return;
    }
    void refresh();
    let timer: ReturnType<typeof setTimeout> | null = null;
    let stopped = false;
    const beat = (): void => {
      if (stopped) return;
      timer = setTimeout(
        () => {
          if (document.visibilityState === "visible") void refresh();
          beat();
        },
        info?.configured ? FAST_MS : IDLE_MS,
      );
    };
    beat();
    const onFocus = (): void => void refresh();
    window.addEventListener("focus", onFocus);
    document.addEventListener("visibilitychange", onFocus);
    const off = window.modbit.onEvent((raw) => {
      const e = raw as WireEventLike;
      if (e.eventType?.startsWith("Computer")) void refresh();
    });
    return () => {
      stopped = true;
      if (timer) clearTimeout(timer);
      window.removeEventListener("focus", onFocus);
      document.removeEventListener("visibilitychange", onFocus);
      off();
    };
    // The beat's rate follows whether a helper is configured; the effect re-arms when that changes.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [connected, refresh, info?.configured]);

  return { info, refresh, error };
}

const HOLD_MS = 250;

/**
 * The task's computer tool calls as cards, joined by main from the transcript,
 * the Core's activity and the approvals. Fetched only when the conversation
 * has a computer call at all, and read again (held briefly, so a burst reads
 * once) when the task's log moves.
 */
export function useComputerCards(sessionId: string | null, taskId: string, connected: boolean, active: boolean, version: string): { cards: ReadonlyMap<string, ComputerCardView>; reload: () => void } {
  const [cards, setCards] = useState<ReadonlyMap<string, ComputerCardView>>(new Map());
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
      const r = await window.modbit.computerCards(sid, tid);
      if (key.current.taskId === tid) setCards(new Map(r.cards.map((c) => [callKey(c.toolCallId), c])));
    } catch {
      // Unreachable Core: the cards on screen stay as they were.
    } finally {
      inFlight.current = false;
      if (again.current) {
        again.current = false;
        void load();
      }
    }
  }, []);

  const reload = useCallback(() => {
    if (timer.current) return;
    timer.current = setTimeout(() => {
      timer.current = null;
      void load();
    }, HOLD_MS);
  }, [load]);

  useEffect(() => {
    setCards(new Map());
  }, [taskId]);

  useEffect(() => {
    if (!connected || !active) return;
    reload();
  }, [connected, active, version, reload]);

  useEffect(
    () => () => {
      if (timer.current) clearTimeout(timer.current);
      timer.current = null;
    },
    [],
  );

  return { cards, reload };
}
