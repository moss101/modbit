import type { ReactNode } from "react";
import { Icon, IconButton, Menu, type IconName, type MenuItem } from "@modbit/ui";
import { IconPlus, IconSidebar } from "./icons.tsx";

export interface AgentRegionProps {
  rail: boolean;
  /** The list was collapsed by the person (it can be expanded); false when the window is simply too narrow. */
  canExpand: boolean;
  taskCount: number;
  attentionCount: number;
  onNewTask: () => void;
  onExpand: () => void;
  onToggleAgentList: () => void;
  canGoBack: boolean;
  onBack: () => void;
  canGoForward: boolean;
  onForward: () => void;
  /** The agent list (PX-046): the region's typed extension point. Absent, the region says only what the model knows. */
  list?: ReactNode | undefined;
  /** The command palette (search tasks, commands and settings). */
  onSearch?: (() => void) | undefined;
  /** REQ-PX-086: the Automations surface. */
  automationsOpen?: boolean | undefined;
  automationsDisabled?: boolean | undefined;
  onToggleAutomations?: (() => void) | undefined;
  /** What needs the person across every task. */
  inboxOpen?: boolean | undefined;
  onToggleInbox?: (() => void) | undefined;
  /** REQ-PX-106: the Customize view over skills, hooks and extensions. */
  customizeOpen?: boolean | undefined;
  customizeDisabled?: boolean | undefined;
  onToggleCustomize?: (() => void) | undefined;
  /** The repositories the tasks ran in; a click starts a new task there. */
  repositories?: readonly string[] | undefined;
  onOpenRepository?: ((root: string) => void) | undefined;
  /** The person's name on this machine; the initials stand in for a picture. */
  userName?: string | undefined;
  coreLabel: string;
  coreStatus: "ok" | "warn" | "danger";
  /** The person's menu: the views that have no row of their own, theme and zoom. */
  menuItems: readonly MenuItem[];
  settingsOpen?: boolean | undefined;
  onOpenSettings: () => void;
}

interface NavEntry {
  testId: string;
  icon: IconName;
  label: string;
  onClick: () => void;
  pressed?: boolean | undefined;
  disabled?: boolean | undefined;
  shortcut?: string | undefined;
  count?: number | undefined;
}

/** One 28 px navigation row: an icon and a label, the same command and test id it has always had. */
function NavRow({ testId, icon, label, onClick, pressed, disabled, shortcut, count }: NavEntry) {
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
        {count !== undefined && count > 0 && (
          <span className="agent-nav-count" data-testid={`${testId}-count`}>
            {count}
          </span>
        )}
      </button>
    </li>
  );
}

const baseName = (p: string) => p.replace(/[/\\]+$/, "").split(/[/\\]/).pop() ?? p;
const initialsOf = (name: string) =>
  name
    .split(/\s+/)
    .filter(Boolean)
    .slice(0, 2)
    .map((w) => w[0]!.toUpperCase())
    .join("");

/** The foot of the left column, in the list and in Settings: the person (their name opens the menu), the Core's state and the gear, which opens Settings. */
export function PersonRow(p: Pick<AgentRegionProps, "userName" | "coreLabel" | "coreStatus" | "menuItems" | "settingsOpen" | "onOpenSettings">) {
  const name = p.userName ?? "";
  return (
    <footer className="agent-foot" data-testid="agent-foot">
      <Menu
        label={`${name || "Account"}: palette, shortcuts, theme and more`}
        trigger={
          <>
            <span className="agent-avatar" aria-hidden="true">
              {initialsOf(name)}
            </span>
            <span className="agent-nav-label agent-user" data-testid="agent-user">
              {name}
            </span>
          </>
        }
        items={p.menuItems}
        align="start"
        placement="up"
        triggerClassName="agent-person"
        triggerTestId="overflow-menu"
      />
      <span className="agent-core" data-testid="core-status" data-status={p.coreStatus} aria-live="polite" title={p.coreLabel}>
        <span className="agent-core-dot" aria-hidden="true" />
        <span className="mb-sr-only">{p.coreLabel}</span>
      </span>
      <IconButton label="Settings" icon={<Icon name="settings" />} pressed={p.settingsOpen ?? false} onClick={p.onOpenSettings} data-testid="open-settings" />
    </footer>
  );
}

/**
 * The agent list region: window controls, the navigation rows, the agent list, the repositories and the person's own row. Below
 * a 448 px centre it is a 40 px rail.
 */
export function AgentRegion(p: AgentRegionProps) {
  const { rail, canExpand, taskCount, attentionCount, onNewTask, onExpand, list } = p;
  if (rail) {
    return (
      <div className="agent-rail" data-testid="agent-rail">
        {canExpand && <IconButton label="Expand agent list" icon={<IconSidebar />} onClick={onExpand} data-testid="rail-expand" />}
        <IconButton label="New task" icon={<IconPlus />} onClick={onNewTask} data-testid="rail-new-task" />
        <IconButton label="Back" icon={<Icon name="arrow-left" />} disabled={!p.canGoBack} onClick={p.onBack} data-testid="nav-back" />
      </div>
    );
  }
  const mac = typeof navigator !== "undefined" && /Mac/.test(navigator.platform);
  const entries: NavEntry[] = [{ testId: "agents-new-task", icon: "square-pen", label: "New task", onClick: onNewTask, shortcut: mac ? "⌘N" : "Ctrl+N" }];
  if (p.onSearch) entries.push({ testId: "agents-search-open", icon: "search", label: "Search", onClick: p.onSearch, shortcut: mac ? "⌘K" : "Ctrl+K" });
  if (p.onToggleAutomations) entries.push({ testId: "open-automations", icon: "clock", label: "Automations", onClick: p.onToggleAutomations, pressed: p.automationsOpen ?? false, disabled: p.automationsDisabled });
  if (p.onToggleInbox) entries.push({ testId: "open-inbox", icon: "shield-check", label: "Approvals", onClick: p.onToggleInbox, pressed: p.inboxOpen ?? false, count: attentionCount });
  if (p.onToggleCustomize) entries.push({ testId: "open-customize", icon: "blocks", label: "Customize", onClick: p.onToggleCustomize, pressed: p.customizeOpen ?? false, disabled: p.customizeDisabled });
  return (
    <div className="agent-region">
      <div className="agent-region-head">
        <IconButton label="Agent list" icon={<IconSidebar />} onClick={p.onToggleAgentList} data-testid="toggle-agent-list" />
        <span className="topbar-spacer" />
        <IconButton label="Back" icon={<Icon name="arrow-left" />} disabled={!p.canGoBack} onClick={p.onBack} data-testid="nav-back" />
        <IconButton label="Forward" icon={<Icon name="arrow-right" />} disabled={!p.canGoForward} onClick={p.onForward} data-testid="nav-forward" />
      </div>
      <ul className="agent-nav" aria-label="Navigation">
        {entries.map((e) => (
          <NavRow key={e.testId} {...e} />
        ))}
      </ul>
      <p className="mb-sr-only" data-testid="agents-summary">
        {taskCount} {taskCount === 1 ? "task" : "tasks"}, {attentionCount} need{attentionCount === 1 ? "s" : ""} attention.
      </p>
      {list ?? (
        <p className="meta empty agent-region-meta" data-testid="agents-placeholder">
          The agent list is not connected in this view.
        </p>
      )}
      {p.repositories && p.repositories.length > 0 && (
        <section className="agent-repos" aria-label="Repositories" data-testid="agent-repositories">
          <h2 className="agents-section-label agent-repos-head">Repositories</h2>
          <ul className="agent-nav">
            {p.repositories.map((root) => (
              <li key={root}>
                <button type="button" className="agent-nav-row" title={root} data-testid="agent-repository" data-root={root} onClick={() => p.onOpenRepository?.(root)}>
                  <Icon name="folder" size={14} />
                  <span className="agent-nav-label">{baseName(root)}</span>
                </button>
              </li>
            ))}
          </ul>
        </section>
      )}
      <PersonRow {...p} />
    </div>
  );
}
