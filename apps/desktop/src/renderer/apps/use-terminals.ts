import { useCallback, useEffect, useRef, useState } from "react";
import type { TerminalViewJson } from "../../preload/preload.ts";

/** Groups the Core's terminal list by owning task, running ones first (the Core's own order). */
export function groupByTask(terminals: readonly TerminalViewJson[]): Map<string, TerminalViewJson[]> {
  const out = new Map<string, TerminalViewJson[]>();
  for (const t of terminals) {
    if (!t.taskId) continue;
    const list = out.get(t.taskId) ?? [];
    list.push(t);
    out.set(t.taskId, list);
  }
  return out;
}

const THROTTLE_MS = 400;
const POLL_MS = 2500;

/**
 * The Core's terminals by task, for the apps panel: read when the app starts,
 * after any task event (throttled), and on a slow poll while a task runs (the
 * running timer and a terminal that ended need no event). `refresh` forces a
 * read. The list is the Core's; this keeps no copy that outlives a read.
 */
export function useTerminals(ready: boolean, anyRunning: boolean, eventTick: string): { byTask: Map<string, TerminalViewJson[]>; refresh: () => void } {
  const [byTask, setByTask] = useState<Map<string, TerminalViewJson[]>>(new Map());
  const last = useRef(0);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const read = useCallback(() => {
    last.current = Date.now();
    void window.modbit
      .terminalList()
      .then((l) => setByTask(groupByTask(l.terminals)))
      .catch(() => {});
  }, []);
  const refresh = useCallback(() => {
    if (timer.current) clearTimeout(timer.current);
    const wait = Math.max(0, THROTTLE_MS - (Date.now() - last.current));
    timer.current = setTimeout(read, wait);
  }, [read]);
  useEffect(() => {
    if (ready) refresh();
  }, [ready, eventTick, refresh]);
  useEffect(() => {
    if (!ready || !anyRunning) return;
    const t = setInterval(read, POLL_MS);
    return () => clearInterval(t);
  }, [ready, anyRunning, read]);
  useEffect(
    () => () => {
      if (timer.current) clearTimeout(timer.current);
    },
    [],
  );
  return { byTask, refresh };
}
