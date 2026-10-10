import { Badge, Button, IconButton, Menu, type MenuItem } from "@modbit/ui";
import { IconBack, IconLocal, IconMore, IconPanel, IconSidebar } from "./icons.tsx";

export interface TopBarProps {
  /** "Fleet" or the open task's goal. */
  title: string;
  /** The execution location of the shown task (AFW-A04): always text as well as a glyph. */
  location: string;
  agentListCollapsed: boolean;
  onToggleAgentList: () => void;
  canGoBack: boolean;
  onBack: () => void;
  menuItems: readonly MenuItem[];
  dashboardOpen: boolean;
  dashboardDisabled: boolean;
  onToggleDashboard: () => void;
  /** REQ-PX-086: the Automations surface. */
  automationsOpen?: boolean | undefined;
  automationsDisabled?: boolean | undefined;
  onToggleAutomations?: (() => void) | undefined;
  panelOpen: boolean;
  /** Why the panel cannot open (no task has an artifact), or null when it can. */
  panelUnavailable: string | null;
  onTogglePanel: () => void;
}

/** The 40 px top bar (AFW-A04): sidebar toggle, back, title with location, dashboard, overflow menu and the apps-panel toggle. */
export function TopBar(p: TopBarProps) {
  return (
    <header className="topbar" data-testid="top-bar">
      <IconButton label="Agent list" icon={<IconSidebar />} pressed={!p.agentListCollapsed} onClick={p.onToggleAgentList} data-testid="toggle-agent-list" />
      <IconButton label="Back to the fleet" icon={<IconBack />} disabled={!p.canGoBack} onClick={p.onBack} data-testid="nav-back" />
      <h1 className="topbar-title" data-testid="top-bar-title" title={p.title}>
        {p.title}
      </h1>
      <Badge tone="neutral">
        <span aria-hidden="true" className="mb-icon">
          <IconLocal />
        </span>{" "}
        {p.location}
      </Badge>
      <span className="topbar-spacer" />
      {p.onToggleAutomations && (
        <Button size="sm" data-testid="open-automations" aria-pressed={p.automationsOpen ?? false} disabled={p.automationsDisabled ?? false} onClick={p.onToggleAutomations}>
          Automations
        </Button>
      )}
      <Button size="sm" data-testid="open-dashboard" disabled={p.dashboardDisabled} onClick={p.onToggleDashboard}>
        {p.dashboardOpen ? "Close dashboard" : "Dashboard"}
      </Button>
      <Menu label="More actions" trigger={<IconMore />} items={p.menuItems} align="end" triggerTestId="overflow-menu" />
      <IconButton label={p.panelUnavailable ? `Apps panel (${p.panelUnavailable})` : "Apps panel"} icon={<IconPanel />} pressed={p.panelOpen} aria-disabled={p.panelUnavailable !== null} data-testid="toggle-apps-panel" onClick={() => p.panelUnavailable === null && p.onTogglePanel()} />
    </header>
  );
}
