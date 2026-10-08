import { IconButton } from "@modbit/ui";
import { IconPlus, IconSidebar } from "./icons.tsx";

export interface AgentRegionProps {
  rail: boolean;
  /** The list was collapsed by the person (it can be expanded); false when the window is simply too narrow. */
  canExpand: boolean;
  taskCount: number;
  attentionCount: number;
  onNewTask: () => void;
  onExpand: () => void;
}

/**
 * The agent list region (AFW-A01, AFW-A05). The list itself (status classes,
 * unread, pins, filters, grouping, search) is PX-046 and is not built here:
 * this region is the typed extension point for it and says so, and states only
 * counts the Core's fleet model already has. Below a 448 px centre it is a
 * 40 px rail.
 */
export function AgentRegion({ rail, canExpand, taskCount, attentionCount, onNewTask, onExpand }: AgentRegionProps) {
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
        <IconButton label="New task" icon={<IconPlus />} onClick={onNewTask} data-testid="agents-new-task" />
      </div>
      <p className="meta" data-testid="agents-summary">
        {taskCount} {taskCount === 1 ? "task" : "tasks"}, {attentionCount} need{attentionCount === 1 ? "s" : ""} attention.
      </p>
      <p className="meta empty" data-testid="agents-placeholder">
        The agent list (status classes, unread, pins, filters, grouping) arrives with PX-046. Until then the Fleet board in the centre shows every task.
      </p>
    </div>
  );
}
