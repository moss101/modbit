/**
 * A pane in a window of its own (REQ-PX-203; DR-PX-2026-11-11-021 MWS-10). The same renderer, under the same preload and content
 * security policy as the app's window, mounts this instead of the shell when the main process opened the window for a pane. It
 * asks main which pane of which task it shows (the window cannot choose), and shows that pane's app as the dock does, over the
 * same typed reads. It decides no approval: main refuses every such channel to this window, and the Changes pane's card sends the
 * person to the approval card in the app's window. What the pane shows (its terminal, the file chosen, the scroll) is told to
 * main as it changes, so the dock shows the same when the window closes.
 */
import { useCallback, useEffect, useRef, useState } from "react";
import type { PaneContextDto, TerminalViewJson } from "../preload/preload.ts";
import { ChangesApp } from "./apps/changes-app.tsx";
import { followDelivered } from "./apps/delivered.ts";
import { EvidenceApp } from "./apps/evidence-app.tsx";
import { FilesApp } from "./apps/files-app.tsx";
import { TerminalApp } from "./apps/terminal-app.tsx";
import { PaneWindowFrame } from "./shell/pane.tsx";
import { onPaneViewChange, paneView } from "./shell/pane-view.ts";
import { loadUiPrefs, UI_PREFS_KEY } from "./shell/prefs.ts";
import { applyDensity, applyTheme } from "./shell/shell.tsx";

const POLL_MS = 2500;
const THROTTLE_MS = 400;

/** The theme and density the person chose, as the app's window applies them, followed when they change there. */
function useAppearance(): void {
  useEffect(() => {
    const apply = () => {
      const p = loadUiPrefs();
      applyTheme(p.theme);
      applyDensity(p.density);
      document.documentElement.style.zoom = p.zoom === 1 ? "" : String(p.zoom);
    };
    apply();
    const onStorage = (e: StorageEvent) => {
      if (e.key === null || e.key === UI_PREFS_KEY) apply();
    };
    window.addEventListener("storage", onStorage);
    return () => window.removeEventListener("storage", onStorage);
  }, []);
}

/** The task's last event offset this window heard (the Core pushes the task's events here), at most once per beat. */
function useTaskTick(taskId: string | null): string {
  const [tick, setTick] = useState("");
  const pending = useRef<string | null>(null);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  useEffect(() => {
    if (!taskId) return;
    const off = window.modbit.onEvent((raw) => {
      const e = raw as { taskId?: string | null; offset?: string };
      if (e.taskId !== taskId) return;
      pending.current = String(e.offset ?? "");
      if (timer.current) return;
      timer.current = setTimeout(() => {
        timer.current = null;
        if (pending.current !== null) setTick(pending.current);
      }, 1200);
    });
    return () => {
      off();
      if (timer.current) clearTimeout(timer.current);
    };
  }, [taskId]);
  return tick;
}

/** The task's terminals, read from the Core when the window opens, after the task's events and on a slow poll while one runs. */
function useTaskTerminals(taskId: string | null, tick: string): { terminals: TerminalViewJson[]; refresh: () => void } {
  const [terminals, setTerminals] = useState<TerminalViewJson[]>([]);
  const last = useRef(0);
  const read = useCallback(() => {
    if (!taskId) return;
    last.current = Date.now();
    void window.modbit
      .terminalList(taskId)
      .then((l) => setTerminals(l.terminals.filter((t) => t.taskId === taskId)))
      .catch(() => {});
  }, [taskId]);
  const refresh = useCallback(() => {
    setTimeout(read, Math.max(0, THROTTLE_MS - (Date.now() - last.current)));
  }, [read]);
  useEffect(() => read(), [read, tick]);
  const running = terminals.some((t) => t.state === "RUNNING");
  useEffect(() => {
    if (!running) return;
    const t = setInterval(read, POLL_MS);
    return () => clearInterval(t);
  }, [running, read]);
  return { terminals, refresh };
}

export function PaneWindow() {
  useAppearance();
  const [ctx, setCtx] = useState<PaneContextDto | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [terminalId, setTerminalId] = useState<string | null>(null);
  const [notice, setNotice] = useState("");
  useEffect(() => {
    void window.modbit
      .paneContext()
      .then((c) => {
        // What the dock was showing is what this window starts with.
        paneView.put(c.taskId, c.kind, c.state);
        setTerminalId(c.state.terminalId ?? null);
        setCtx(c);
        document.title = c.title;
      })
      .catch((e: Error) => setError(e.message));
  }, []);
  // The approval events reach the Changes pane through the same buffer as in the app's window.
  useEffect(() => followDelivered(), []);
  // What the pane shows is told to main as it changes (main keeps the last, and hands it to the dock when the window closes).
  useEffect(() => {
    if (!ctx) return;
    let timer: ReturnType<typeof setTimeout> | null = null;
    onPaneViewChange((taskId, kind) => {
      if (taskId !== ctx.taskId || kind !== ctx.kind) return;
      if (timer) clearTimeout(timer);
      timer = setTimeout(() => void window.modbit.reportPaneState(paneView.get(ctx.taskId, ctx.kind)).catch(() => {}), 120);
    });
    return () => {
      onPaneViewChange(null);
      if (timer) clearTimeout(timer);
    };
  }, [ctx]);
  const tick = useTaskTick(ctx?.taskId ?? null);
  const { terminals, refresh } = useTaskTerminals(ctx?.kind === "terminal" ? ctx.taskId : null, tick);

  if (error) {
    return (
      <p className="pane-window-empty" role="alert" data-testid="pane-window-error">
        This window could not find its pane: {error}
      </p>
    );
  }
  if (!ctx) {
    return (
      <p className="pane-window-empty" role="status" data-testid="pane-window-loading">
        Opening the pane…
      </p>
    );
  }
  const back = () => void window.modbit.returnPane(paneView.get(ctx.taskId, ctx.kind));
  const pick = (id: string) => {
    setTerminalId(id);
    paneView.set(ctx.taskId, ctx.kind, { terminalId: id });
    void window.modbit.reportPaneState(paneView.get(ctx.taskId, ctx.kind)).catch(() => {});
  };
  return (
    <PaneWindowFrame kind={ctx.kind} taskId={ctx.taskId} taskTitle={ctx.title.replace(/^[^:]+: /, "")} onReturn={back}>
      {ctx.kind === "terminal" && <TerminalApp sessionId={ctx.sessionId} taskId={ctx.taskId} terminals={terminals} selectedId={terminalId} onSelect={pick} onChanged={refresh} announce={setNotice} />}
      {ctx.kind === "changes" && (
        <ChangesApp
          sessionId={ctx.sessionId}
          taskId={ctx.taskId}
          taskState=""
          refreshKey={tick}
          onOpenReview={() => void window.modbit.paneShow({ what: "review" })}
          onOpenCall={(toolCallId) => void window.modbit.paneShow({ what: "call", toolCallId })}
          approvalElsewhere={{ onShowApproval: () => void window.modbit.paneShow({ what: "approval" }) }}
        />
      )}
      {ctx.kind === "files" && <FilesApp taskId={ctx.taskId} refreshKey={tick} />}
      {ctx.kind === "evidence" && <EvidenceApp taskId={ctx.taskId} refreshKey={tick} />}
      <p className="mb-sr-only" role="status" aria-live="polite" data-testid="pane-window-notice">
        {notice}
      </p>
    </PaneWindowFrame>
  );
}
