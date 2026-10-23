import { useRef, type KeyboardEvent, type PointerEvent } from "react";

export interface SplitterProps {
  label: string;
  value: number;
  min: number;
  max: number;
  /** A drag to this clientX; the owner converts it to its own width. */
  onDragTo: (clientX: number) => void;
  /** Keyboard: grow (+) or shrink (-) the region this separator sizes by `delta` px. */
  onNudge: (delta: number) => void;
  onJump: (to: "min" | "max") => void;
  onReset: () => void;
  /** The separator sizes a region to its right, so ArrowLeft grows it. */
  growsLeft?: boolean | undefined;
  /** Which edge of its region the separator sits on. */
  edge: "left" | "right";
  testId: string;
}

/**
 * A window separator (WAI-ARIA "window splitter"): focusable, exposes
 * aria-valuenow/min/max, moves with the arrow keys (Shift for a larger step),
 * Home and End jump to the limits, Enter resets, and the pointer drags it.
 */
export function Splitter({ label, value, min, max, onDragTo, onNudge, onJump, onReset, growsLeft = false, edge, testId }: SplitterProps) {
  const dragging = useRef(false);
  const onKey = (e: KeyboardEvent<HTMLDivElement>) => {
    const step = e.shiftKey ? 64 : 16;
    const dir = e.key === "ArrowRight" ? 1 : e.key === "ArrowLeft" ? -1 : 0;
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
    if (dragging.current) onDragTo(e.clientX);
  };
  const onUp = (e: PointerEvent<HTMLDivElement>) => {
    dragging.current = false;
    if (e.currentTarget.hasPointerCapture(e.pointerId)) e.currentTarget.releasePointerCapture(e.pointerId);
  };
  return (
    <div
      role="separator"
      aria-orientation="vertical"
      aria-label={label}
      aria-valuenow={Math.round(value)}
      aria-valuemin={Math.round(min)}
      aria-valuemax={Math.round(Math.max(min, max))}
      tabIndex={0}
      className="splitter"
      data-edge={edge}
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
