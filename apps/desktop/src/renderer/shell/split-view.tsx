/**
 * Split view's two columns (DR-PX-2026-11-11-021 MWS-11, REQ-PX-204; board C2): two tasks side by side in the centre, each
 * with its own title bar, transcript, composer and approval card. The conversation is the one component the single view uses,
 * mounted once per column with its own tray store. One column has the keyboard and shows it (an accent rule along its top and
 * "has the keyboard" in its name); focus moving into a column, or a press in it, gives it the keyboard. A separator between
 * the two sizes them (arrows, Home, End, Enter resets), and each title bar's Close leaves split view with the other task.
 */
import { useMemo, useState, useRef, type ReactNode } from "react";
import type { MenuItem } from "@modbit/ui";
import { Splitter } from "./splitter.tsx";
import { TopBar, type PaneToggle } from "./top-bar.tsx";
import type { AppKind } from "./prefs.ts";
import { ratioForColumns, SPLIT_LAYOUT } from "./split.ts";

export interface SplitColumn {
  taskId: string;
  title: string;
  taskMenu?: readonly MenuItem[] | undefined;
  panes: readonly PaneToggle[];
  onPane: (kind: AppKind) => void;
}

export interface SplitViewProps {
  columns: readonly [SplitColumn, SplitColumn];
  widths: { left: number; right: number };
  /** The column that has the keyboard. */
  focus: 0 | 1;
  onFocus: (column: 0 | 1) => void;
  onClose: (column: 0 | 1) => void;
  onRatio: (ratio: number) => void;
  /** The narrowest a column may be here (424, or 560 beside the dock). */
  min: number;
  /** One column's conversation, given its title bar's end slot (More goes there) and whether it has the keyboard. */
  render: (column: 0 | 1, endSlot: HTMLElement | null, keyFocus: boolean) => ReactNode;
}

export function SplitView(p: SplitViewProps) {
  const root = useRef<HTMLDivElement>(null);
  // Each title bar's last slot, where its conversation puts More; the callbacks keep their identity so a render never re-mounts them.
  const [slots, setSlots] = useState<readonly [HTMLElement | null, HTMLElement | null]>([null, null]);
  const slotRefs = useMemo(() => [0, 1].map((i) => (el: HTMLElement | null) => setSlots((s) => (s[i] === el ? s : i === 0 ? [el, s[1]] : [s[0], el]))) as [(el: HTMLElement | null) => void, (el: HTMLElement | null) => void], []);
  const total = p.widths.left + p.widths.right;
  const setLeft = (px: number) => p.onRatio(ratioForColumns(Math.min(total - p.min, Math.max(p.min, px)), total));
  return (
    <div ref={root} className="split-view" data-testid="split-view" data-focus={p.focus}>
      {p.columns.map((c, i) => {
        const column = i as 0 | 1;
        const focused = p.focus === column;
        return (
          <div
            key={c.taskId}
            className="split-col"
            role="group"
            aria-label={focused ? `${c.title}, has the keyboard` : c.title}
            data-testid="split-column"
            data-column={column}
            data-task-id={c.taskId}
            data-focused={focused}
            style={{ width: column === 0 ? p.widths.left : p.widths.right }}
            onFocusCapture={() => {
              if (!focused) p.onFocus(column);
            }}
            onPointerDownCapture={() => {
              if (!focused) p.onFocus(column);
            }}
          >
            <TopBar title={c.title} taskMenu={c.taskMenu} endSlotRef={slotRefs[column]} panes={c.panes} onPane={c.onPane} onLeaveSplit={() => p.onClose(column)} />
            <div className="split-col-body">{p.render(column, slots[column], focused)}</div>
            {column === 0 && (
              <Splitter
                label="Resize the two tasks"
                testId="splitter-split"
                edge="right"
                value={p.widths.left}
                min={p.min}
                max={total - p.min}
                onDragTo={(x) => setLeft(x - (root.current?.getBoundingClientRect().left ?? 0))}
                onNudge={(d) => setLeft(p.widths.left + d)}
                onJump={(to) => setLeft(to === "min" ? p.min : total - p.min)}
                onReset={() => p.onRatio(SPLIT_LAYOUT.ratioDefault)}
              />
            )}
          </div>
        );
      })}
    </div>
  );
}
