import { useRef, type KeyboardEvent, type PointerEvent } from "react";

export interface SplitterProps {
  label: string;
  value: number;
  min: number;
  max: number;
  /** A drag to this clientX (clientY for a horizontal separator); the owner converts it to its own size. */
  onDragTo: (client: number) => void;
  /** Keyboard: grow (+) or shrink (-) the region this separator sizes by `delta` px. */
  onNudge: (delta: number) => void;
  onJump: (to: "min" | "max") => void;
  onReset: () => void;
  /** The separator sizes a region to its right, so ArrowLeft grows it. */
  growsLeft?: boolean | undefined;
  /** Which edge of its region the separator sits on. */
  edge: "left" | "right" | "top";
  /** The separator's own direction: a vertical line between regions side by side (the default), or a horizontal one between a region and the one under it, moved with ArrowUp and ArrowDown. */
  axis?: "vertical" | "horizontal" | undefined;
  testId: string;
}

/**
 * A window separator (WAI-ARIA "window splitter"): focusable, exposes
 * aria-valuenow/min/max, moves with the arrow keys (Shift for a larger step),
 * Home and End jump to the limits, Enter resets, and the pointer drags it.
 */
export function Splitter({ label, value, min, max, onDragTo, onNudge, onJump, onReset, growsLeft = false, edge, axis = "vertical", testId }: SplitterProps) {
  const dragging = useRef(false);
  const onKey = (e: KeyboardEvent<HTMLDivElement>) => {
    const step = e.shiftKey ? 64 : 16;
    const dir = axis === "horizontal" ? (e.key === "ArrowDown" ? 1 : e.key === "ArrowUp" ? -1 : 0) : e.key === "ArrowRight" ? 1 : e.key === "ArrowLeft" ? -1 : 0;
    if (dir !== 0) {
      e.preventDefault();
      onNudge(dir * (growsLeft ? -1 : 1) * step);
    } else if (e.key === "Home") {
      e.preventDefault();
      onJump("min");
    } else if (e.key === "End") {
      e.preventDefault();
      onJump("max");
    } else if (e.key === "Enter") {
      e.preventDefault();
      onReset();
    }
  };
  const onDown = (e: PointerEvent<HTMLDivElement>) => {
    dragging.current = true;
    e.currentTarget.setPointerCapture(e.pointerId);
    e.preventDefault();
  };
  const onMove = (e: PointerEvent<HTMLDivElement>) => {
    if (dragging.current) onDragTo(axis === "horizontal" ? e.clientY : e.clientX);
  };
  const onUp = (e: PointerEvent<HTMLDivElement>) => {
    dragging.current = false;
    if (e.currentTarget.hasPointerCapture(e.pointerId)) e.currentTarget.releasePointerCapture(e.pointerId);
  };
  return (
    <div
      role="separator"
      aria-orientation={axis}
      aria-label={label}
      aria-valuenow={Math.round(value)}
      aria-valuemin={Math.round(min)}
      aria-valuemax={Math.round(Math.max(min, max))}
      tabIndex={0}
      className="splitter"
      data-edge={edge}
      data-axis={axis}
      data-testid={testId}
      onKeyDown={onKey}
      onPointerDown={onDown}
      onPointerMove={onMove}
      onPointerUp={onUp}
      onPointerCancel={onUp}
      onDoubleClick={onReset}
    />
  );
}
