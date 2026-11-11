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
  /** REQ-PX-086: the Automations surface. */
  automationsOpen?: boolean | undefined;
  automationsDisabled?: boolean | undefined;
  onToggleAutomations?: (() => void) | undefined;
  /** Inbox: what needs the person across every task (the count is the Core's attention count). */
  inboxOpen?: boolean | undefined;
  onToggleInbox?: (() => void) | undefined;
  /** REQ-PX-106: the Customize view over skills, hooks and extensions. */
  customizeOpen?: boolean | undefined;
  customizeDisabled?: boolean | undefined;
  onToggleCustomize?: (() => void) | undefined;
  /** The person's name on this machine; the initials stand in for a picture. */
  userName?: string | undefined;
  coreLabel: string;
  coreStatus: "ok" | "warn" | "danger";
  /** The person's menu: the views that have no row of their own, theme and zoom. */
  menuItems: readonly MenuItem[];
  settingsOpen?: boolean | undefined;
  onOpenSettings: () => void;
  /** The keyboard shortcuts; the help button is absent where nothing opens them. */
  onOpenHelp?: (() => void) | undefined;
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

/** One navigation row (32 px, 30 in Compact: `--mb-row`): an icon and a label, the same command and test id it has always had. */
function NavRow({ testId, icon, label, onClick, pressed, disabled, shortcut, count }: NavEntry) {
  return (
    <li>
      <button type="button" className="agent-nav-row" data-testid={testId} onClick={onClick} disabled={disabled ?? false} {...(pressed === undefined ? {} : { "aria-pressed": pressed })}>
        <Icon name={icon} />
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

const initialsOf = (name: string) =>
  name
    .split(/\s+/)
    .filter(Boolean)
    .slice(0, 2)
    .map((w) => w[0]!.toUpperCase())
    .join("");

/**
 * The foot of the left column, in the list and in Settings (MWS-04): the person (their name opens the menu), the Core's state in
 * a word with the whole of it in the accessibility tree, help and the gear, which opens Settings.
 */
export function PersonRow(p: Pick<AgentRegionProps, "userName" | "coreLabel" | "coreStatus" | "menuItems" | "settingsOpen" | "onOpenSettings" | "onOpenHelp">) {
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
        <span className="agent-core-word" aria-hidden="true">
          {p.coreStatus === "ok" ? "Core" : p.coreLabel}
        </span>
        <span className="mb-sr-only">{p.coreLabel}</span>
      </span>
      <span className="topbar-spacer" />
      {p.onOpenHelp && <IconButton label="Keyboard shortcuts" icon={<Icon name="circle-question-mark" />} onClick={p.onOpenHelp} data-testid="open-help" />}
      <IconButton label="Settings" icon={<Icon name="settings" />} pressed={p.settingsOpen ?? false} onClick={p.onOpenSettings} data-testid="open-settings" />
    </footer>
  );
}

/**
 * The task list region (MWS-04), top to bottom: the head (list toggle, back, forward; the window controls on macOS), the
 * navigation rows (New task, Inbox with its count, Automations, Customize), the task list with its own Tasks head, and the
 * person's row. Below a 448 px centre it is a 40 px rail.
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
  const entries: NavEntry[] = [{ testId: "agents-new-task", icon: "plus", label: "New task", onClick: onNewTask, shortcut: mac ? "⌘N" : "Ctrl+N" }];
  if (p.onToggleInbox) entries.push({ testId: "open-inbox", icon: "shield-check", label: "Inbox", onClick: p.onToggleInbox, pressed: p.inboxOpen ?? false, count: attentionCount });
  if (p.onToggleAutomations) entries.push({ testId: "open-automations", icon: "clock", label: "Automations", onClick: p.onToggleAutomations, pressed: p.automationsOpen ?? false, disabled: p.automationsDisabled });
  if (p.onToggleCustomize) entries.push({ testId: "open-customize", icon: "blocks", label: "Customize", onClick: p.onToggleCustomize, pressed: p.customizeOpen ?? false, disabled: p.customizeDisabled });
  return (
    <div className="agent-region">
      <div className="agent-region-head">
        <IconButton label="Agent list" icon={<IconSidebar />} onClick={p.onToggleAgentList} data-testid="toggle-agent-list" />
        <span className="topbar-spacer" />
        <IconButton label="Back" icon={<Icon name="arrow-left" />} disabled={!p.canGoBack} onClick={p.onBack} data-testid="nav-back" />
        <IconButton label="Forward" icon={<Icon name="arrow-right" />} disabled={!p.canGoForward} onClick={p.onForward} data-testid="nav-forward" />
      </div>
      <ul className="agent-nav" aria-label="Navigation" data-testid="agent-nav">
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
      <PersonRow {...p} />
    </div>
  );
}
