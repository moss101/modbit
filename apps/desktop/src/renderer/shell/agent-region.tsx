import type { ReactNode } from "react";
import { Button, IconButton } from "@modbit/ui";
import { IconPlus, IconSidebar } from "./icons.tsx";

export interface AgentRegionProps {
  rail: boolean;
  /** The list was collapsed by the person (it can be expanded); false when the window is simply too narrow. */
  canExpand: boolean;
  taskCount: number;
  attentionCount: number;
  onNewTask: () => void;
  onExpand: () => void;
  /** The agent list (PX-046): the region's typed extension point. Absent, the region says only what the fleet model knows. */
  list?: ReactNode | undefined;
  /** The Fleet board is the view in the centre (it stays reachable as its own view). */
  fleetActive?: boolean | undefined;
  onShowFleet?: (() => void) | undefined;
  /** The worktree list (PX-068) is its own view in the centre. */
  worktreesActive?: boolean | undefined;
  onShowWorktrees?: (() => void) | undefined;
}

/**
 * The agent list region (AFW-A01, AFW-A05): the brand and the two ways in (a
 * new task, the Fleet board), then the list. Below a 448 px centre it is a
 * 40 px rail.
 */
export function AgentRegion({ rail, canExpand, taskCount, attentionCount, onNewTask, onExpand, list, fleetActive, onShowFleet, worktreesActive, onShowWorktrees }: AgentRegionProps) {
  if (rail) {
    return (
      <div className="agent-rail" data-testid="agent-rail">
        {canExpand && <IconButton label="Expand agent list" icon={<IconSidebar />} onClick={onExpand} data-testid="rail-expand" />}
        <IconButton label="New task" icon={<IconPlus />} onClick={onNewTask} data-testid="rail-new-task" />
      </div>
    );
  }
  return (
    <div className="agent-region">
      <div className="agent-region-head">
        <span className="brand" data-testid="brand">
          Modbit
        </span>
        {onShowFleet && (
          <Button size="sm" className="agent-fleet" aria-pressed={fleetActive === true} onClick={onShowFleet} data-testid="agents-fleet">
            Fleet board
          </Button>
        )}
        {onShowWorktrees && (
          <Button size="sm" className="agent-fleet" aria-pressed={worktreesActive === true} onClick={onShowWorktrees} data-testid="agents-worktrees">
            Worktrees
          </Button>
        )}
        <IconButton label="New task" icon={<IconPlus />} onClick={onNewTask} data-testid="agents-new-task" />
      </div>
      <p className="meta agent-region-meta" data-testid="agents-summary">
        {taskCount} {taskCount === 1 ? "task" : "tasks"}, {attentionCount} need{attentionCount === 1 ? "s" : ""} attention.
      </p>
      {list ?? (
        <p className="meta empty agent-region-meta" data-testid="agents-placeholder">
          The agent list is not connected in this view.
        </p>
      )}
    </div>
  );
}
