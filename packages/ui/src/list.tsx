import { type KeyboardEvent, type ReactNode } from "react";
import { navTarget } from "./nav.ts";

/** A list of navigation rows; ArrowUp/Down, Home and End move focus between the rows without wrapping. */
export function List({ label, children, testId }: { label: string; children: ReactNode; testId?: string | undefined }) {
  const onKey = (e: KeyboardEvent<HTMLUListElement>) => {
    const rows = [...e.currentTarget.querySelectorAll<HTMLElement>("[data-mb-row]")];
    const at = rows.indexOf(document.activeElement as HTMLElement);
    const t = navTarget(e.key, at, rows.length, "vertical", false);
    if (t === null) return;
    e.preventDefault();
    rows[t]?.focus();
  };
  return (
    <ul className="mb-list" aria-label={label} data-testid={testId} onKeyDown={onKey}>
      {children}
    </ul>
  );
}

export interface RowProps {
  title: ReactNode;
  subtitle?: ReactNode | undefined;
  /** A status glyph or icon at the left. */
  leading?: ReactNode | undefined;
  /** An age, a badge or a count at the right. */
  trailing?: ReactNode | undefined;
  selected?: boolean | undefined;
  onActivate?: (() => void) | undefined;
  /** Full-text description shown to assistive technology and as the native tooltip. */
  description?: string | undefined;
  testId?: string | undefined;
}

/** One row: a single control that names itself from its title and subtitle; `aria-current` marks the selected one. */
export function Row({ title, subtitle, leading, trailing, selected, onActivate, description, testId }: RowProps) {
  return (
    <li className="mb-row-item">
      <button type="button" data-mb-row="" className="mb-row" aria-current={selected ? "true" : undefined} title={description} data-testid={testId} data-selected={selected ? "true" : "false"} onClick={onActivate}>
        {leading !== undefined && <span className="mb-row-leading">{leading}</span>}
        <span className="mb-row-text">
          <span className="mb-row-title">{title}</span>
          {subtitle !== undefined && <span className="mb-row-subtitle">{subtitle}</span>}
        </span>
        {trailing !== undefined && <span className="mb-row-trailing">{trailing}</span>}
      </button>
    </li>
  );
}
