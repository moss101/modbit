import type { ReactNode } from "react";
import { Icon, IconButton, type IconName } from "@modbit/ui";
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
  /** The command palette (search tasks, commands and settings). */
  onSearch?: (() => void) | undefined;
  /** The Fleet board is the view in the centre (it stays reachable as its own view). */
  fleetActive?: boolean | undefined;
  onShowFleet?: (() => void) | undefined;
  /** The worktree list (PX-068) is its own view in the centre. */
  worktreesActive?: boolean | undefined;
  onShowWorktrees?: (() => void) | undefined;
  /** REQ-PX-086: the Automations surface. */
  automationsOpen?: boolean | undefined;
  automationsDisabled?: boolean | undefined;
  onToggleAutomations?: (() => void) | undefined;
  /** REQ-PX-106: the Customize view over skills, hooks and extensions. */
  customizeOpen?: boolean | undefined;
  customizeDisabled?: boolean | undefined;
  onToggleCustomize?: (() => void) | undefined;
  dashboardOpen?: boolean | undefined;
  dashboardDisabled?: boolean | undefined;
  onToggleDashboard?: (() => void) | undefined;
}

interface NavEntry {
  testId: string;
  icon: IconName;
  label: string;
  onClick: () => void;
  pressed?: boolean | undefined;
  disabled?: boolean | undefined;
  shortcut?: string | undefined;
}

/** One 30 px navigation row (AWL-06): an icon and a label, the same command and test id it had in the head or the top bar. */
function NavRow({ testId, icon, label, onClick, pressed, disabled, shortcut }: NavEntry) {
  return (
    <li>
      <button type="button" className="agent-nav-row" data-testid={testId} onClick={onClick} disabled={disabled ?? false} {...(pressed === undefined ? {} : { "aria-pressed": pressed })}>
        <Icon name={icon} size={14} />
        <span className="agent-nav-label">{label}</span>
        {shortcut && (
          <span className="agent-nav-key" aria-hidden="true">
            {shortcut}
          </span>
        )}
      </button>
    </li>
  );
}

/**
 * The agent list region (AFW-A01, AFW-A05, DR-PX-2026-11-03-019 AWL-06): the brand, then the navigation entries in one column
 * (a new task, search, the Fleet board, worktrees, automations, customize, the dashboard), then the list. Below a 448 px centre
 * it is a 40 px rail.
 */
export function AgentRegion(p: AgentRegionProps) {
  const { rail, canExpand, taskCount, attentionCount, onNewTask, onExpand, list } = p;
  if (rail) {
    return (
      <div className="agent-rail" data-testid="agent-rail">
        {canExpand && <IconButton label="Expand agent list" icon={<IconSidebar />} onClick={onExpand} data-testid="rail-expand" />}
        <IconButton label="New task" icon={<IconPlus />} onClick={onNewTask} data-testid="rail-new-task" />
      </div>
    );
  }
  const mac = typeof navigator !== "undefined" && /Mac/.test(navigator.platform);
  const entries: NavEntry[] = [{ testId: "agents-new-task", icon: "square-pen", label: "New task", onClick: onNewTask }];
  if (p.onSearch) entries.push({ testId: "agents-search-open", icon: "search", label: "Search", onClick: p.onSearch, shortcut: mac ? "⌘K" : "Ctrl+K" });
  if (p.onShowFleet) entries.push({ testId: "agents-fleet", icon: "layout-grid", label: "Fleet board", onClick: p.onShowFleet, pressed: p.fleetActive === true });
  if (p.onShowWorktrees) entries.push({ testId: "agents-worktrees", icon: "git-branch", label: "Worktrees", onClick: p.onShowWorktrees, pressed: p.worktreesActive === true });
  if (p.onToggleAutomations) entries.push({ testId: "open-automations", icon: "clock", label: "Automations", onClick: p.onToggleAutomations, pressed: p.automationsOpen ?? false, disabled: p.automationsDisabled });
  if (p.onToggleCustomize) entries.push({ testId: "open-customize", icon: "blocks", label: "Customize", onClick: p.onToggleCustomize, pressed: p.customizeOpen ?? false, disabled: p.customizeDisabled });
  if (p.onToggleDashboard) entries.push({ testId: "open-dashboard", icon: "chart-column", label: p.dashboardOpen ? "Close dashboard" : "Dashboard", onClick: p.onToggleDashboard, disabled: p.dashboardDisabled });
  return (
    <div className="agent-region">
      <div className="agent-region-head">
        <span className="brand" data-testid="brand">
          Modbit
        </span>
      </div>
      <ul className="agent-nav" aria-label="Navigation">
        {entries.map((e) => (
          <NavRow key={e.testId} {...e} />
        ))}
      </ul>
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
