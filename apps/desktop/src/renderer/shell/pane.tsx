/**
 * One pane of the dock (REQ-PX-202; DR-PX-2026-11-11-021 MWS-10, MWS-01): a head the height of the top bar with the pane's name,
 * its own actions, Minimize and Close, over its body. The pane's content puts its own actions in the head through `PaneActions`.
 * Pop out (PX-203) is not built, so no control for it is drawn.
 */
import { createContext, useContext, useMemo, useState, type ReactNode, type Ref } from "react";
import { createPortal } from "react-dom";
import { Icon, IconButton } from "@modbit/ui";
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
  /** What stands above the head: the separator this pane shares with the one over it. */
  before?: ReactNode | undefined;
  paneRef?: Ref<HTMLElement> | undefined;
  children: ReactNode;
}

export function PaneFrame({ kind, height, onMinimize, onClose, before, paneRef, children }: PaneFrameProps) {
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
