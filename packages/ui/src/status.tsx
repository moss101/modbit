import type { ReactNode } from "react";
import { chordParts, isMacPlatform } from "./keys.ts";

export type Status = "ok" | "warn" | "danger" | "info" | "running" | "idle";

/** Each status has its own glyph shape, so status is never conveyed by colour alone (AFW-A14). */
const GLYPH: Record<Status, ReactNode> = {
  ok: (
    <svg viewBox="0 0 12 12" aria-hidden="true">
      <path d="M2 6.5 5 9.5 10.5 3" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  ),
  warn: (
    <svg viewBox="0 0 12 12" aria-hidden="true">
      <path d="M6 1.5 11 10.5H1Z" fill="currentColor" />
    </svg>
  ),
  danger: (
    <svg viewBox="0 0 12 12" aria-hidden="true">
      <path d="M2.5 2.5 9.5 9.5M9.5 2.5 2.5 9.5" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" />
    </svg>
  ),
  info: (
    <svg viewBox="0 0 12 12" aria-hidden="true">
      <circle cx="6" cy="6" r="4.5" fill="none" stroke="currentColor" strokeWidth="1.5" />
      <circle cx="6" cy="3.8" r="0.9" fill="currentColor" />
      <path d="M6 5.6V9" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
    </svg>
  ),
  running: (
    <svg viewBox="0 0 12 12" aria-hidden="true" className="mb-spin">
      <circle cx="6" cy="6" r="4.5" fill="none" stroke="currentColor" strokeWidth="1.5" strokeDasharray="14 14" strokeLinecap="round" />
    </svg>
  ),
  idle: (
    <svg viewBox="0 0 12 12" aria-hidden="true">
      <circle cx="6" cy="6" r="4" fill="none" stroke="currentColor" strokeWidth="1.5" />
    </svg>
  ),
};

export const STATUS_TEXT: Record<Status, string> = { ok: "OK", warn: "Warning", danger: "Error", info: "Info", running: "Running", idle: "Idle" };

export interface StatusDotProps {
  status: Status;
  /** The text alternative; defaults to the status word. With `showLabel` it is also shown. */
  label?: string | undefined;
  showLabel?: boolean | undefined;
}

export function StatusDot({ status, label, showLabel = false }: StatusDotProps) {
  const text = label ?? STATUS_TEXT[status];
  return (
    <span className="mb-dot" data-status={status}>
      <span className="mb-dot-glyph" aria-hidden="true">
        {GLYPH[status]}
      </span>
      {showLabel ? <span className="mb-dot-label">{text}</span> : <span className="mb-sr-only">{text}</span>}
    </span>
  );
}

export type BadgeTone = "neutral" | "ok" | "warn" | "danger" | "info" | "accent";

/** A short text chip. It is always text (a count or a word), so it is never colour-only. */
export function Badge({ tone = "neutral", children }: { tone?: BadgeTone | undefined; children: ReactNode }) {
  return (
    <span className="mb-badge" data-tone={tone}>
      {children}
    </span>
  );
}

/** A keyboard hint for a chord such as `mod+k`, spelled for the platform. */
export function Kbd({ chord, children }: { chord?: string | undefined; children?: ReactNode }) {
  if (children !== undefined) return <kbd className="mb-kbd">{children}</kbd>;
  const parts = chord ? chordParts(chord, isMacPlatform()) : [];
  return (
    <kbd className="mb-kbd" data-chord={chord}>
      {parts.join(isMacPlatform() ? "" : "+")}
    </kbd>
  );
}
