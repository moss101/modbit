import { cloneElement, useCallback, useEffect, useId, useRef, useState, type ReactElement } from "react";
import { useLayer } from "./hooks.ts";
import { Kbd } from "./status.tsx";

export interface TooltipProps {
  text: string;
  /** A chord hint such as `mod+k`, shown after the text as a key cap (display only: the command registry binds it). */
  shortcut?: string | undefined;
  /** The single focusable child the tooltip describes. */
  children: ReactElement<{ "aria-describedby"?: string; onFocus?: (e: never) => void; onBlur?: (e: never) => void }>;
  /** Hover delay in ms; focus shows it at once. */
  delay?: number | undefined;
}

/**
 * A description shown on hover and on keyboard focus, linked with
 * aria-describedby, dismissed with Escape without moving focus (WCAG 1.4.13),
 * and hoverable: the host wraps trigger and bubble, so moving the pointer
 * onto the bubble does not dismiss it.
 */
export function Tooltip({ text, shortcut, children, delay = 400 }: TooltipProps) {
  const id = useId();
  const [open, setOpen] = useState(false);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const clear = () => {
    if (timer.current) clearTimeout(timer.current);
    timer.current = null;
  };
  useEffect(() => clear, []);
  const show = useCallback(
    (immediately: boolean) => {
      clear();
      if (immediately) setOpen(true);
      else timer.current = setTimeout(() => setOpen(true), delay);
    },
    [delay],
  );
  const hide = useCallback(() => {
    clear();
    setOpen(false);
  }, []);
  useLayer(open, `tooltip:${id}`, false, hide);
  const child = cloneElement(children, { "aria-describedby": id, onFocus: () => show(true), onBlur: hide });
  return (
    <span className="mb-tooltip-host" onMouseEnter={() => show(false)} onMouseLeave={hide}>
      {child}
      <span id={id} role="tooltip" className="mb-tooltip" hidden={!open}>
        {text}
        {shortcut && <Kbd chord={shortcut} />}
      </span>
    </span>
  );
}
