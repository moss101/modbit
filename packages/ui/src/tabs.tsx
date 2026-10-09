import { useRef, type KeyboardEvent, type ReactNode } from "react";
import { navTarget, nextEnabled } from "./nav.ts";

export interface TabDef {
  id: string;
  label: string;
  disabled?: boolean | undefined;
  /** A short text count or word after the label. */
  badge?: string | undefined;
}

export interface TabsProps {
  /** The accessible name of the tab list. */
  label: string;
  tabs: readonly TabDef[];
  selected: string;
  onSelect: (id: string) => void;
  /** Prefix for the ids that link each tab to its panel; must be unique on the page. */
  idPrefix: string;
  /** The selected tab's panel content. */
  children: ReactNode;
  testId?: string | undefined;
}

/** Tabs with automatic activation: arrows move and select, Home and End jump, only the selected tab is in the tab order. */
export function Tabs({ label, tabs, selected, onSelect, idPrefix, children, testId }: TabsProps) {
  const refs = useRef<(HTMLButtonElement | null)[]>([]);
  const disabled = tabs.map((t) => t.disabled === true);
  const index = Math.max(0, tabs.findIndex((t) => t.id === selected));
  const onKey = (e: KeyboardEvent<HTMLDivElement>) => {
    const t = navTarget(e.key, index, tabs.length, "horizontal");
    if (t === null) return;
    e.preventDefault();
    const to = e.key === "Home" ? nextEnabled(-1, 1, disabled) : e.key === "End" ? nextEnabled(0, -1, disabled) : nextEnabled(index, e.key === "ArrowRight" ? 1 : -1, disabled);
    const tab = tabs[to];
    if (!tab) return;
    onSelect(tab.id);
    refs.current[to]?.focus();
  };
  return (
    <div className="mb-tabs" data-testid={testId}>
      <div role="tablist" aria-label={label} aria-orientation="horizontal" className="mb-tablist" onKeyDown={onKey}>
        {tabs.map((t, i) => (
          <button
            key={t.id}
            ref={(el) => {
              refs.current[i] = el;
            }}
            type="button"
            role="tab"
            id={`${idPrefix}-tab-${t.id}`}
            aria-selected={t.id === selected}
            aria-controls={`${idPrefix}-panel-${t.id}`}
            tabIndex={t.id === selected ? 0 : -1}
            disabled={t.disabled}
            className="mb-tab"
            data-testid={`tab-${t.id}`}
            onClick={() => onSelect(t.id)}
          >
            {t.label}
            {t.badge !== undefined && <span className="mb-tab-badge"> {t.badge}</span>}
          </button>
        ))}
      </div>
      <div role="tabpanel" id={`${idPrefix}-panel-${selected}`} aria-labelledby={`${idPrefix}-tab-${selected}`} tabIndex={0} className="mb-tabpanel">
        {children}
      </div>
    </div>
  );
}
