/**
 * The agent list's per-viewer preferences (AFW-B05..B07): grouping, filter
 * chips, pins, collapsed sections per grouping and the row-subtitle fields.
 * Renderer-local like the shell's other preferences: never task state, never
 * authoritative (the Core has no pin command in PX-042, so pins are a
 * convenience of this viewer and the cap is enforced here). Everything read
 * back is validated; a missing or hostile entry falls back to the default.
 */
import { GROUPINGS, LOCATION_FILTERS, NO_FILTERS, ORIGIN_FILTERS, PIN_CAP, STATE_FILTERS, SUBTITLE_FIELDS, type Filters, type Grouping, type SubtitleField } from "./list-model.ts";

export interface ListPrefs {
  grouping: Grouping;
  filters: Filters;
  pins: string[];
  /** Collapsed section keys, kept per grouping. */
  collapsed: Record<string, string[]>;
  subtitle: SubtitleField[];
}

export const LIST_PREFS_KEY = "modbit.agents.v1";
export const DEFAULT_LIST_PREFS: ListPrefs = { grouping: "repository", filters: NO_FILTERS, pins: [], collapsed: {}, subtitle: ["repository", "changes", "location", "origin"] };

const isObject = (v: unknown): v is Record<string, unknown> => typeof v === "object" && v !== null && !Array.isArray(v);
const strings = (v: unknown, max: number): string[] => (Array.isArray(v) ? v.filter((x): x is string => typeof x === "string" && x.length > 0 && x.length < 512).slice(0, max) : []);
const pick = <T extends string>(v: unknown, allowed: readonly T[]): T[] => strings(v, 32).filter((s): s is T => (allowed as readonly string[]).includes(s));

export function parseListPrefs(raw: string | null): ListPrefs {
  if (!raw) return DEFAULT_LIST_PREFS;
  let v: unknown;
  try {
    v = JSON.parse(raw);
  } catch {
    return DEFAULT_LIST_PREFS;
  }
  if (!isObject(v)) return DEFAULT_LIST_PREFS;
  const grouping = GROUPINGS.find((g) => g.id === v.grouping)?.id ?? DEFAULT_LIST_PREFS.grouping;
  const f = isObject(v.filters) ? v.filters : {};
  const filters: Filters = {
    states: pick(f.states, STATE_FILTERS.map((s) => s.id)),
    locations: pick(f.locations, LOCATION_FILTERS),
    origins: pick(f.origins, ORIGIN_FILTERS.map((o) => o.id)),
    showArchived: f.showArchived === true,
  };
  const collapsed: Record<string, string[]> = {};
  if (isObject(v.collapsed)) for (const [g, keys] of Object.entries(v.collapsed).slice(0, GROUPINGS.length)) if (GROUPINGS.some((x) => x.id === g)) collapsed[g] = strings(keys, 200);
  const subtitle = Array.isArray(v.subtitle) ? pick(v.subtitle, SUBTITLE_FIELDS.map((s) => s.id)) : DEFAULT_LIST_PREFS.subtitle;
  // A stored list longer than the cap (an older cap, a hand edit) is cut, never trusted.
  const pins = [...new Set(strings(v.pins, PIN_CAP))];
  return { grouping, filters, pins, collapsed, subtitle };
}

export function serializeListPrefs(p: ListPrefs): string {
  return JSON.stringify(p);
}

export function loadListPrefs(): ListPrefs {
  try {
    return parseListPrefs(localStorage.getItem(LIST_PREFS_KEY));
  } catch {
    return DEFAULT_LIST_PREFS;
  }
}

export function saveListPrefs(p: ListPrefs): void {
  try {
    localStorage.setItem(LIST_PREFS_KEY, serializeListPrefs(p));
  } catch {
    // a per-viewer convenience: the list works without it
  }
}
