import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { Button } from "@modbit/ui";
import { HistoryLoader, historyText, type HistoryState } from "./terminal-history.ts";

export interface TerminalHistoryProps {
  taskId: string;
  terminalId: string;
  /** The cursor of the first byte the live view shows; output before it is older than the replay window. */
  origin: bigint | null;
  /** The Core sealed the process's output, so older ranges can be read. */
  available: boolean;
  /** The first cursor the Core retained: nothing is older, so a view that starts there has no earlier output. */
  retainedFrom: bigint;
  /** Counts the times the person scrolled the live view past its top. */
  reachedTop: number;
}

/**
 * Output that scrolled out of the replay window (REQ-PX-048, PX-043): above the
 * live view, read as OutputRef ranges from the Core when the person scrolls the
 * view past its top, and further back each time the pane itself is scrolled
 * past its top. The ranges arrive newest first and are shown in order, once.
 */
export function TerminalHistory({ taskId, terminalId, origin, available, retainedFrom, reachedTop }: TerminalHistoryProps) {
  const [open, setOpen] = useState(false);
  const [state, setState] = useState<HistoryState | null>(null);
  const [text, setText] = useState("");
  const loader = useRef<HistoryLoader | null>(null);
  const pane = useRef<HTMLDivElement>(null);
  const before = useRef<{ height: number; top: number } | null>(null);

  // A different terminal, or a view that starts elsewhere, is a different history.
  useEffect(() => {
    loader.current = null;
    setOpen(false);
    setState(null);
    setText("");
  }, [terminalId, origin]);

  const ensure = useCallback((): HistoryLoader | null => {
    if (origin === null || !available) return null;
    if (!loader.current) {
      const l: HistoryLoader = new HistoryLoader(
        async (from, to) => {
          const r = await window.modbit.terminalHistory(taskId, terminalId, from.toString(), to.toString());
          return { startCursor: BigInt(r.startCursor), from: BigInt(r.from), data: r.data };
        },
        origin,
        () => {
          if (loader.current !== l) return;
          setState(l.snapshot());
          setText(historyText(l.bytes()));
        },
      );
      loader.current = l;
    }
    return loader.current;
  }, [taskId, terminalId, origin, available]);

  const older = useCallback(() => {
    const l = ensure();
    if (!l) return;
    const el = pane.current;
    before.current = el ? { height: el.scrollHeight, top: el.scrollTop } : null;
    setOpen(true);
    void l.loadOlder();
  }, [ensure]);

  const seen = useRef(0);
  useEffect(() => {
    if (reachedTop === seen.current) return;
    seen.current = reachedTop;
    older();
  }, [reachedTop, older]);

  // A range arrives above what is shown: keep the line the person was reading where it was; the first one starts at the bottom.
  useLayoutEffect(() => {
    const el = pane.current;
    if (!el) return;
    const prev = before.current;
    before.current = null;
    el.scrollTop = prev && prev.height > 0 ? el.scrollHeight - prev.height + prev.top : el.scrollHeight;
  }, [text]);

  const exhausted = state?.exhausted ?? false;
  const atTop = () => (pane.current?.scrollTop ?? 1) <= 0;
  const lines = useMemo(() => (text === "" ? 0 : text.split("\n").length), [text]);
  if (origin === null || origin <= retainedFrom) return null;
  if (!available) return null;
  return (
    <section className="terminal-history" aria-label="Earlier output" data-testid="terminal-history" data-loaded-from={state?.loadedFrom.toString() ?? origin.toString()} data-origin={origin.toString()} data-pages={state?.pages ?? 0} data-bytes={state?.bytes ?? 0} data-lines={lines} data-exhausted={exhausted ? "true" : "false"} data-loading={state?.loading ? "true" : "false"}>
      <div className="terminal-toolbar">
        <span className="meta" role="status" data-testid="terminal-history-status">
          {state === null ? "Scroll past the top of the terminal to read earlier output." : exhausted ? `Showing all the output the Core retained before the view (${state.bytes} bytes).` : `Showing ${state.bytes} bytes before the view; scroll up for more.`}
        </span>
        <Button size="sm" data-testid="terminal-history-load" disabled={exhausted || state?.loading === true} onClick={older}>
          Load earlier output
        </Button>
      </div>
      {state?.error && (
        <p className="meta" role="alert" data-testid="terminal-history-error">
          {state.error}
        </p>
      )}
      {open && (
        <div
          ref={pane}
          className="terminal-history-pane"
          role="region"
          aria-label="Earlier output, oldest first"
          tabIndex={0}
          data-testid="terminal-history-pane"
          onScroll={() => atTop() && !exhausted && older()}
          onWheel={(e) => e.deltaY < 0 && atTop() && !exhausted && older()}
          onKeyDown={(e) => (e.key === "ArrowUp" || e.key === "PageUp" || e.key === "Home") && atTop() && !exhausted && older()}
        >
          <pre data-testid="terminal-history-text">{text}</pre>
        </div>
      )}
    </section>
  );
}
