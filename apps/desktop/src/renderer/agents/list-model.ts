/**
 * The agent list's pure model (REQ-PX-046; docs/65 AFW-B01..B11): filters,
 * grouping, pins, sections, the flat virtual list and the selection that
 * follows an archive. Nothing here decides a status class. The Core computes
 * the class of each header by its precedence (AFW-B02, MOD-UX-001 attention
 * first) and this module only orders and filters by the class it was given;
 * the single-source test proves a header's class is never re-derived from its
 * other fields.
 */
import type { Status } from "@modbit/ui";
import { STATUS_CLASSES, type AgentHeaderView, type StatusClass } from "../../shared/conversation-types.ts";
import type { ProjectInfo } from "../../shared/project-types.ts";

/** How each class is shown: its own glyph (via the status primitive) and always its words. */
export const CLASS_META: Record<StatusClass, { status: Status; short: string }> = {
  NEEDS_ATTENTION: { status: "warn", short: "Needs attention" },
  FAILED: { status: "danger", short: "Failed" },
  READY_FOR_REVIEW_UNSEEN: { status: "info", short: "Ready for review" },
  READY_FOR_REVIEW_SEEN: { status: "ok", short: "Ready for review (seen)" },
  RUNNING: { status: "running", short: "Running" },
  WAITING: { status: "idle", short: "Waiting" },
  COMPLETED: { status: "ok", short: "Completed" },
  DRAFT: { status: "idle", short: "Draft" },
  ARCHIVED: { status: "idle", short: "Archived" },
};

/** The Core's rank of a class (lower first). Used to order sections only. */
export const classRank = (c: StatusClass): number => STATUS_CLASSES.indexOf(c);

// ------------------------------------------------------------------ filters

/** The chips of AFW-B06 that the header projection can answer. */
export const STATE_FILTERS = [
  { id: "attention", label: "Needs attention" },
  { id: "unread", label: "Unread" },
  { id: "running", label: "Running" },
  { id: "draft", label: "Draft" },
  { id: "done", label: "Done" },
  { id: "failed", label: "Failed" },
] as const;
export type StateFilter = (typeof STATE_FILTERS)[number]["id"];

export const LOCATION_FILTERS = ["local", "cloud"] as const;
export type LocationFilter = (typeof LOCATION_FILTERS)[number];

export const ORIGIN_FILTERS = [
  { id: "desktop", label: "Desktop" },
  { id: "cli", label: "CLI" },
  { id: "ide", label: "IDE" },
  { id: "forge_issue", label: "Forge issue" },
  { id: "forge_webhook", label: "Forge webhook" },
] as const;
export type OriginFilter = (typeof ORIGIN_FILTERS)[number]["id"];

/** The project chip for tasks that are in no project. */
export const NO_PROJECT = "none";

export interface Filters {
  states: StateFilter[];
  locations: LocationFilter[];
  origins: OriginFilter[];
  /** Project ids (the Core's), or NO_PROJECT: the chips of the stored query (AFW-B06, PX-064). */
  projects: string[];
  /** Archived conversations are listed only when asked for. */
  showArchived: boolean;
}

export const NO_FILTERS: Filters = { states: [], locations: [], origins: [], projects: [], showArchived: false };

const DONE_CLASSES: readonly StatusClass[] = ["COMPLETED", "READY_FOR_REVIEW_UNSEEN", "READY_FOR_REVIEW_SEEN"];

function matchesState(h: AgentHeaderView, f: StateFilter): boolean {
  switch (f) {
    case "attention":
      return h.statusClass === "NEEDS_ATTENTION";
    case "unread":
      return h.unread;
    case "running":
      return h.statusClass === "RUNNING";
    case "draft":
      return h.statusClass === "DRAFT";
    case "done":
      return DONE_CLASSES.includes(h.statusClass);
    case "failed":
      return h.statusClass === "FAILED";
  }
}

/** A header passes when it satisfies one chip of every non-empty family (within a family the chips widen). */
export function passes(h: AgentHeaderView, f: Filters): boolean {
  if (h.archived && !f.showArchived) return false;
  if (f.states.length > 0 && !f.states.some((s) => matchesState(h, s))) return false;
  if (f.locations.length > 0 && !f.locations.includes((h.executionLocation || "local") as LocationFilter)) return false;
  if (f.origins.length > 0 && !f.origins.includes(h.origin as OriginFilter)) return false;
  // The project a header is in is the Core's map (AgentHeader.projectId); a chip names a project or "no project".
  if (f.projects.length > 0 && !f.projects.some((id) => (id === NO_PROJECT ? !h.projectId : h.projectId === id))) return false;
  return true;
}

export function filterCount(f: Filters): number {
  return f.states.length + f.locations.length + f.origins.length + f.projects.length + (f.showArchived ? 1 : 0);
}

// ----------------------------------------------------------------- grouping

export const GROUPINGS = [
  { id: "repository", label: "Repository" },
  { id: "workspace", label: "Workspace" },
  { id: "status", label: "Status" },
  { id: "location", label: "Execution location" },
  { id: "time", label: "Time" },
  { id: "project", label: "Project" },
] as const;
export type Grouping = (typeof GROUPINGS)[number]["id"];

/** How rows are ordered inside a section (a part of a stored view). */
export const SORTS = [
  { id: "recent", label: "Newest activity" },
  { id: "status", label: "Status" },
  { id: "title", label: "Title" },
] as const;
export type SortKey = (typeof SORTS)[number]["id"];

export interface Section {
  key: string;
  label: string;
  rows: AgentHeaderView[];
  /** Pinned tasks form the first section (AFW-B07). */
  pinned?: boolean;
}

/** Newest activity first; the id breaks ties so the order is total and stable. */
export const byRecent = (a: AgentHeaderView, b: AgentHeaderView): number => b.updatedAtMs - a.updatedAtMs || (a.taskId < b.taskId ? -1 : 1);

export type TimeBucket = "today" | "yesterday" | "week" | "older";
const BUCKET_LABEL: Record<TimeBucket, string> = { today: "Today", yesterday: "Yesterday", week: "Previous 7 days", older: "Older" };
const BUCKET_ORDER: TimeBucket[] = ["today", "yesterday", "week", "older"];

/** Local calendar buckets; `now` is passed in so the result is deterministic. */
export function timeBucket(atMs: number, nowMs: number): TimeBucket {
  const start = new Date(nowMs);
  start.setHours(0, 0, 0, 0);
  const today = start.getTime();
  if (atMs >= today) return "today";
  if (atMs >= today - 86_400_000) return "yesterday";
  if (atMs >= today - 7 * 86_400_000) return "week";
  return "older";
}

function repoLabel(h: AgentHeaderView): string {
  return h.subtitle || baseName(h.workspaceRoot) || "No repository";
}
export function baseName(p: string): string {
  const parts = p.split(/[\\/]+/).filter(Boolean);
  return parts[parts.length - 1] ?? "";
}

/** Rows inside a section in a stored view's order; the id breaks ties so the order is total. */
export function byOrder(sort: SortKey): (a: AgentHeaderView, b: AgentHeaderView) => number {
  switch (sort) {
    case "status":
      return (a, b) => classRank(a.statusClass) - classRank(b.statusClass) || byRecent(a, b);
    case "title":
      return (a, b) => (a.title || "").localeCompare(b.title || "") || byRecent(a, b);
    default:
      return byRecent;
  }
}

export interface GroupOptions {
  /** The Core's projects (for the Project grouping and for hiding the tasks the Projects group shows). */
  projects?: readonly ProjectInfo[] | undefined;
  sort?: SortKey | undefined;
}

/** The ids of the projects a list can show: live ones, and archived ones only when archived items are asked for. */
export function visibleProjects(projects: readonly ProjectInfo[], showArchived: boolean): ProjectInfo[] {
  return projects.filter((p) => showArchived || !p.archived);
}

/** Splits headers into sections for a grouping; pinned headers (in pin order) form the first section. */
export function groupHeaders(headers: readonly AgentHeaderView[], grouping: Grouping, pins: readonly string[], nowMs: number, opts: GroupOptions = {}): Section[] {
  const byId = new Map(headers.map((h) => [h.taskId, h]));
  const pinnedRows = pins.map((id) => byId.get(id)).filter((h): h is AgentHeaderView => h !== undefined);
  const pinnedIds = new Set(pinnedRows.map((h) => h.taskId));
  const rest = headers.filter((h) => !pinnedIds.has(h.taskId)).sort(byOrder(opts.sort ?? "recent"));
  const projectName = new Map((opts.projects ?? []).map((p) => [p.projectId, p]));
  const sections: Section[] = [];
  if (pinnedRows.length > 0) sections.push({ key: "pinned", label: "Pinned", rows: pinnedRows, pinned: true });

  const bucketed = new Map<string, { label: string; order: number; rows: AgentHeaderView[] }>();
  const put = (key: string, label: string, order: number, h: AgentHeaderView) => {
    const s = bucketed.get(key);
    if (s) s.rows.push(h);
    else bucketed.set(key, { label, order, rows: [h] });
  };
  rest.forEach((h, i) => {
    switch (grouping) {
      case "repository": {
        const label = repoLabel(h);
        // Same-named folders of different parents stay apart; the order is that of first (most recent) appearance.
        put(h.workspaceRoot || "none", label, i, h);
        break;
      }
      case "workspace":
        put(h.workspaceRoot || "none", h.workspaceRoot || "No workspace", i, h);
        break;
      case "status":
        put(h.statusClass, CLASS_META[h.statusClass].short, classRank(h.statusClass), h);
        break;
      case "location":
        put(h.executionLocation || "local", (h.executionLocation || "local") === "cloud" ? "Cloud" : "Local", (h.executionLocation || "local") === "cloud" ? 1 : 0, h);
        break;
      case "time": {
        const b = timeBucket(h.updatedAtMs, nowMs);
        put(b, BUCKET_LABEL[b], BUCKET_ORDER.indexOf(b), h);
        break;
      }
      case "project": {
        // The Core's membership map; a task whose project the list cannot show (archived, hidden) is shown as in no project.
        const p = h.projectId ? projectName.get(h.projectId) : undefined;
        if (p) put(p.projectId, p.name, 1 + [...projectName.keys()].indexOf(p.projectId), h);
        else put("none", "No project", 0, h);
        break;
      }
    }
  });
  if (grouping === "project") {
    // Projects first (in the Core's order, newest first), the tasks of no project last.
    const none = bucketed.get("none");
    if (none) none.order = 10_000;
  }
  if (grouping === "project") {
    // A project with no task passing the filters is still a section (count 0): it is a place a task can be added to.
    for (const [i, p] of [...projectName.values()].entries()) if (!bucketed.has(p.projectId)) bucketed.set(p.projectId, { label: p.name, order: 1 + i, rows: [] });
  }
  const ordered = [...bucketed.entries()].sort((a, b) => a[1].order - b[1].order);
  // Two repositories with one folder name get their paths in the label so the sections are distinguishable.
  if (grouping === "repository") {
    const seen = new Map<string, number>();
    for (const [, s] of ordered) seen.set(s.label, (seen.get(s.label) ?? 0) + 1);
    for (const [key, s] of ordered) if ((seen.get(s.label) ?? 0) > 1) s.label = `${s.label} (${key})`;
  }
  for (const [key, s] of ordered) sections.push({ key: `${grouping}:${key}`, label: s.label, rows: s.rows });
  return sections;
}

// --------------------------------------------------------------------- pins

/** Pinning has a hard cap (AFW-B07). */
export const PIN_CAP = 8;
export const PIN_CAP_MESSAGE = `You can pin up to ${PIN_CAP} tasks. Unpin one to pin another.`;

export type PinResult = { pins: string[]; message: string | null };

/** Pins a task; at the cap nothing changes and the message says why. */
export function pin(pins: readonly string[], taskId: string): PinResult {
  if (pins.includes(taskId)) return { pins: [...pins], message: null };
  if (pins.length >= PIN_CAP) return { pins: [...pins], message: PIN_CAP_MESSAGE };
  return { pins: [...pins, taskId], message: null };
}
export const unpin = (pins: readonly string[], taskId: string): string[] => pins.filter((p) => p !== taskId);

// ------------------------------------------------------------- virtual list

export const SECTION_HEIGHT = 28;
export const ROW_HEIGHT = 56;

export type ListItem =
  | { kind: "section"; key: string; label: string; count: number; collapsed: boolean; pinned: boolean }
  | { kind: "row"; key: string; header: AgentHeaderView; sectionKey: string }
  /** The Projects group's heading (PX-064): separate from the repositories. */
  | { kind: "projects-head"; key: string; label: string; count: number; collapsed: boolean }
  /** A project parent: its facts are the Core's record and rollup; `shown` is how many of its tasks pass the filters. */
  | { kind: "project"; key: string; project: ProjectInfo; shown: number; collapsed: boolean }
  /** The "New project" row that closes the Projects group. */
  | { kind: "new-project"; key: string };

/** The flat, ordered items of the list: a section header, then its rows unless the section is collapsed. */
export function flatten(sections: readonly Section[], collapsed: ReadonlySet<string>): ListItem[] {
  const out: ListItem[] = [];
  for (const s of sections) {
    const isCollapsed = collapsed.has(s.key);
    out.push({ kind: "section", key: `s:${s.key}`, label: s.label, count: s.rows.length, collapsed: isCollapsed, pinned: s.pinned === true });
    if (!isCollapsed) for (const h of s.rows) out.push({ kind: "row", key: `r:${s.key}:${h.taskId}`, header: h, sectionKey: s.key });
  }
  return out;
}

export interface Layout {
  offsets: number[];
  total: number;
}

/** Height of a project parent row and of the "New project" row. */
export const PROJECT_HEIGHT = 40;
export const NEW_PROJECT_HEIGHT = 32;

export const heightOf = (item: ListItem): number => {
  switch (item.kind) {
    case "section":
    case "projects-head":
      return SECTION_HEIGHT;
    case "project":
      return PROJECT_HEIGHT;
    case "new-project":
      return NEW_PROJECT_HEIGHT;
    default:
      return ROW_HEIGHT;
  }
};

export function layoutOf(items: readonly ListItem[]): Layout {
  const offsets = new Array<number>(items.length);
  let y = 0;
  for (let i = 0; i < items.length; i++) {
    offsets[i] = y;
    y += heightOf(items[i]!);
  }
  return { offsets, total: y };
}

/** The slice of items to render for a scroll position: those intersecting the viewport, plus an overscan on both sides. */
export function visibleRange(layout: Layout, count: number, scrollTop: number, viewport: number, overscan = 6): { start: number; end: number } {
  if (count === 0) return { start: 0, end: 0 };
  // The last item whose top is at or above scrollTop.
  let lo = 0;
  let hi = count - 1;
  while (lo < hi) {
    const mid = (lo + hi + 1) >> 1;
    if (layout.offsets[mid]! <= scrollTop) lo = mid;
    else hi = mid - 1;
  }
  let end = lo;
  while (end < count && layout.offsets[end]! < scrollTop + viewport) end++;
  return { start: Math.max(0, lo - overscan), end: Math.min(count, end + overscan) };
}

// ---------------------------------------------------------------- selection

/** The task the selection moves to when `taskId` leaves the list: the next row, else the previous one (AFW-B08). */
export function nearestSibling(orderedIds: readonly string[], taskId: string): string | null {
  const i = orderedIds.indexOf(taskId);
  if (i < 0) return null;
  return orderedIds[i + 1] ?? orderedIds[i - 1] ?? null;
}

/** Row order as the person sees it (collapsed sections hide their rows). */
export const visibleTaskIds = (items: readonly ListItem[]): string[] => items.flatMap((i) => (i.kind === "row" ? [i.header.taskId] : []));

// ---------------------------------------------------------------------- age

export function relativeAge(thenMs: number, nowMs: number): string {
  if (!thenMs) return "";
  const s = Math.max(0, Math.floor((nowMs - thenMs) / 1000));
  if (s < 45) return "now";
  const m = Math.floor(s / 60);
  if (m < 60) return `${Math.max(1, m)}m`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h}h`;
  const d = Math.floor(h / 24);
  if (d < 14) return `${d}d`;
  const w = Math.floor(d / 7);
  if (w < 9) return `${w}w`;
  return `${Math.floor(d / 30)}mo`;
}

// --------------------------------------------------------------- row facts

/** The facts shown on a row's second line, each individually toggleable (AFW-B06). */
export const SUBTITLE_FIELDS = [
  { id: "repository", label: "Repository" },
  { id: "location", label: "Location" },
  { id: "changes", label: "Changes" },
  { id: "origin", label: "Origin" },
] as const;
export type SubtitleField = (typeof SUBTITLE_FIELDS)[number]["id"];

export function subtitleOf(h: AgentHeaderView, fields: readonly SubtitleField[]): string {
  const parts: string[] = [];
  if (fields.includes("repository") && h.subtitle) parts.push(h.subtitle);
  if (fields.includes("location") && h.executionLocation === "cloud") parts.push("cloud");
  if (fields.includes("changes") && h.filesChanged > 0) parts.push(`${h.filesChanged} ${h.filesChanged === 1 ? "file" : "files"} +${h.linesAdded} −${h.linesRemoved}`);
  if (fields.includes("origin") && h.origin && h.origin !== "desktop") parts.push(h.origin.replace(/_/g, " "));
  return parts.join(" · ");
}

/** Accessible name of a row: title, status words, unread, attention. */
export function rowLabel(h: AgentHeaderView, pinned: boolean): string {
  return [h.title || "Untitled task", h.statusLabel || CLASS_META[h.statusClass].short, h.unread ? "unread" : "", h.pendingApproval ? "waiting on your approval" : "", pinned ? "pinned" : ""].filter(Boolean).join(", ");
}

// -------------------------------------------------------- the projects group

export interface ItemsInput {
  /** The headers that pass the filters. */
  headers: readonly AgentHeaderView[];
  /** The Core's projects (ListProjects), archived ones included; the list decides which to show. */
  projects: readonly ProjectInfo[];
  grouping: Grouping;
  pins: readonly string[];
  collapsed: ReadonlySet<string>;
  nowMs: number;
  sort: SortKey;
  showArchived: boolean;
}

/** The section key of the Projects group and of one project (collapsed state persists per grouping). */
export const PROJECTS_KEY = "projects";
export const projectKey = (projectId: string): string => `project:${projectId}`;

/**
 * The flat items of the list. With a project to show, the Projects group sits
 * after the pinned tasks and before the repositories: each project is an
 * expandable parent showing the tasks the Core's membership map puts in it, and
 * those tasks do not appear a second time below. A project the list does not
 * show (archived and not asked for) is not a parent, and its tasks are listed
 * where they would be otherwise: archiving a project hides the grouping, never
 * the work. Under the Project grouping the projects are the sections.
 */
export function listItems(i: ItemsInput): ListItem[] {
  const shown = visibleProjects(i.projects, i.showArchived);
  if (i.grouping === "project") return flatten(groupHeaders(i.headers, "project", i.pins, i.nowMs, { projects: shown, sort: i.sort }), i.collapsed);
  const shownIds = new Set(shown.map((p) => p.projectId));
  const pinned = new Set(i.pins);
  const inShownProject = (h: AgentHeaderView) => h.projectId !== undefined && h.projectId !== "" && shownIds.has(h.projectId) && !pinned.has(h.taskId);
  const members = i.headers.filter(inShownProject);
  const sections = groupHeaders(i.headers.filter((h) => !inShownProject(h)), i.grouping, i.pins, i.nowMs, { sort: i.sort });
  const out: ListItem[] = [];
  const first = sections[0];
  if (first?.pinned) out.push(...flatten([sections.shift()!], i.collapsed));
  if (shown.length > 0) {
    const groupCollapsed = i.collapsed.has(PROJECTS_KEY);
    out.push({ kind: "projects-head", key: `s:${PROJECTS_KEY}`, label: "Projects", count: shown.length, collapsed: groupCollapsed });
    if (!groupCollapsed) {
      for (const p of shown) {
        const rows = members.filter((h) => h.projectId === p.projectId).sort(byOrder(i.sort));
        const pc = i.collapsed.has(projectKey(p.projectId));
        out.push({ kind: "project", key: `p:${p.projectId}`, project: p, shown: rows.length, collapsed: pc });
        if (!pc) for (const h of rows) out.push({ kind: "row", key: `r:${projectKey(p.projectId)}:${h.taskId}`, header: h, sectionKey: projectKey(p.projectId) });
      }
      out.push({ kind: "new-project", key: "np" });
    }
  }
  out.push(...flatten(sections, i.collapsed));
  return out;
}

// ------------------------------------------------------------ stored views

/**
 * A stored view is a saved query: the grouping, the filter chips and the
 * sort of the list (AFW-B06). It is a per-viewer preference kept in the
 * renderer's own storage like the other list preferences: a convenience that
 * decides nothing about a task, never authoritative, and it names projects by
 * the Core's ids (a view whose project is gone simply matches nothing for that
 * chip).
 */
export interface StoredView {
  id: string;
  name: string;
  grouping: Grouping;
  filters: Filters;
  sort: SortKey;
}

export const MAX_VIEWS = 20;
export const MAX_VIEW_NAME = 40;

const sameSet = (a: readonly string[], b: readonly string[]): boolean => a.length === b.length && a.every((x) => b.includes(x));

export function sameFilters(a: Filters, b: Filters): boolean {
  return sameSet(a.states, b.states) && sameSet(a.locations, b.locations) && sameSet(a.origins, b.origins) && sameSet(a.projects, b.projects) && a.showArchived === b.showArchived;
}

/** Whether the list is showing exactly what a view stores. */
export function viewMatches(v: StoredView, now: { grouping: Grouping; filters: Filters; sort: SortKey }): boolean {
  return v.grouping === now.grouping && v.sort === now.sort && sameFilters(v.filters, now.filters);
}

export type SaveViewResult = { ok: true; views: StoredView[]; view: StoredView } | { ok: false; message: string };

/** Saves the current query under a name: names are unique (any case) and bounded; saving a name again replaces that view. */
export function saveView(views: readonly StoredView[], name: string, now: { grouping: Grouping; filters: Filters; sort: SortKey }, newId: () => string): SaveViewResult {
  const trimmed = name.trim();
  if (trimmed.length === 0) return { ok: false, message: "A view needs a name." };
  if (trimmed.length > MAX_VIEW_NAME) return { ok: false, message: `A view name has at most ${MAX_VIEW_NAME} characters.` };
  const key = trimmed.toLowerCase();
  const existing = views.find((v) => v.name.toLowerCase() === key);
  if (!existing && views.length >= MAX_VIEWS) return { ok: false, message: `You can keep up to ${MAX_VIEWS} views. Delete one to save another.` };
  const view: StoredView = { id: existing?.id ?? newId(), name: existing?.name ?? trimmed, grouping: now.grouping, filters: { ...now.filters, states: [...now.filters.states], locations: [...now.filters.locations], origins: [...now.filters.origins], projects: [...now.filters.projects] }, sort: now.sort };
  return { ok: true, views: existing ? views.map((v) => (v.id === existing.id ? view : v)) : [...views, view], view };
}

export const deleteView = (views: readonly StoredView[], id: string): StoredView[] => views.filter((v) => v.id !== id);
