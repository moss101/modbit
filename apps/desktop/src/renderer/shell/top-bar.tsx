import type { Ref } from "react";
import { Icon, IconButton, Menu, type IconName, type MenuItem } from "@modbit/ui";
import { APP_LABEL } from "./artifacts.ts";
import type { AppKind } from "./prefs.ts";

/** One pane the open task can show, and whether it is open now. */
export interface PaneToggle {
  kind: AppKind;
  open: boolean;
}

export interface TopBarProps {
  /** The open task's goal or the page's name. */
  title: string;
  /** The open task's menu (pin, archive); absent where the title is a page's name or the task list is not there to act. */
  taskMenu?: readonly MenuItem[] | undefined;
  /** Where an open conversation puts its state and branch. */
  slotRef: Ref<HTMLSpanElement>;
  /** Where an open conversation puts More, after the pane toggles. */
  endSlotRef: Ref<HTMLSpanElement>;
  /** One toggle per pane the open task can show (MWS-05); empty where no task is open or it has nothing to show yet. */
  panes: readonly PaneToggle[];
  onPane: (kind: AppKind) => void;
}

export const APP_ICON: Record<AppKind, IconName> = { changes: "file-diff", terminal: "terminal", browser: "globe", files: "file-text", evidence: "eye" };

/**
 * The centre's top bar (45 px, 40 in Compact: `--mb-top-bar`; DR-PX-2026-11-11-021 MWS-05): the title, which opens the task's
 * menu; the open conversation's state and branch; then one toggle per pane the task can show, each saying whether it is open;
 * then More.
 */
export function TopBar(p: TopBarProps) {
  return (
    <header className="topbar" data-testid="top-bar">
      <h1 className="topbar-title" data-testid="top-bar-title" title={p.title}>
        {p.taskMenu && p.taskMenu.length > 0 ? (
          <Menu
            label={`Task menu: ${p.title}`}
            triggerTestId="task-menu"
            triggerClassName="topbar-title-btn"
            trigger={
              <>
                <span className="topbar-title-text">{p.title}</span>
                <Icon name="chevron-down" size={14} />
              </>
            }
            items={p.taskMenu}
          />
        ) : (
          p.title
        )}
      </h1>
      <span className="topbar-slot" ref={p.slotRef} />
      {p.panes.length > 0 && (
        <span className="topbar-panes" role="group" aria-label="Panes" data-testid="pane-toggles">
          {p.panes.map((pane) => (
            <IconButton key={pane.kind} label={`${APP_LABEL[pane.kind]} pane`} title={pane.open ? `Hide ${APP_LABEL[pane.kind]}` : `Show ${APP_LABEL[pane.kind]}`} icon={<Icon name={APP_ICON[pane.kind]} />} pressed={pane.open} data-open={pane.open} data-testid={`pane-toggle-${pane.kind}`} onClick={() => p.onPane(pane.kind)} />
          ))}
        </span>
      )}
      <span className="topbar-end" ref={p.endSlotRef} />
    </header>
  );
}
