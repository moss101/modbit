import { useId, useRef, type ReactNode, type RefObject } from "react";
import { useFocusTrap, useKeyScope, useLayer } from "./hooks.ts";

export interface DialogProps {
  open: boolean;
  /** The visible title; it names the dialog (aria-labelledby). */
  title: string;
  onClose: () => void;
  children: ReactNode;
  /** "alertdialog" for a dialog that needs an answer. */
  role?: "dialog" | "alertdialog" | undefined;
  /** Clicking the scrim closes it. Default true. */
  dismissOnScrim?: boolean | undefined;
  /** The element that takes focus on open; the first focusable otherwise. */
  initialFocus?: RefObject<HTMLElement | null> | undefined;
  size?: "sm" | "md" | "lg" | undefined;
  testId?: string | undefined;
  /** Extra class for the panel (e.g. the palette's). */
  className?: string | undefined;
  /** Hide the visible title bar (the title still names the dialog for assistive technology). */
  hideTitle?: boolean | undefined;
}

/**
 * A modal dialog: role dialog, aria-modal, named by its title, focus moves in
 * and is trapped, Escape closes it (the layer stack), focus returns to the
 * opener, and the shell's shortcuts are blocked while it is open.
 */
export function Dialog({ open, title, onClose, children, role = "dialog", dismissOnScrim = true, initialFocus, size = "md", testId, className, hideTitle = false }: DialogProps) {
  const titleId = useId();
  const panel = useRef<HTMLDivElement>(null);
  useLayer(open, `dialog:${titleId}`, true, onClose);
  useKeyScope(open, { id: `dialog:${titleId}`, blocking: true, resolve: () => null });
  useFocusTrap(panel, open, initialFocus);
  if (!open) return null;
  return (
    <div className="mb-scrim" data-testid={testId ? `${testId}-scrim` : undefined} onPointerDown={(e) => dismissOnScrim && e.target === e.currentTarget && onClose()}>
      <div ref={panel} role={role} aria-modal="true" aria-labelledby={titleId} tabIndex={-1} className={`mb-dialog${className ? ` ${className}` : ""}`} data-size={size} data-testid={testId}>
        <h2 id={titleId} className={hideTitle ? "mb-sr-only" : "mb-dialog-title"}>
          {title}
        </h2>
        {children}
      </div>
    </div>
  );
}
