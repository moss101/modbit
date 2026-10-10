/**
 * The agent list's per-viewer preferences (AFW-B05..B07): grouping, filter
 * chips, pins, collapsed sections per grouping and the row-subtitle fields.
 * Renderer-local like the shell's other preferences: never task state, never
 * authoritative (the Core has no pin command in PX-042, so pins are a
 * convenience of this viewer and the cap is enforced here). Everything read
 * back is validated; a missing or hostile entry falls back to the default.
 */
import { GROUPINGS, LOCATION_FILTERS, MAX_VIEWS, MAX_VIEW_NAME, NO_FILTERS, ORIGIN_FILTERS, PIN_CAP, SORTS, STATE_FILTERS, SUBTITLE_FIELDS, type Filters, type Grouping, type SortKey, type StoredView, type SubtitleField } from "./list-model.ts";

export interface ListPrefs {
  grouping: Grouping;
  filters: Filters;
  pins: string[];
  /** Collapsed section keys, kept per grouping. */
  collapsed: Record<string, string[]>;
  subtitle: SubtitleField[];
  /** How rows are ordered inside a section (PX-064). */
  sort: SortKey;
  /** The stored views: saved queries of this viewer (grouping, filter chips, sort). */
  views: StoredView[];
}

export const LIST_PREFS_KEY = "modbit.agents.v1";
export const DEFAULT_LIST_PREFS: ListPrefs = { grouping: "repository", filters: NO_FILTERS, pins: [], collapsed: {}, subtitle: ["repository", "changes", "location", "origin"], sort: "recent", views: [] };

const isObject = (v: unknown): v is Record<string, unknown> => typeof v === "object" && v !== null && !Array.isArray(v);
const strings = (v: unknown, max: number): string[] => (Array.isArray(v) ? v.filter((x): x is string => typeof x === "string" && x.length > 0 && x.length < 512).slice(0, max) : []);
const pick = <T extends string>(v: unknown, allowed: readonly T[]): T[] => strings(v, 32).filter((s): s is T => (allowed as readonly string[]).includes(s));

/** Filters as stored, validated: unknown chips are dropped; project ids are the Core's and are kept as text. */
function parseFilters(v: unknown): Filters {
  const f = isObject(v) ? v : {};
  return {
    states: pick(f.states, STATE_FILTERS.map((s) => s.id)),
    locations: pick(f.locations, LOCATION_FILTERS),
    origins: pick(f.origins, ORIGIN_FILTERS.map((o) => o.id)),
    projects: [...new Set(strings(f.projects, 32))],
    showArchived: f.showArchived === true,
  };
}

function parseViews(v: unknown): StoredView[] {
  if (!Array.isArray(v)) return [];
  const out: StoredView[] = [];
  for (const x of v) {
    if (!isObject(x) || out.length >= MAX_VIEWS) continue;
    const name = typeof x.name === "string" ? x.name.trim() : "";
    const id = typeof x.id === "string" && x.id.length > 0 && x.id.length < 64 ? x.id : "";
    const grouping = GROUPINGS.find((g) => g.id === x.grouping)?.id;
    const sort = SORTS.find((s) => s.id === x.sort)?.id;
    if (!name || name.length > MAX_VIEW_NAME || !id || !grouping || !sort) continue;
    if (out.some((o) => o.id === id || o.name.toLowerCase() === name.toLowerCase())) continue;
    out.push({ id, name, grouping, sort, filters: parseFilters(x.filters) });
  }
  return out;
}

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
  const filters = parseFilters(v.filters);
  const collapsed: Record<string, string[]> = {};
  if (isObject(v.collapsed)) for (const [g, keys] of Object.entries(v.collapsed).slice(0, GROUPINGS.length)) if (GROUPINGS.some((x) => x.id === g)) collapsed[g] = strings(keys, 200);
  const subtitle = Array.isArray(v.subtitle) ? pick(v.subtitle, SUBTITLE_FIELDS.map((s) => s.id)) : DEFAULT_LIST_PREFS.subtitle;
  // A stored list longer than the cap (an older cap, a hand edit) is cut, never trusted.
  const pins = [...new Set(strings(v.pins, PIN_CAP))];
  const sort = SORTS.find((s) => s.id === v.sort)?.id ?? DEFAULT_LIST_PREFS.sort;
  return { grouping, filters, pins, collapsed, subtitle, sort, views: parseViews(v.views) };
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
