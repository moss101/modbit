/**
 * The Background tasks pane (REQ-PX-202; DR-PX-2026-11-11-021 MWS-10): the task's child tasks and terminals, each running one
 * with Stop, and the finished ones behind their count. It is the list the details rail's section draws (one component), from
 * the same facts: the children the session's snapshot and events name, and the Core's terminal list. Stop on a child asks with
 * the confirmation Cancel task asks and the Core then cancels it; Stop on a terminal asks once more in place and the Core kills it.
 */
import type { TerminalViewJson } from "../../preload/preload.ts";
import type { TaskCard } from "../model.ts";
import { BackgroundList } from "../conversation/background-list.tsx";
import { backgroundOf } from "../conversation/rail-model.ts";

export interface BackgroundAppProps {
  taskId: string;
  sessionId: string | null;
  subtasks: readonly TaskCard[];
  terminals: readonly TerminalViewJson[];
  onOpenTask: (taskId: string) => void;
  onOpenTerminal: (terminalId: string) => void;
  onStopChild: (taskId: string) => void;
  onTerminalsChanged: () => void;
}

export function BackgroundApp(p: BackgroundAppProps) {
  const bg = backgroundOf(p.subtasks, p.terminals);
  const running = bg.liveChildren.length + bg.liveTerminals.length;
  if (p.subtasks.length === 0 && p.terminals.length === 0) {
    return (
      <p className="meta" data-testid="app-background-empty">
        This task has no child task and no terminal. Work the agent hands to a child task, and commands it leaves running, are listed here with Stop.
      </p>
    );
  }
  return (
    <div className="background-app" data-testid="app-background" data-running={running} data-finished={bg.finished}>
      <p className="meta background-count" data-testid="background-running">
        {running === 0 ? "Nothing is running" : `${running} running`}
      </p>
      <BackgroundList taskId={p.taskId} sessionId={p.sessionId} subtasks={p.subtasks} terminals={p.terminals} onOpenTask={p.onOpenTask} onOpenTerminal={p.onOpenTerminal} onStopChild={p.onStopChild} onTerminalsChanged={p.onTerminalsChanged} />
    </div>
  );
}
