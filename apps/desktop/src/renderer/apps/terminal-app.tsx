import { useEffect, useRef, useState } from "react";
import { Badge, Button, Dialog } from "@modbit/ui";
import type { TerminalViewJson } from "../../preload/preload.ts";
import { elapsedLabel, isRunning, ownerNote, terminalLabel, type TerminalViewState } from "./terminal-controller.ts";
import { XtermView } from "./xterm-view.tsx";

export interface TerminalAppProps {
  sessionId: string;
  taskId: string;
  terminals: readonly TerminalViewJson[];
  /** The terminal chosen in this task's panel (remembered by the caller). */
  selectedId: string | null;
  onSelect: (terminalId: string) => void;
  onChanged: () => void;
  announce: (text: string) => void;
}

const stateLabel = (t: TerminalViewJson): string => {
  if (t.state === "RUNNING") return "running";
  if (t.state === "EXITED") return t.exitCode === null ? "exited" : `exited ${t.exitCode}`;
  if (t.state === "KILLED") return "stopped";
  return t.state.toLowerCase();
};

/**
 * The Terminal app (REQ-PX-048, AFW-I03): the task's terminals as a list (the
 * background-terminals tray: owner, state, running timer, Kill with a warning)
 * and the selected one as a live xterm view attached by cursor. Output replays
 * from the oldest byte the Core still holds, so a reload shows the same bytes.
 */
export function TerminalApp({ sessionId, taskId, terminals, selectedId, onSelect, onChanged, announce }: TerminalAppProps) {
  const selected = terminals.find((t) => t.terminalId === selectedId) ?? terminals[0] ?? null;
  const [view, setView] = useState<TerminalViewState | null>(null);
  const [notice, setNotice] = useState("");
  const [killing, setKilling] = useState<TerminalViewJson | null>(null);
  const [error, setError] = useState<string | null>(null);
  const leave = useRef<HTMLButtonElement>(null);
  // A different terminal starts with a clean status line.
  useEffect(() => {
    setView(null);
    setNotice("");
  }, [selected?.terminalId]);

  const kill = async (t: TerminalViewJson) => {
    setKilling(null);
    setError(null);
    try {
      const r = await window.modbit.terminalKill(sessionId, taskId, t.terminalId);
      announce(r.outcome === "KILLED" ? `Stopped ${terminalLabel(t)}.` : `${terminalLabel(t)} had already ended.`);
      onChanged();
    } catch (e) {
      setError((e as Error).message);
    }
  };

  if (terminals.length === 0) {
    return (
      <p className="meta" data-testid="app-terminal-empty">
        This task has no terminal. A terminal appears here when the agent starts a command that keeps running.
      </p>
    );
  }
  const exit = view?.exit;
  return (
    <div className="terminal-app" data-testid="app-terminal">
      <ul className="terminal-list" aria-label="Terminals of this task" data-testid="terminal-list">
        {terminals.map((t) => (
          <li key={t.terminalId} className="terminal-row" data-testid="terminal-row" data-terminal-id={t.terminalId} data-state={t.state} data-selected={t.terminalId === selected?.terminalId ? "true" : "false"}>
            <button type="button" className="terminal-pick" data-testid="terminal-pick" aria-pressed={t.terminalId === selected?.terminalId} onClick={() => onSelect(t.terminalId)}>
              <span className="terminal-title">{terminalLabel(t)}</span>
            </button>
            <Badge tone={t.state === "RUNNING" ? "ok" : "neutral"}>{t.owner === "agent" ? "agent" : t.owner}</Badge>
            <span className="meta" data-testid="terminal-state">
              {stateLabel(t)}
              {isRunning(t) ? ` · ${elapsedLabel(t.elapsedMs)}` : ""}
            </span>
            {isRunning(t) && (
              <Button size="sm" variant="danger" data-testid="terminal-kill" aria-label={`Stop ${terminalLabel(t)}`} onClick={() => setKilling(t)}>
                Stop
              </Button>
            )}
          </li>
        ))}
      </ul>
      {error && (
        <p className="meta" role="alert" data-testid="terminal-error">
          {error}
        </p>
      )}
      {selected && (
        <>
          <p className="meta terminal-note" data-testid="terminal-owner-note">
            {ownerNote(selected)} Shift+Escape leaves the terminal.
          </p>
          <div className="terminal-toolbar">
            <Button size="sm" ref={leave} data-testid="terminal-leave">
              Leave terminal
            </Button>
            <span className="meta" role="status" data-testid="terminal-status" data-lease={view?.leaseHeld ? "held" : "free"}>
              {view?.phase === "attaching" ? "Attaching…" : view?.phase === "lost" || view?.phase === "refused" ? `Disconnected: ${view.reason ?? ""}` : view?.leaseHeld ? "You hold the input; the agent's input to this terminal is refused." : "Click the terminal to type."}
            </span>
          </div>
          <XtermView key={selected.terminalId} sessionId={sessionId} terminal={selected} onLeave={() => leave.current?.focus()} onState={setView} onNotice={setNotice} />
          {exit && (
            <p className="meta" role="status" data-testid="terminal-exit">
              {exit.timedOut ? "Timed out" : exit.cancelled ? "Stopped" : exit.signal !== null ? `Ended by signal ${exit.signal}` : `Exited with code ${exit.exitCode ?? "unknown"}`}. The output above is its full replay.
            </p>
          )}
          {notice && (
            <p className="meta" role="status" data-testid="terminal-notice">
              {notice}
            </p>
          )}
        </>
      )}
      <Dialog open={killing !== null} title="Stop this terminal?" role="alertdialog" onClose={() => setKilling(null)} testId="terminal-kill-dialog">
        <p>{killing ? `"${terminalLabel(killing)}" is still running. Stopping it ends the process; the agent is told it was stopped.` : ""}</p>
        <div className="dialog-actions">
          <Button onClick={() => setKilling(null)} data-testid="terminal-kill-cancel">
            Keep running
          </Button>
          <Button variant="danger" onClick={() => killing && void kill(killing)} data-testid="terminal-kill-confirm">
            Stop it
          </Button>
        </div>
      </Dialog>
    </div>
  );
}
