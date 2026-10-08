/**
 * The automations surface's data: the Core's list (definitions, states,
 * attention, the global switch) and run history, read again after every
 * command and, while the surface is open, on a steady beat (the Core's
 * scheduler and runs change state without the desktop session seeing an
 * event). Nothing is derived here; every field is the Core's.
 */
import { useCallback, useEffect, useRef, useState } from "react";
import type { AutoList, AutoRun } from "../../shared/automation-types.ts";
import type { AutomationCalls } from "./calls.ts";

const BEAT_MS = 1500;

export interface AutomationsData {
  list: AutoList | null;
  runs: AutoRun[];
  loaded: boolean;
  error: string | null;
  refresh: () => Promise<void>;
}

export function useAutomations(calls: AutomationCalls, connected: boolean): AutomationsData {
  const [list, setList] = useState<AutoList | null>(null);
  const [runs, setRuns] = useState<AutoRun[]>([]);
  const [loaded, setLoaded] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const alive = useRef(true);
  const inFlight = useRef<Promise<void> | null>(null);
  useEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
    };
  }, []);

  const refresh = useCallback((): Promise<void> => {
    if (inFlight.current) return inFlight.current;
    const p = (async () => {
      try {
        const [l, r] = await Promise.all([calls.list(), calls.runs({ limit: 300 })]);
        if (!alive.current) return;
        setList(l);
        setRuns(r);
        setLoaded(true);
        setError(null);
      } catch (e) {
        if (alive.current) setError((e as Error).message.replace(/^Error invoking remote method '[^']*': (Error: )?/, ""));
      } finally {
        inFlight.current = null;
      }
    })();
    inFlight.current = p;
    return p;
    // `calls` is fixed for the life of the surface.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    if (!connected) return;
    void refresh();
    const t = setInterval(() => void refresh(), BEAT_MS);
    return () => clearInterval(t);
  }, [connected, refresh]);

  return { list, runs, loaded, error, refresh };
}
