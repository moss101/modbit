import { useLayoutEffect, useState, type Ref } from "react";
import { Icon, IconButton, Menu, type IconName, type MenuItem } from "@modbit/ui";
import { APP_LABEL } from "./artifacts.ts";
import type { AppKind } from "./prefs.ts";
import type { PaneState } from "./dock.ts";
import { fitCount, shownToggles, TITLE_FLOOR, wantedToggles } from "./title-fit.ts";

/** One pane the open task can show, and where it is now: in the grid, minimized out of it, or closed. */
export interface PaneToggle {
  kind: AppKind;
  open: boolean;
  state: PaneState;
}

export interface TopBarProps {
  /** The open task's goal or the page's name. */
  title: string;
  /** The open task's menu (pin, archive); absent where the title is a page's name or the task list is not there to act. */
  taskMenu?: readonly MenuItem[] | undefined;
  /** Where an open conversation puts More, after the pane toggles. */
  endSlotRef: Ref<HTMLSpanElement>;
  /** Every pane the open task can show (MWS-05, MWS-10); empty where no task is open or it has nothing to show yet. A few stand in the bar; all are in the Panes menu. */
  panes: readonly PaneToggle[];
  onPane: (kind: AppKind) => void;
  /** The details rail's toggle (MWS-09): whether the rail is shown now. Absent where no task is open or the task area has no room for the rail (its facts are then under More). */
  details?: { shown: boolean; onToggle: () => void } | undefined;
}

export const APP_ICON: Record<AppKind, IconName> = { changes: "file-diff", terminal: "terminal", browser: "globe", files: "file-text", evidence: "eye", activity: "clock", background: "layers", "pull-request": "git-compare" };

/** What a pane's toggle does next, as its tooltip: an open pane closes, a minimized one comes back, a closed one opens. */
export const paneToggleTitle = (kind: AppKind, state: PaneState): string => (state === "open" ? `Hide ${APP_LABEL[kind]}` : state === "minimized" ? `Show ${APP_LABEL[kind]} (minimized)` : `Show ${APP_LABEL[kind]}`);

/**
 * The pane toggles of a bar: the ones `shownToggles` keeps (Changes, Terminal, and what is open or minimized), as many as fit
 * beside the title, then the Panes menu, which lists every pane with its state and does what its toggle does. Drawn in the
 * title bar, and in the dock's own bar where the centre shows something other than the dock's task.
 */
export function PaneToggles({ panes, onPane, fit, size }: { panes: readonly PaneToggle[]; onPane: (kind: AppKind) => void; fit: number; size?: "sm" | "md" | undefined }) {
  if (panes.length === 0) return null;
  const shown = new Set(shownToggles(panes, fit));
  const items: MenuItem[] = panes.map((pane) => ({
    id: `pane-${pane.kind}`,
    label: pane.state === "minimized" ? `${APP_LABEL[pane.kind]} (minimized)` : APP_LABEL[pane.kind],
    checked: pane.state !== "closed",
    testId: `panes-menu-${pane.kind}`,
    onSelect: () => onPane(pane.kind),
  }));
  return (
    <>
      {shown.size > 0 && (
        <span className="topbar-panes" role="group" aria-label="Panes" data-testid="pane-toggles">
          {panes
            .filter((pane) => shown.has(pane.kind))
            .map((pane) => (
              <IconButton key={pane.kind} size={size ?? "md"} label={pane.state === "minimized" ? `${APP_LABEL[pane.kind]} pane, minimized` : `${APP_LABEL[pane.kind]} pane`} title={paneToggleTitle(pane.kind, pane.state)} icon={<Icon name={APP_ICON[pane.kind]} size={size === "sm" ? 16 : undefined} />} pressed={pane.open} data-open={pane.open} data-state={pane.state} data-testid={`pane-toggle-${pane.kind}`} onClick={() => onPane(pane.kind)} />
            ))}
        </span>
      )}
      <span className="topbar-panes-menu" data-testid="panes-menu-host" data-hidden={panes.length - shown.size}>
        <Menu label="Panes" triggerTestId="panes-menu" trigger={<Icon name="layout-grid" size={size === "sm" ? 16 : undefined} />} items={items} align="end" />
      </span>
    </>
  );
}

/**
 * How many toggles fit in a bar: the bar's width less its padding, gaps, the title (its whole text, up to what is left: the
 * title has priority), and every other control in it. Measured on the bar, again whenever it or the title changes size.
 */
export function useToggleFit(bar: HTMLElement | null, wanted: number, titleSelector: string, floor: number): number {
  const [fit, setFit] = useState(wanted);
  useLayoutEffect(() => {
    if (!bar) return;
    const measure = () => {
      const style = getComputedStyle(bar);
      const gap = parseFloat(style.columnGap) || 0;
      const padding = (parseFloat(style.paddingLeft) || 0) + (parseFloat(style.paddingRight) || 0);
      const children = [...bar.children] as HTMLElement[];
      const fixed = children.filter((c) => !c.matches(`${titleSelector}, .topbar-spacer, .topbar-panes`) && getComputedStyle(c).display !== "none").reduce((sum, c) => sum + c.getBoundingClientRect().width, 0);
      const title = bar.querySelector<HTMLElement>(`${titleSelector} .topbar-title-text`) ?? bar.querySelector<HTMLElement>(titleSelector);
      // The title's own width with nothing cut, and the controls a title button carries around it.
      const titleBox = bar.querySelector<HTMLElement>(titleSelector);
      const chrome = titleBox && title && title !== titleBox ? Math.max(0, titleBox.getBoundingClientRect().width - title.getBoundingClientRect().width) : 0;
      const titleWant = title ? Math.max(floor, title.scrollWidth + chrome) : 0;
      const gaps = gap * Math.max(0, children.length - 1);
      setFit(fitCount(bar.clientWidth - padding - fixed - gaps - titleWant, wanted));
    };
    measure();
    const ro = new ResizeObserver(measure);
    ro.observe(bar);
    const t = bar.querySelector(titleSelector);
    if (t) ro.observe(t);
    return () => ro.disconnect();
  }, [bar, wanted, titleSelector, floor]);
  return fit;
}

/**
 * The centre's top bar (45 px, 40 in Compact: `--mb-top-bar`; DR-PX-2026-11-11-021 MWS-05): the title, which opens the task's
 * menu; then the details rail's toggle, the pane toggles that fit (Changes, Terminal, and what is open or minimized) and the
 * Panes menu with every pane; then More. The title has priority: toggles move into the Panes menu before it is cut.
 * The task's state and branch are in the details rail (MWS-09).
 */
export function TopBar(p: TopBarProps) {
  const [bar, setBar] = useState<HTMLElement | null>(null);
  const fit = useToggleFit(bar, wantedToggles(p.panes), ".topbar-title", TITLE_FLOOR);
  return (
    <header ref={setBar} className="topbar" data-testid="top-bar" data-toggle-fit={fit}>
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
          <span className="topbar-title-text">{p.title}</span>
        )}
      </h1>
      <span className="topbar-spacer" />
      {p.details && <IconButton label="Details" title={p.details.shown ? "Hide details" : "Show details"} icon={<Icon name="panel-right" />} pressed={p.details.shown} data-open={p.details.shown} data-testid="details-toggle" onClick={p.details.onToggle} />}
      <PaneToggles panes={p.panes} onPane={p.onPane} fit={fit} />
      <span className="topbar-end" ref={p.endSlotRef} />
    </header>
  );
}
