/**
 * One pane of the dock (REQ-PX-202; DR-PX-2026-11-11-021 MWS-10, MWS-01): a head the height of the top bar with the pane's name,
 * its own actions, Pop out, Minimize and Close, over its body. The pane's content puts its own actions in the head through
 * `PaneActions`. Pop out (REQ-PX-203) is drawn only on the panes that can stand in a window of their own.
 */
import { createContext, useContext, useMemo, useState, type ReactNode, type Ref } from "react";
import { createPortal } from "react-dom";
import { Button, Icon, IconButton } from "@modbit/ui";
import { APP_LABEL } from "./artifacts.ts";
import type { AppKind } from "./prefs.ts";

/** The head's slot for the content's own actions; `null` outside a pane, `{ el: null }` for the frame before the head exists. */
const PaneHeadContext = createContext<{ el: HTMLElement | null } | null>(null);

/**
 * A pane's own actions. Inside a pane they stand in its head, after its name; a view shown outside the dock (the state gallery)
 * keeps them where it wrote them, in an element of `className`.
 */
export function PaneActions({ className, children }: { className?: string | undefined; children: ReactNode }) {
  const head = useContext(PaneHeadContext);
  if (head === null) return <div className={className}>{children}</div>;
  return head.el ? createPortal(children, head.el) : null;
}

export interface PaneFrameProps {
  kind: AppKind;
  /** The height the dock's model gives the pane. */
  height: number;
  onMinimize: () => void;
  onClose: () => void;
  /** Moves the pane to a window of its own (REQ-PX-203); absent for a pane that cannot be popped out. */
  onPopOut?: (() => void) | undefined;
  /** What stands above the head: the separator this pane shares with the one over it. */
  before?: ReactNode | undefined;
  paneRef?: Ref<HTMLElement> | undefined;
  children: ReactNode;
}

export function PaneFrame({ kind, height, onMinimize, onClose, onPopOut, before, paneRef, children }: PaneFrameProps) {
  const [slot, setSlot] = useState<HTMLElement | null>(null);
  const head = useMemo(() => ({ el: slot }), [slot]);
  const name = APP_LABEL[kind];
  return (
    <section ref={paneRef} className="pane" aria-label={`${name} pane`} data-testid={`pane-${kind}`} data-kind={kind} style={{ height }}>
      {before}
      <header className="pane-head" data-testid={`pane-head-${kind}`}>
        <h2 className="pane-name" data-testid={`pane-name-${kind}`}>
          {name}
        </h2>
        <span className="pane-actions" ref={setSlot} />
        {onPopOut && <IconButton label={`Pop out ${name}`} title="Pop out into its own window" icon={<Icon name="maximize-2" size={16} />} size="sm" data-testid={`pane-popout-${kind}`} onClick={onPopOut} />}
        <IconButton label={`Minimize ${name}`} title="Minimize" icon={<Icon name="chevron-down" size={16} />} size="sm" data-testid={`pane-minimize-${kind}`} onClick={onMinimize} />
        <IconButton label={`Close ${name}`} title="Close" icon={<Icon name="x" size={16} />} size="sm" data-testid={`pane-close-${kind}`} onClick={onClose} />
      </header>
      {/* The body scrolls, so it takes the keyboard (as the tab panel it replaces did). */}
      <div className="pane-body" role="group" aria-label={`${name}: content`} tabIndex={0} data-testid={`pane-body-${kind}`}>
        <PaneHeadContext.Provider value={head}>{children}</PaneHeadContext.Provider>
      </div>
    </section>
  );
}

/**
 * A pane in a window of its own (REQ-PX-203): the same head, the height of the top bar, with the pane's name, the task it
 * belongs to, its own actions and Back to the dock (which closes the window; the pane is then in the dock, as it was here).
 */
export function PaneWindowFrame({ kind, taskId, taskTitle, onReturn, children }: { kind: AppKind; taskId: string; taskTitle: string; onReturn: () => void; children: ReactNode }) {
  const [slot, setSlot] = useState<HTMLElement | null>(null);
  const head = useMemo(() => ({ el: slot }), [slot]);
  const name = APP_LABEL[kind];
  return (
    <main className="pane pane-window" aria-label={`${name}: ${taskTitle}`} data-testid="pane-window" data-kind={kind} data-task-id={taskId}>
      <header className="pane-head" data-testid={`pane-head-${kind}`}>
        <h1 className="pane-name" data-testid={`pane-name-${kind}`}>
          {name}
        </h1>
        <span className="pane-window-task" title={taskTitle} data-testid="pane-window-task">
          {taskTitle}
        </span>
        <span className="pane-actions" ref={setSlot} />
        <Button size="sm" variant="ghost" data-testid="pane-return" onClick={onReturn}>
          Back to the dock
        </Button>
      </header>
      <div className="pane-body" role="group" aria-label={`${name}: content`} tabIndex={0} data-testid={`pane-body-${kind}`}>
        <PaneHeadContext.Provider value={head}>{children}</PaneHeadContext.Provider>
      </div>
    </main>
  );
}
