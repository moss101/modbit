import type { Ref } from "react";
import { IconButton } from "@modbit/ui";
import { IconPanel } from "./icons.tsx";

export interface TopBarProps {
  /** The open task's goal or the page's name. */
  title: string;
  /** Where an open conversation puts its own controls (its state, branch and overflow). */
  slotRef: Ref<HTMLSpanElement>;
  panelOpen: boolean;
  /** No task to show artifacts of: the toggle says so. */
  panelUnavailable: string | null;
  onTogglePanel: () => void;
}

/** The centre's top bar (45 px, 40 in Compact: `--mb-top-bar`): the title, the open conversation's controls and, while the apps panel is closed, its toggle (it moves into the panel when open). */
export function TopBar(p: TopBarProps) {
  return (
    <header className="topbar" data-testid="top-bar">
      <h1 className="topbar-title" data-testid="top-bar-title" title={p.title}>
        {p.title}
      </h1>
      <span className="topbar-slot" ref={p.slotRef} />
      {!p.panelOpen && <IconButton label={p.panelUnavailable ? `Apps panel (${p.panelUnavailable})` : "Apps panel"} icon={<IconPanel />} pressed={false} aria-disabled={p.panelUnavailable !== null} data-testid="toggle-apps-panel" onClick={() => p.panelUnavailable === null && p.onTogglePanel()} />}
    </header>
  );
}
