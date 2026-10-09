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
  { id: "automation", label: "Automation" },
] as const;
export type OriginFilter = (typeof ORIGIN_FILTERS)[number]["id"];

export interface Filters {
  states: StateFilter[];
  locations: LocationFilter[];
  origins: OriginFilter[];
  /** Archived conversations are listed only when asked for. */
  showArchived: boolean;
}

export const NO_FILTERS: Filters = { states: [], locations: [], origins: [], showArchived: false };

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
  return true;
}

export function filterCount(f: Filters): number {
  return f.states.length + f.locations.length + f.origins.length + (f.showArchived ? 1 : 0);
}

// ----------------------------------------------------------------- grouping

export const GROUPINGS = [
  { id: "repository", label: "Repository" },
  { id: "workspace", label: "Workspace" },
  { id: "status", label: "Status" },
  { id: "location", label: "Execution location" },
  { id: "time", label: "Time" },
] as const;
export type Grouping = (typeof GROUPINGS)[number]["id"];

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

/** Splits headers into sections for a grouping; pinned headers (in pin order) form the first section. */
export function groupHeaders(headers: readonly AgentHeaderView[], grouping: Grouping, pins: readonly string[], nowMs: number): Section[] {
  const byId = new Map(headers.map((h) => [h.taskId, h]));
  const pinnedRows = pins.map((id) => byId.get(id)).filter((h): h is AgentHeaderView => h !== undefined);
  const pinnedIds = new Set(pinnedRows.map((h) => h.taskId));
  const rest = headers.filter((h) => !pinnedIds.has(h.taskId)).sort(byRecent);
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
    }
  });
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

export type ListItem = { kind: "section"; key: string; label: string; count: number; collapsed: boolean; pinned: boolean } | { kind: "row"; key: string; header: AgentHeaderView; sectionKey: string };

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

export const heightOf = (item: ListItem): number => (item.kind === "section" ? SECTION_HEIGHT : ROW_HEIGHT);

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
  return [h.title || "Untitled task", h.statusLabel || CLASS_META[h.statusClass].short, h.origin === "automation" ? "started by an automation" : "", h.unread ? "unread" : "", h.pendingApproval ? "waiting on your approval" : "", pinned ? "pinned" : ""].filter(Boolean).join(", ");
}
