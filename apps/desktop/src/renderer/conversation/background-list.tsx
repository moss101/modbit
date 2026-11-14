/**
 * The task's background work as a list (REQ-PX-201, REQ-PX-202; DR-PX-2026-11-11-021 MWS-09, MWS-10): each running child task
 * (its goal opens it, its state in the Core's word, Stop) and each running terminal (its title opens it in the Terminal pane, how
 * long it has run, Stop), then how many have finished, which opens their list. One list, drawn in the details rail's Background
 * tasks section and in the Background tasks pane. Stop on a child asks with the confirmation Cancel task asks and then the
 * Core cancels it; Stop on a terminal asks once more in place and then the Core kills it. Each value names its read.
 */
import { useEffect, useState } from "react";
import { Button, Icon } from "@modbit/ui";
import type { TerminalViewJson } from "../../preload/preload.ts";
import type { TaskCard } from "../model.ts";
import { elapsedLabel } from "../composer/model.ts";
import { backgroundOf, type RailSource } from "./rail-model.ts";

/** Marks an element as a value and names the preload read (or reads) it is drawn from. */
const src = (...s: RailSource[]) => ({ "data-rail-value": "", "data-src": s.join(" ") });

/** A running terminal's row: what it runs, for how long, and Stop, which asks once more before the Core kills it. */
function TerminalRow({ t, now, sessionId, taskId, onOpen, onChanged }: { t: TerminalViewJson; now: number; sessionId: string | null; taskId: string; onOpen?: ((terminalId: string) => void) | undefined; onChanged?: (() => void) | undefined }) {
  const [armed, setArmed] = useState(false);
  const [busy, setBusy] = useState(false);
  const [note, setNote] = useState("");
  const title = t.title || "terminal";
  const kill = async () => {
    if (!sessionId) return;
    setBusy(true);
    try {
      await window.modbit.terminalKill(sessionId, taskId, t.terminalId);
      setNote("");
      onChanged?.();
    } catch (e) {
      setNote((e as Error).message.replace(/^Error invoking remote method '[^']*': (Error: )?/, ""));
    } finally {
      setBusy(false);
      setArmed(false);
    }
  };
  return (
    <li className="rail-row" data-testid="rail-terminal" data-terminal-id={t.terminalId} data-state={t.state}>
      <span className="rail-ico">
        <Icon name="terminal" size={16} />
      </span>
      {onOpen ? (
        <button type="button" className="rail-x rail-link rail-mono" onClick={() => onOpen(t.terminalId)} title={`Open ${title} in the Terminal pane`} data-testid="rail-terminal-title" {...src("terminalList")}>
          {title}
        </button>
      ) : (
        <span className="rail-x rail-mono" data-testid="rail-terminal-title" {...src("terminalList")}>
          {title}
        </span>
      )}
      {note ? (
        <span className="rail-aside" role="alert">
          {note}
        </span>
      ) : (
        !armed && (
          <span className="rail-aside" data-testid="rail-terminal-elapsed" {...src("terminalList")}>
            {elapsedLabel(t.startedAtMs, now)}
          </span>
        )
      )}
      {armed ? (
        <span
          className="rail-armed"
          onBlur={(e) => {
            if (!e.currentTarget.contains(e.relatedTarget as Node | null)) setArmed(false);
          }}
        >
          <Button size="sm" variant="danger" autoFocus disabled={busy || !sessionId} onClick={() => void kill()} data-testid="rail-terminal-stop-confirm">
            Stop it
          </Button>
          <Button size="sm" variant="ghost" onClick={() => setArmed(false)} data-testid="rail-terminal-stop-cancel">
            Keep
          </Button>
        </span>
      ) : (
        <button type="button" className="rail-stop" aria-label={`Stop ${title}`} title="Stop" onClick={() => setArmed(true)} data-testid="rail-terminal-stop">
          <span aria-hidden="true" />
        </button>
      )}
    </li>
  );
}

export interface BackgroundListProps {
  taskId: string;
  sessionId: string | null;
  subtasks: readonly TaskCard[];
  terminals: readonly TerminalViewJson[];
  onOpenTask: (taskId: string) => void;
  onOpenTerminal?: ((terminalId: string) => void) | undefined;
  /** Asks to cancel a child task (the same confirmation Cancel task asks). */
  onStopChild: (taskId: string) => void;
  onTerminalsChanged?: (() => void) | undefined;
}

export function BackgroundList({ taskId, sessionId, subtasks, terminals, onOpenTask, onOpenTerminal, onStopChild, onTerminalsChanged }: BackgroundListProps) {
  const [finishedOpen, setFinishedOpen] = useState(false);
  const bg = backgroundOf(subtasks, terminals);
  const [now, setNow] = useState(() => Date.now());
  const ticking = bg.liveTerminals.length > 0;
  useEffect(() => {
    if (!ticking) return;
    setNow(Date.now());
    const t = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(t);
  }, [ticking]);
  return (
    <div data-testid="task-children">
    <ul className="rail-list" aria-label="Running child tasks and terminals">
      {bg.liveChildren.map((c) => (
        <li key={c.taskId} className="rail-row" data-testid="child-card" data-task-id={c.taskId} data-state={c.state}>
          <span className="rail-ico">
            <Icon name="list-tree" size={16} />
          </span>
          <button type="button" className="rail-x rail-link" onClick={() => onOpenTask(c.taskId)} title={c.goalText}>
            <span data-testid="child-goal" {...src("sessionSnapshot")}>
              {c.goalText}
            </span>
          </button>
          <span className="rail-aside" data-testid="child-state" {...src("sessionSnapshot")}>
            {c.state}
          </span>
          <button type="button" className="rail-stop" aria-label={`Stop ${c.goalText}`} title="Stop" onClick={() => onStopChild(c.taskId)} data-testid="child-stop">
            <span aria-hidden="true" />
          </button>
        </li>
      ))}
      {bg.liveTerminals.map((t) => (
        <TerminalRow key={t.terminalId} t={t} now={now} sessionId={sessionId} taskId={taskId} onOpen={onOpenTerminal} onChanged={onTerminalsChanged} />
      ))}
    </ul>
    {bg.finished > 0 && (
      <>
        <button type="button" className="rail-row rail-rowbtn" aria-expanded={finishedOpen} onClick={() => setFinishedOpen((o) => !o)} data-testid="rail-finished" data-count={bg.finished}>
          <span className="rail-ico">
            <Icon name="check" size={16} />
          </span>
          <span className="rail-x rail-dim" data-testid="rail-finished-count" {...src("sessionSnapshot", "terminalList")}>
            {bg.finished} finished
          </span>
          <span className="rail-aside">{finishedOpen ? "hide" : "show"}</span>
        </button>
        {finishedOpen && (
          <ul className="rail-list" aria-label="Finished child tasks and terminals" data-testid="rail-finished-list">
            {bg.endedChildren.map((c) => (
              <li key={c.taskId} className="rail-row rail-sub" data-testid="child-card" data-task-id={c.taskId} data-state={c.state}>
                <button type="button" className="rail-x rail-link" onClick={() => onOpenTask(c.taskId)} title={c.goalText}>
                  <span data-testid="child-goal" {...src("sessionSnapshot")}>
                    {c.goalText}
                  </span>
                </button>
                <span className="rail-aside" data-testid="child-state" {...src("sessionSnapshot")}>
                  {c.state}
                </span>
              </li>
            ))}
            {bg.endedTerminals.map((t) => (
              <li key={t.terminalId} className="rail-row rail-sub" data-testid="rail-terminal-ended" data-terminal-id={t.terminalId} data-state={t.state}>
                {onOpenTerminal ? (
                  <button type="button" className="rail-x rail-link rail-mono" onClick={() => onOpenTerminal?.(t.terminalId)} {...src("terminalList")}>
                    {t.title || "terminal"}
                  </button>
                ) : (
                  <span className="rail-x rail-mono" {...src("terminalList")}>
                    {t.title || "terminal"}
                  </span>
                )}
                <span className="rail-aside" {...src("terminalList")}>
                  {t.exitCode === null ? t.state.toLowerCase() : `exit ${t.exitCode}`}
                </span>
              </li>
            ))}
          </ul>
        )}
      </>
    )}
    </div>
  );
}
