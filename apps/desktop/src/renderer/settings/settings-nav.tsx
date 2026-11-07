import type { ReactNode } from "react";
import { Icon } from "@modbit/ui";
import { SETTINGS_ENTRIES, settingsMatches } from "./settings-page.tsx";

export interface SettingsNavProps {
  query: string;
  onQuery: (q: string) => void;
  active: string | null;
  onPick: (id: string) => void;
  onBack: () => void;
  /** The person's row, the same as under the agent list. */
  foot: ReactNode;
}

/** AFW-A10: while Settings is open the left column is its navigation: Back, a search field, grouped entries, the pinned footer. */
export function SettingsNav({ query, onQuery, active, onPick, onBack, foot }: SettingsNavProps) {
  const shown = SETTINGS_ENTRIES.filter((e) => settingsMatches(e, query));
  const groups = [...new Set(shown.map((e) => e.group))];
  return (
    <div className="agent-region settings-nav" data-testid="settings-nav">
      <div className="agent-region-head settings-nav-head">
        <button type="button" className="agent-nav-row settings-back" onClick={onBack} data-testid="nav-back">
          <Icon name="arrow-left" size={14} />
          <span className="agent-nav-label">Back</span>
        </button>
      </div>
      <div className="settings-nav-search">
        <input type="search" aria-label="Search settings" placeholder="Search settings" value={query} onChange={(e) => onQuery(e.target.value)} data-testid="settings-search" autoComplete="off" spellCheck={false} />
      </div>
      <nav className="settings-nav-list" aria-label="Settings sections">
        {groups.map((g) => (
          <div key={g} className="settings-nav-group">
            <h2 className="agents-section-label agent-repos-head">{g}</h2>
            <ul className="agent-nav">
              {shown
                .filter((e) => e.group === g)
                .map((e) => (
                  <li key={e.id}>
                    <button type="button" className="agent-nav-row" aria-current={active === e.id ? "true" : undefined} data-testid={`settings-nav-${e.id}`} onClick={() => onPick(e.id)}>
                      <Icon name={e.icon} size={14} />
                      <span className="agent-nav-label">{e.label}</span>
                    </button>
                  </li>
                ))}
            </ul>
          </div>
        ))}
      </nav>
      {foot}
    </div>
  );
}
