import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import type { TerminalViewJson } from "../../preload/preload.ts";
import type { TaskCard } from "../model.ts";
import { ActivityApp } from "../apps/activity-app.tsx";
import { BackgroundApp } from "../apps/background-app.tsx";
import { BrowserApp } from "../apps/browser-app.tsx";
import { ChangesApp } from "../apps/changes-app.tsx";
import { EvidenceApp } from "../apps/evidence-app.tsx";
import { FilesApp } from "../apps/files-app.tsx";
import { PullRequestApp } from "../apps/pull-request-app.tsx";
import { TerminalApp } from "../apps/terminal-app.tsx";
import { APP_LABEL } from "./artifacts.ts";
import { closePane, dockColumns, leftColumnPx, minimizePane, PANE_MIN, resizeRows, rowHeights, splitForLeft, weightOf } from "./dock.ts";
import { PaneFrame } from "./pane.tsx";
import { PaneToggles, useToggleFit, type PaneToggle } from "./top-bar.tsx";
import { wantedToggles } from "./title-fit.ts";
import { DOCK_SPLIT_DEFAULT, type AppKind, type PanelState } from "./prefs.ts";
import { Splitter } from "./splitter.tsx";

export interface AppsPanelProps {
  sessionId: string;
  taskId: string;
  taskTitle: string;
  taskState: string;
  /** The task's dock: its open panes, the minimized ones and their sizes (remembered per task by the caller). */
  dock: PanelState;
  /** The panes in the grid, in order: the open ones the task can show now that are not minimized. */
  tiled: readonly AppKind[];
  /** Applies a change to the task's dock (the owner persists it). */
  onDock: (update: (s: PanelState) => PanelState) => void;
  /** Changes whenever the task moves (its last event offset): the apps read the Core again. */
  refreshKey: string;
  terminals: readonly TerminalViewJson[];
  selectedTerminalId: string | null;
  onSelectTerminal: (terminalId: string) => void;
  onTerminalsChanged: () => void;
  announce: (text: string) => void;
  onOpenReview: () => void;
  /** PX-104: open a tool call in the task's conversation (the Changes chip). */
  onOpenCall: (toolCallId: string) => void;
  onOpenBrowser: () => void;
  /** The task's child tasks, as the session's snapshot and events name them (the Background tasks pane). */
  subtasks: readonly TaskCard[];
  onOpenTask: (taskId: string) => void;
  /** Asks to cancel a child task (the same confirmation Cancel task asks). */
  onStopChild: (taskId: string) => void;
  /** Shows a terminal in the Terminal pane. */
  onOpenTerminal: (terminalId: string) => void;
  /**
   * The task's pane toggles, where the title bar does not have them: the centre shows something other than this task (home after
   * a reload, another page), and the dock then says whose panes these are and is where one is opened from. Absent otherwise.
   */
  ownToggles?: { panes: readonly PaneToggle[]; onPane: (kind: AppKind) => void } | undefined;
}

/**
 * The pane dock (REQ-PX-202; DR-PX-2026-11-11-021 MWS-10; AFW-A01, AFW-I01, REQ-PX-048 for the apps): any of the task's panes
 * open together, tiled in a grid of at most two columns, each with a head the height of the top bar (its name, its own
 * actions, Minimize, Close) and none under 280 by 100 px. The separators between them are dragged or moved with the arrow keys,
 * and what they set is remembered for the task with the set of open panes. Every pane is a view over typed Core reads through
 * the preload bridge; none holds task state, and a pane with nothing to show says so.
 */
export function AppsPanel(p: AppsPanelProps) {
  const root = useRef<HTMLDivElement>(null);
  const paneEls = useRef(new Map<AppKind, HTMLElement>());
  const [size, setSize] = useState<{ width: number; height: number } | null>(null);
  useEffect(() => {
    const el = root.current;
    if (!el) return;
    const measure = () => {
      const r = el.getBoundingClientRect();
      setSize((s) => (s && s.width === Math.round(r.width) && s.height === Math.round(r.height) ? s : { width: Math.round(r.width), height: Math.round(r.height) }));
    };
    measure();
    const ro = new ResizeObserver(measure);
    ro.observe(el);
    return () => ro.disconnect();
  }, []);
  const { dock, onDock } = p;
  const columns = size ? dockColumns(p.tiled, size.width) : [];
  const two = columns.length === 2;
  const leftPx = two && size ? leftColumnPx(dock.split, size.width) : 0;
  const setLeft = useCallback((px: number) => size && onDock((s) => splitForLeft(s, px, size.width)), [onDock, size]);

  const body = (kind: AppKind): ReactNode => {
    switch (kind) {
      case "changes":
        return <ChangesApp sessionId={p.sessionId} taskId={p.taskId} taskState={p.taskState} refreshKey={p.refreshKey} onOpenReview={p.onOpenReview} onOpenCall={p.onOpenCall} />;
      case "terminal":
        return <TerminalApp sessionId={p.sessionId} taskId={p.taskId} terminals={p.terminals} selectedId={p.selectedTerminalId} onSelect={p.onSelectTerminal} onChanged={p.onTerminalsChanged} announce={p.announce} />;
      case "browser":
        return <BrowserApp taskId={p.taskId} onOpenBrowser={p.onOpenBrowser} />;
      case "files":
        return <FilesApp taskId={p.taskId} refreshKey={p.refreshKey} />;
      case "evidence":
        return <EvidenceApp taskId={p.taskId} refreshKey={p.refreshKey} />;
      case "activity":
        return <ActivityApp taskId={p.taskId} />;
      case "background":
        return <BackgroundApp taskId={p.taskId} sessionId={p.sessionId || null} subtasks={p.subtasks} terminals={p.terminals} onOpenTask={p.onOpenTask} onOpenTerminal={p.onOpenTerminal} onStopChild={p.onStopChild} onTerminalsChanged={p.onTerminalsChanged} />;
      case "pull-request":
        return <PullRequestApp sessionId={p.sessionId} taskId={p.taskId} refreshKey={p.refreshKey} />;
    }
  };

  return (
    <div className="apps-panel dock" role="group" aria-label="Panes" data-testid="apps-panel" data-task-id={p.taskId} data-columns={columns.length} data-panes={p.tiled.join(" ")}>
      {p.ownToggles && p.ownToggles.panes.length > 0 && <DockBar title={p.taskTitle} panes={p.ownToggles.panes} onPane={p.ownToggles.onPane} />}
      <div ref={root} className="dock-grid" data-testid="dock-grid">
      {size &&
        columns.map((column, ci) => {
          const heights = rowHeights(dock, column, size.height);
          return (
            <div key={column.join("+")} className="dock-col" data-testid="dock-col" data-index={ci} style={two && ci === 0 ? { width: leftPx, flex: "none" } : undefined}>
              {column.map((kind, ri) => {
                const above = ri > 0 ? column[ri - 1]! : null;
                const both = above ? heights[ri - 1]! + heights[ri]! : 0;
                const setAbove = (px: number) => above && onDock((s) => resizeRows(s, above, kind, px, both));
                return (
                  <PaneFrame
                    key={kind}
                    kind={kind}
                    height={heights[ri]!}
                    paneRef={(el) => {
                      if (el) paneEls.current.set(kind, el);
                      else paneEls.current.delete(kind);
                    }}
                    onMinimize={() => {
                      onDock((s) => minimizePane(s, kind));
                      p.announce(`${APP_LABEL[kind]} is minimized. Its button in the title bar brings it back.`);
                    }}
                    onClose={() => onDock((s) => closePane(s, kind))}
                    before={
                      above && both >= 2 * PANE_MIN.height ? (
                        <Splitter
                          label={`Resize ${APP_LABEL[above]} and ${APP_LABEL[kind]}`}
                          testId={`splitter-row-${kind}`}
                          edge="top"
                          axis="horizontal"
                          value={heights[ri - 1]!}
                          min={PANE_MIN.height}
                          max={both - PANE_MIN.height}
                          onDragTo={(y) => setAbove(y - (paneEls.current.get(above)?.getBoundingClientRect().top ?? 0))}
                          onNudge={(d) => setAbove(heights[ri - 1]! + d)}
                          onJump={(to) => setAbove(to === "min" ? PANE_MIN.height : both - PANE_MIN.height)}
                          onReset={() =>
                            onDock((s) => {
                              const half = (weightOf(s, above) + weightOf(s, kind)) / 2;
                              return { ...s, weights: { ...s.weights, [above]: half, [kind]: half } };
                            })
                          }
                        />
                      ) : undefined
                    }
                  >
                    {body(kind)}
                  </PaneFrame>
                );
              })}
              {two && ci === 0 && (
                <Splitter
                  label="Resize the pane columns"
                  testId="splitter-columns"
                  edge="right"
                  value={leftPx}
                  min={PANE_MIN.width}
                  max={size.width - PANE_MIN.width}
                  onDragTo={(x) => setLeft(x - (root.current?.getBoundingClientRect().left ?? 0))}
                  onNudge={(d) => setLeft(leftPx + d)}
                  onJump={(to) => setLeft(to === "min" ? PANE_MIN.width : size.width - PANE_MIN.width)}
                  onReset={() => onDock((s) => ({ ...s, split: DOCK_SPLIT_DEFAULT }))}
                />
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}

/** The dock's own bar, where the title bar does not hold the dock's task's toggles: whose panes these are, the toggles that fit, the Panes menu. */
function DockBar({ title, panes, onPane }: { title: string; panes: readonly PaneToggle[]; onPane: (kind: AppKind) => void }) {
  const [bar, setBar] = useState<HTMLElement | null>(null);
  const fit = useToggleFit(bar, wantedToggles(panes), ".dock-bar-task", 120);
  return (
    <div ref={setBar} className="dock-bar" data-testid="dock-bar">
      <span className="dock-bar-task" title={title} data-testid="dock-bar-task">
        {title}
      </span>
      <PaneToggles panes={panes} onPane={onPane} fit={fit} size="sm" />
    </div>
  );
}
