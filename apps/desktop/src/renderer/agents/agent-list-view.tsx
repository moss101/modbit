/**
 * The task list, presentational (REQ-PX-046, DR-PX-2026-11-11-021 MWS-04): headers in, callbacks out. The connected list and
 * the state gallery both render it, so the gallery shows the real component with fixture headers. Top to bottom: the Tasks head
 * (search, list options, more), the state views, the groups. A row is one line: the title, then the state's glyph, the files a
 * task ready for review changed and the unread dot; a row whose task needs the person or failed keeps a second line with the
 * Core's reason. It shows the status class it is given (glyph and words, never colour alone), child tasks under their parent,
 * pin and archive on hover and on focus, a preview card on rest and on Space, and it windows its rows: only those in the
 * viewport (plus an overscan) are in the document, so a thousand tasks cost the same as twenty.
 */
import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, type CSSProperties, type KeyboardEvent, type ReactNode } from "react";
import { Badge, Button, Icon, IconButton, Menu, StatusDot, type MenuItem } from "@modbit/ui";
import { densityMetrics, type Density } from "@modbit/design-tokens";
import type { AgentHeaderView, ConversationSearchView } from "../../shared/conversation-types.ts";
import type { ProjectInfo } from "../../shared/project-types.ts";
import { CLASS_META, FACT_FIELDS, GROUPINGS, LOCATION_FILTERS, NO_FILTERS, NO_PROJECT, ORIGIN_FILTERS, ROW_DETAILS, SORTS, STATE_FILTERS, STATE_VIEWS, SUBTITLE_FIELDS, changedFilesShown, filterCount, heightOf, layoutOf, listItems, passes, projectKey, rowLabel, rowLine, rowTitle, toggleView, viewCounts, viewMatches, visibleProjects, visibleRange, type FactField, type Filters, type Grouping, type ListItem, type ListMetrics, type RowDetails, type RowFacts, type SortKey, type StoredView, type SubtitleField } from "./list-model.ts";
import { ProjectGlyph } from "../projects/project-glyph.tsx";
import { projectMenuFor, rollupWords } from "../projects/project-model.ts";
import { highlightRuns } from "./snippets.ts";
import { localMachine } from "../location.ts";
import { IconArchive, IconFilter, IconPin, IconProject, IconRename, IconRestore, IconViews } from "./icons.tsx";
import { PreviewCard } from "./preview-card.tsx";

/** What the list can do with projects (PX-064). Every call is a Core command made by the connected list; the view only reports the person's intent. */
export interface ProjectActions {
  selectedId: string | null;
  onNew: () => void;
  onOpen: (projectId: string) => void;
  onRename: (projectId: string) => void;
  onArchive: (projectId: string) => void;
  onRestore: (projectId: string) => void;
  onAdd: (taskId: string, projectId: string) => void;
  onRemove: (taskId: string, projectId: string) => void;
}

/** The stored views of this viewer (PX-064). */
export interface ViewActions {
  views: readonly StoredView[];
  onApply: (v: StoredView) => void;
  onSaveRequest: () => void;
  onDelete: (id: string) => void;
}

const TASK_DRAG = "application/x-modbit-task";

export interface AgentListViewProps {
  headers: readonly AgentHeaderView[];
  loaded: boolean;
  error: string | null;
  selectedId: string | null;
  pins: readonly string[];
  grouping: Grouping;
  filters: Filters;
  subtitleFields: readonly SubtitleField[];
  /** What each task is doing and needs; a view with no cards passes nothing and the row shows its header's facts. */
  facts?: ReadonlyMap<string, RowFacts> | undefined;
  factFields?: readonly FactField[] | undefined;
  onFactFields?: ((f: FactField[]) => void) | undefined;
  collapsed: ReadonlySet<string>;
  nowMs: number;
  query: string;
  search: { state: "idle" | "searching" | "done" | "error"; results: ConversationSearchView | null; error: string | null };
  note: string | null;
  onQuery: (q: string) => void;
  onSelect: (taskId: string, rowId?: string) => void;
  onPin: (taskId: string) => void;
  onUnpin: (taskId: string) => void;
  onArchive: (taskId: string) => void;
  onToggleSection: (key: string) => void;
  onGrouping: (g: Grouping) => void;
  onFilters: (f: Filters) => void;
  onSubtitleFields: (f: SubtitleField[]) => void;
  /** The Core's projects, archived ones included; absent in views that have none. */
  projects?: readonly ProjectInfo[] | undefined;
  projectActions?: ProjectActions | undefined;
  sort?: SortKey | undefined;
  onSort?: ((s: SortKey) => void) | undefined;
  viewActions?: ViewActions | undefined;
  /** The row to focus after the list changes (the nearest sibling of an archived row). */
  focusRequest?: { taskId: string; n: number } | null | undefined;
  /** When a row keeps its second line (MWS-04); when needed by default. */
  rowDetails?: RowDetails | undefined;
  onRowDetails?: ((d: RowDetails) => void) | undefined;
  /** Child task to parent task, from the session model: a child is listed, indented, under its parent. */
  parents?: ReadonlyMap<string, string> | undefined;
  /** The shell's density (MWS-03): the heights the rows are laid out with are its tokens'. */
  density?: Density | undefined;
  /** Opens the search over tasks, commands and settings (the command palette). */
  onSearch?: (() => void) | undefined;
  /** Starts a new task in a group's folder (a repository, a project's workspace). */
  onNewTaskIn?: ((root: string) => void) | undefined;
}

/** The pointer rests on a row this long before its preview card opens, and the card stays this long after the pointer leaves it. */
export const PREVIEW_REST_MS = 600;
const PREVIEW_LEAVE_MS = 160;

const toggle = <T,>(xs: readonly T[], x: T): T[] => (xs.includes(x) ? xs.filter((y) => y !== x) : [...xs, x]);
const EMPTY_PROJECTS: readonly ProjectInfo[] = [];
const ALL_FACTS: readonly FactField[] = FACT_FIELDS.map((x) => x.id);

export function AgentListView(p: AgentListViewProps) {
  const [panelOpen, setPanelOpen] = useState(false);
  // Search, grouping, filters and views stay out of the way until asked for, except while one of them is narrowing the list.
  const [toolsOpen, setToolsOpen] = useState(false);
  const scroller = useRef<HTMLDivElement>(null);
  const [scrollTop, setScrollTop] = useState(0);
  const [viewport, setViewport] = useState(480);
  const [activeId, setActiveId] = useState<string | null>(null);
  const pendingFocus = useRef<string | null>(null);
  const [preview, setPreview] = useState<{ taskId: string; by: "pointer" | "key" } | null>(null);
  const restTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const leaveTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const pinned = useMemo(() => new Set(p.pins), [p.pins]);
  const f = filterCount(p.filters);
  const searching = p.query.trim().length > 0;
  // A state view is its own control, so choosing one does not unfold the search and the chips.
  const narrowed = f - p.filters.states.length > 0;
  const counts = useMemo(() => viewCounts(p.headers), [p.headers]);
  const views = STATE_VIEWS.filter((v) => (counts.get(v.id) ?? 0) > 0 || p.filters.states.includes(v.id));
  const factFields = p.factFields ?? ALL_FACTS;
  const rowDetails = p.rowDetails ?? "auto";
  const metrics: ListMetrics = useMemo(() => densityMetrics(p.density ?? "comfortable"), [p.density]);

  const visible = useMemo(() => p.headers.filter((h) => passes(h, p.filters)), [p.headers, p.filters]);
  const projects = p.projects ?? EMPTY_PROJECTS;
  const sort = p.sort ?? "recent";
  // The layout needs one thing of the tasks' facts: which tasks have a reason (they keep a second line). A turn's events change
  // the facts constantly; the list is laid out again only when that set changes.
  const reasons = useMemo(() => [...(p.facts ?? [])].flatMap(([id, x]) => (x.next ? [id] : [])).sort().join("|"), [p.facts]);
  const items = useMemo(
    () => listItems({ headers: visible, projects, grouping: p.grouping, pins: p.pins, collapsed: p.collapsed, nowMs: p.nowMs, sort, showArchived: p.filters.showArchived, details: rowDetails, parents: p.parents, facts: p.facts }),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [visible, projects, p.grouping, p.pins, p.collapsed, p.nowMs, sort, p.filters.showArchived, rowDetails, p.parents, reasons],
  );
  const projectChoices = useMemo(() => visibleProjects(projects, p.filters.showArchived), [projects, p.filters.showArchived]);
  const now = { grouping: p.grouping, filters: p.filters, sort };
  const layout = useMemo(() => layoutOf(items, metrics), [items, metrics]);
  const range = visibleRange(layout, items.length, scrollTop, viewport);
  const titles = useMemo(() => new Map(p.headers.map((h) => [h.taskId, h.title || "Untitled task"])), [p.headers]);

  // The scroller's height follows the region (resizing the window or the splitter).
  useLayoutEffect(() => {
    const el = scroller.current;
    if (!el) return;
    const measure = () => setViewport(el.clientHeight || 480);
    measure();
    if (typeof ResizeObserver === "undefined") return;
    const ro = new ResizeObserver(measure);
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const rowIds = useMemo(() => items.flatMap((i, idx) => (i.kind === "row" ? [{ id: i.header.taskId, idx }] : [])), [items]);

  // Roving focus: the active row is the selected one, else the first.
  const tabStop = activeId && rowIds.some((r) => r.id === activeId) ? activeId : p.selectedId && rowIds.some((r) => r.id === p.selectedId) ? p.selectedId : (rowIds[0]?.id ?? null);

  const reveal = useCallback(
    (taskId: string) => {
      const target = rowIds.find((r) => r.id === taskId);
      const el = scroller.current;
      if (!target || !el) return;
      const top = layout.offsets[target.idx]!;
      const height = heightOf(items[target.idx]!, metrics);
      if (top < el.scrollTop) el.scrollTop = top;
      else if (top + height > el.scrollTop + el.clientHeight) el.scrollTop = top + height - el.clientHeight;
      setScrollTop(el.scrollTop);
    },
    [rowIds, layout, items, metrics],
  );

  const focusRow = useCallback(
    (taskId: string) => {
      pendingFocus.current = taskId;
      setActiveId(taskId);
      reveal(taskId);
    },
    [reveal],
  );

  // Focus lands once the row is in the window (a row outside it is not in the document yet).
  useEffect(() => {
    const id = pendingFocus.current;
    if (!id) return;
    const el = scroller.current?.querySelector<HTMLElement>(`[data-row-id="${id}"]`);
    if (el) {
      pendingFocus.current = null;
      el.focus({ preventScroll: true });
    }
  });

  const requestN = p.focusRequest?.n;
  useEffect(() => {
    if (p.focusRequest) focusRow(p.focusRequest.taskId);
    // Only a new request (its counter) moves focus.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [requestN]);

  // ---- the preview card: it opens when the pointer rests on a row and on Space, and reads nothing the list does not already hold.
  const clearTimer = (t: { current: ReturnType<typeof setTimeout> | null }) => {
    if (t.current) clearTimeout(t.current);
    t.current = null;
  };
  useEffect(
    () => () => {
      clearTimer(restTimer);
      clearTimer(leaveTimer);
    },
    [],
  );
  const closePreview = useCallback((restoreFocusTo?: string) => {
    clearTimer(restTimer);
    clearTimer(leaveTimer);
    setPreview(null);
    if (restoreFocusTo) scroller.current?.querySelector<HTMLElement>(`[data-row-id="${restoreFocusTo}"]`)?.focus({ preventScroll: true });
  }, []);
  const onRowEnter = (taskId: string) => {
    clearTimer(leaveTimer);
    clearTimer(restTimer);
    if (preview?.taskId === taskId) return;
    restTimer.current = setTimeout(() => setPreview({ taskId, by: "pointer" }), PREVIEW_REST_MS);
  };
  const leavePointer = () => {
    clearTimer(restTimer);
    clearTimer(leaveTimer);
    leaveTimer.current = setTimeout(() => setPreview((v) => (v?.by === "pointer" ? null : v)), PREVIEW_LEAVE_MS);
  };
  const previewed = preview ? p.headers.find((h) => h.taskId === preview.taskId) : undefined;
  // A card whose row left the list (archived, filtered out, scrolled out of the window) closes with it.
  const previewInWindow = preview !== null && items.slice(range.start, range.end).some((i) => i.kind === "row" && i.header.taskId === preview.taskId);
  useEffect(() => {
    if (preview && (!previewed || !previewInWindow || searching)) setPreview(null);
  }, [preview, previewed, previewInWindow, searching]);

  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    const target = e.target as HTMLElement;
    const rowId = target.getAttribute("data-row-id");
    if (rowId === null) return;
    const at = rowIds.findIndex((r) => r.id === rowId);
    const move = (to: number) => {
      const r = rowIds[Math.max(0, Math.min(rowIds.length - 1, to))];
      if (r) focusRow(r.id);
      e.preventDefault();
      e.stopPropagation();
    };
    const page = Math.max(1, Math.floor(viewport / (metrics.row + metrics.rowGap)) - 1);
    switch (e.key) {
      case "ArrowDown":
        return move(at + 1);
      case "ArrowUp":
        return move(at - 1);
      case "Home":
        return move(0);
      case "End":
        return move(rowIds.length - 1);
      case "PageDown":
        return move(at + page);
      case "PageUp":
        return move(at - page);
      case " ":
        // Space previews the row; Enter (the button's own key) opens the task.
        if (e.ctrlKey || e.metaKey || e.altKey) return;
        clearTimer(restTimer);
        clearTimer(leaveTimer);
        setPreview((v) => (v?.taskId === rowId && v.by === "key" ? null : { taskId: rowId, by: "key" }));
        e.preventDefault();
        e.stopPropagation();
        return;
      case "p":
      case "P":
        if (e.ctrlKey || e.metaKey || e.altKey) return;
        (pinned.has(rowId) ? p.onUnpin : p.onPin)(rowId);
        e.preventDefault();
        e.stopPropagation();
        return;
      case "e":
      case "E":
        if (e.ctrlKey || e.metaKey || e.altKey) return;
        p.onArchive(rowId);
        e.preventDefault();
        e.stopPropagation();
        return;
      default:
    }
  };

  const more: MenuItem[] = [
    ...(p.projectActions ? [{ id: "new-project", label: "New project…", testId: "agents-new-project", onSelect: p.projectActions.onNew }] : []),
    ...(p.onRowDetails ? ROW_DETAILS.map((d, i): MenuItem => ({ id: `row-details-${d.id}`, label: `Row details: ${d.label.toLowerCase()}`, radio: true, checked: rowDetails === d.id, separatorBefore: i === 0, onSelect: () => p.onRowDetails?.(d.id) })) : []),
  ];
  const groupingLabel = GROUPINGS.find((g) => g.id === p.grouping)?.label ?? "";

  return (
    <div className="agents" data-testid="agent-list" data-count={p.headers.length} data-visible={visible.length} data-grouping={p.grouping} data-row-details={rowDetails}>
      <div className="agents-head" data-testid="agents-head">
        <h2 className="agents-head-title" id="agents-head-title">
          Tasks
        </h2>
        <span className="agents-head-tools">
          {p.onSearch && <IconButton label="Search" icon={<Icon name="search" />} onClick={p.onSearch} data-testid="agents-search-open" />}
          <IconButton label={`List options: grouped by ${groupingLabel.toLowerCase()}${f > 0 ? `, ${f} ${f === 1 ? "filter" : "filters"} active` : ""}`} icon={<IconFilter />} pressed={toolsOpen} onClick={() => setToolsOpen((o) => !o)} data-testid="agents-tools-toggle" />
          {more.length > 0 && <Menu label="More" trigger={<span className="mb-icon" aria-hidden="true"><Icon name="ellipsis" /></span>} items={more} align="end" triggerClassName="mb-icon-btn" triggerTestId="agents-more" />}
        </span>
      </div>
      {views.length > 0 && (
        <div className="agents-states" role="group" aria-label="Task states" data-testid="agents-states">
          {views.map((v) => (
            <button key={v.id} type="button" className="agents-state" aria-pressed={p.filters.states.includes(v.id)} onClick={() => p.onFilters(toggleView(p.filters, v.id))} aria-label={`${v.label}, ${counts.get(v.id) ?? 0}`} title={v.label} data-testid={`state-view-${v.id}`} data-count={counts.get(v.id) ?? 0}>
              {v.short} <span className="agents-state-count">{counts.get(v.id) ?? 0}</span>
            </button>
          ))}
        </div>
      )}
      {(toolsOpen || narrowed || searching) && <div className="agents-controls" data-testid="agents-controls">
        <input type="search" className="agents-search" aria-label="Search conversations" placeholder="Search tasks and conversations" value={p.query} onChange={(e) => p.onQuery(e.target.value)} data-testid="agents-search" autoComplete="off" spellCheck={false} />
        <div className="agents-tools">
          <label className="agents-grouping">
            <span className="mb-sr-only">Group by</span>
            <select value={p.grouping} onChange={(e) => p.onGrouping(e.target.value as Grouping)} data-testid="agents-grouping">
              {GROUPINGS.map((g) => (
                <option key={g.id} value={g.id}>
                  Group: {g.label}
                </option>
              ))}
            </select>
          </label>
          {/* Words, not a second funnel: the Tasks head's list options already carry that glyph. */}
          <Button size="sm" variant="ghost" aria-label={f > 0 ? `Filters, ${f} active` : "Filters"} aria-pressed={panelOpen} onClick={() => setPanelOpen((o) => !o)} aria-expanded={panelOpen} aria-controls="agents-filter-panel" data-testid="agents-filter-toggle">
            Filters
          </Button>
          {f > 0 && <Badge tone="accent">{f}</Badge>}
          {p.viewActions && <ViewsMenu v={p.viewActions} now={now} />}
        </div>
        {panelOpen && (
          <div id="agents-filter-panel" className="agents-filters" role="group" aria-label="Filters" data-testid="agents-filters">
            <FilterGroup legend="State">
              {STATE_FILTERS.map((s) => (
                <Chip key={s.id} on={p.filters.states.includes(s.id)} testId={`filter-${s.id}`} onClick={() => p.onFilters({ ...p.filters, states: toggle(p.filters.states, s.id) })}>
                  {s.label}
                </Chip>
              ))}
            </FilterGroup>
            <FilterGroup legend="Location">
              {LOCATION_FILTERS.map((l) => (
                <Chip key={l} on={p.filters.locations.includes(l)} testId={`filter-${l}`} onClick={() => p.onFilters({ ...p.filters, locations: toggle(p.filters.locations, l) })}>
                  {l === "local" ? "Local" : "Cloud"}
                </Chip>
              ))}
            </FilterGroup>
            <FilterGroup legend="Origin">
              {ORIGIN_FILTERS.map((o) => (
                <Chip key={o.id} on={p.filters.origins.includes(o.id)} testId={`filter-origin-${o.id}`} onClick={() => p.onFilters({ ...p.filters, origins: toggle(p.filters.origins, o.id) })}>
                  {o.label}
                </Chip>
              ))}
            </FilterGroup>
            {p.onSort && (
              <FilterGroup legend="Sort">
                {SORTS.map((s) => (
                  <Chip key={s.id} on={sort === s.id} testId={`sort-${s.id}`} onClick={() => p.onSort?.(s.id)}>
                    {s.label}
                  </Chip>
                ))}
              </FilterGroup>
            )}
            {projectChoices.length > 0 && (
              <FilterGroup legend="Project">
                {projectChoices.map((pr) => (
                  <Chip key={pr.projectId} on={p.filters.projects.includes(pr.projectId)} testId={`filter-project-${pr.projectId}`} onClick={() => p.onFilters({ ...p.filters, projects: toggle(p.filters.projects, pr.projectId) })}>
                    {pr.name}
                  </Chip>
                ))}
                <Chip on={p.filters.projects.includes(NO_PROJECT)} testId="filter-project-none" onClick={() => p.onFilters({ ...p.filters, projects: toggle(p.filters.projects, NO_PROJECT) })}>
                  No project
                </Chip>
              </FilterGroup>
            )}
            <FilterGroup legend="Archive">
              <Chip on={p.filters.showArchived} testId="filter-archived" onClick={() => p.onFilters({ ...p.filters, showArchived: !p.filters.showArchived })}>
                Show archived
              </Chip>
            </FilterGroup>
            <FilterGroup legend="A row's second line shows">
              {SUBTITLE_FIELDS.map((s) => (
                <Chip key={s.id} on={p.subtitleFields.includes(s.id)} testId={`subtitle-${s.id}`} onClick={() => p.onSubtitleFields(toggle(p.subtitleFields, s.id))}>
                  {s.label}
                </Chip>
              ))}
              {p.onFactFields &&
                FACT_FIELDS.map((s) => (
                  <Chip key={s.id} on={factFields.includes(s.id)} testId={`subtitle-${s.id}`} onClick={() => p.onFactFields?.(toggle(factFields, s.id))}>
                    {s.label}
                  </Chip>
                ))}
            </FilterGroup>
            {f > 0 && (
              <button type="button" className="agents-link" data-testid="filters-clear" onClick={() => p.onFilters(NO_FILTERS)}>
                Clear filters
              </button>
            )}
          </div>
        )}
      </div>}
      {p.note && (
        <p className="agents-note meta" role="status" data-testid="agents-note">
          {p.note}
        </p>
      )}
      {searching ? (
        <SearchResults search={p.search} query={p.query} onSelect={p.onSelect} />
      ) : (
        <div
          ref={scroller}
          className="agents-scroll"
          onFocusCapture={(e) => {
            // Real focus wins over a focus still pending: a request made before it (a pin's follow-through, whose
            // commit a slow machine delays by seconds) must not pull the keyboard off the row the person just focused.
            const id = (e.target as HTMLElement).getAttribute("data-row-id");
            if (id) {
              pendingFocus.current = null;
              setActiveId((was) => (was === id ? was : id));
            }
          }}
          onScroll={(e) => {
            setScrollTop(e.currentTarget.scrollTop);
            if (preview) closePreview();
          }}
          onKeyDown={onKeyDown}
          onKeyUp={(e) => {
            // A button acts on Space when the key comes up: on a row Space is the preview, so the click is not made.
            if (e.key === " " && (e.target as HTMLElement).hasAttribute("data-row-id")) e.preventDefault();
          }}
          data-testid="agents-scroll"
        >
          {!p.loaded && !p.error && (
            <p className="meta agents-empty" role="status" data-testid="agents-loading">
              Loading tasks…
            </p>
          )}
          {p.error && !p.loaded && (
            <p className="meta agents-empty" role="alert" data-testid="agents-error">
              The task list is unavailable: {p.error}
            </p>
          )}
          <div className="agents-canvas" style={{ height: layout.total }}>
            {items.slice(range.start, range.end).map((item, i) => (
              <ItemView
                key={item.key}
                item={item}
                top={layout.offsets[range.start + i]!}
                p={p}
                metrics={metrics}
                pinned={pinned}
                selected={p.selectedId}
                tabStop={tabStop}
                titles={titles}
                previewId={preview?.taskId ?? null}
                onRowEnter={onRowEnter}
                onRowLeave={leavePointer}
                onOpen={(id) => {
                  closePreview();
                  p.onSelect(id);
                }}
              />
            ))}
          </div>
          {p.loaded && items.length === 0 && (
            <p className="meta agents-empty" data-testid="agents-empty">
              {p.headers.length === 0 ? "No tasks yet. New task starts one." : "No task matches these filters."}
            </p>
          )}
        </div>
      )}
      {preview && previewed && previewInWindow && !searching && (
        <PreviewCard
          key={preview.taskId}
          header={previewed}
          facts={p.facts?.get(preview.taskId)}
          nowMs={p.nowMs}
          byKeyboard={preview.by === "key"}
          onOpen={() => {
            const id = preview.taskId;
            closePreview();
            p.onSelect(id);
          }}
          onClose={(restoreFocus) => closePreview(restoreFocus ? preview.taskId : undefined)}
          onPointerEnter={() => clearTimer(leaveTimer)}
          onPointerLeave={() => {
            if (preview.by === "pointer") leavePointer();
          }}
        />
      )}
    </div>
  );
}

function FilterGroup({ legend, children }: { legend: string; children: ReactNode }) {
  return (
    <fieldset className="agents-fieldset">
      <legend>{legend}</legend>
      <div className="agents-chips">{children}</div>
    </fieldset>
  );
}

function Chip({ on, onClick, children, testId }: { on: boolean; onClick: () => void; children: ReactNode; testId: string }) {
  return (
    <button type="button" className="agents-chip" aria-pressed={on} onClick={onClick} data-testid={testId}>
      {children}
    </button>
  );
}

function ViewsMenu({ v, now }: { v: ViewActions; now: { grouping: Grouping; filters: Filters; sort: SortKey } }) {
  const active = v.views.find((x) => viewMatches(x, now));
  const items: MenuItem[] = [
    ...v.views.map((x, i) => ({ id: `view:${x.id}`, label: x.name, checked: active?.id === x.id, radio: true, separatorBefore: i === 0 && false, onSelect: () => v.onApply(x) })),
    { id: "view:save", label: "Save current view…", separatorBefore: v.views.length > 0, onSelect: v.onSaveRequest },
    ...(active ? [{ id: "view:delete", label: `Delete view “${active.name}”`, onSelect: () => v.onDelete(active.id) }] : []),
  ];
  return <Menu label={active ? `Views, ${active.name}` : "Views"} trigger={<IconViews />} items={items} align="end" triggerTestId="agents-views" />;
}

/** A group's own menu (MWS-04): it starts a task in the group's folder. Absent for a group that is not a place (a status, a day). */
function GroupMenu({ name, root, onNewTaskIn }: { name: string; root: string | undefined; onNewTaskIn: ((root: string) => void) | undefined }) {
  if (!root || !onNewTaskIn) return null;
  return <Menu label={`${name}: more`} trigger={<span className="mb-icon" aria-hidden="true"><Icon name="ellipsis" /></span>} align="end" triggerClassName="mb-icon-btn agents-group-menu" triggerTestId="group-menu" items={[{ id: "new-task-here", label: `New task in ${name}`, onSelect: () => onNewTaskIn(root) }]} />;
}

function ProjectRow({ item, top, p, metrics }: { item: Extract<ListItem, { kind: "project" }>; top: number; p: AgentListViewProps; metrics: ListMetrics }) {
  const pa = p.projectActions;
  const pr = item.project;
  const [over, setOver] = useState(false);
  const words = rollupWords(pr.rollup);
  return (
    <div
      className="agents-group agents-project"
      style={{ top, height: metrics.groupHead }}
      data-testid="project-row"
      data-project-id={pr.projectId}
      data-archived={pr.archived}
      data-selected={pa?.selectedId === pr.projectId}
      data-drop={over}
      onDragOver={(e) => {
        if (pa && !pr.archived && e.dataTransfer.types.includes(TASK_DRAG)) {
          e.preventDefault();
          setOver(true);
        }
      }}
      onDragLeave={() => setOver(false)}
      onDrop={(e) => {
        setOver(false);
        const taskId = e.dataTransfer.getData(TASK_DRAG);
        if (pa && taskId) {
          e.preventDefault();
          pa.onAdd(taskId, pr.projectId);
        }
      }}
    >
      <button type="button" className="agents-project-toggle" aria-expanded={!item.collapsed} aria-label={`${item.collapsed ? "Expand" : "Collapse"} ${pr.name}`} onClick={() => p.onToggleSection(projectKey(pr.projectId))} data-testid="project-toggle">
        <Icon name={item.collapsed ? "chevron-right" : "chevron-down"} size={14} />
      </button>
      <button type="button" className="agents-project-open" onClick={() => pa?.onOpen(pr.projectId)} aria-current={pa?.selectedId === pr.projectId ? "true" : undefined} title={`${pr.name}\n${pr.workspaceRoot}`} data-testid="project-open">
        <ProjectGlyph icon={pr.icon} color={pr.color} size={14} />
        <span className="agents-section-label" data-testid="project-name-text">
          {pr.name}
          {pr.archived ? " (archived)" : ""}
        </span>
        <span className="mb-sr-only" data-testid="project-rollup">
          {words}
        </span>
        <span className="agents-section-count" aria-hidden="true">
          {item.shown}
        </span>
        {pr.rollup.attentionTasks > 0 && <span className="agents-dot" data-kind="attention" data-testid="project-attention-dot" aria-hidden="true" />}
      </button>
      {pa && (
        <span className="agents-actions">
          <IconButton label={`Rename ${pr.name}`} icon={<IconRename />} size="sm" onClick={() => pa.onRename(pr.projectId)} data-testid="project-rename" />
          {pr.archived ? <IconButton label={`Restore ${pr.name}`} icon={<IconRestore />} size="sm" onClick={() => pa.onRestore(pr.projectId)} data-testid="project-restore" /> : <IconButton label={`Archive ${pr.name}`} icon={<IconArchive />} size="sm" onClick={() => pa.onArchive(pr.projectId)} data-testid="project-archive" />}
          {!pr.archived && <GroupMenu name={pr.name} root={pr.workspaceRoot} onNewTaskIn={p.onNewTaskIn} />}
        </span>
      )}
    </div>
  );
}

interface ItemViewProps {
  item: ListItem;
  top: number;
  p: AgentListViewProps;
  metrics: ListMetrics;
  pinned: ReadonlySet<string>;
  selected: string | null;
  tabStop: string | null;
  titles: ReadonlyMap<string, string>;
  previewId: string | null;
  onRowEnter: (taskId: string) => void;
  onRowLeave: () => void;
  onOpen: (taskId: string) => void;
}

function ItemView({ item, top, p, metrics, pinned, selected, tabStop, titles, previewId, onRowEnter, onRowLeave, onOpen }: ItemViewProps) {
  if (item.kind === "project") return <ProjectRow item={item} top={top} p={p} metrics={metrics} />;
  if (item.kind === "section") {
    const key = item.key.slice(2);
    // A group that is a place says how many tasks it holds only while it is closed; a status or a day always does.
    const counted = item.root === undefined || item.collapsed || item.pinned;
    return (
      <div className="agents-group" style={{ top, height: metrics.groupHead }}>
        <div className="agents-section" data-testid="agents-section" data-section={key}>
          <button type="button" className="agents-section-btn" aria-expanded={!item.collapsed} onClick={() => p.onToggleSection(key)}>
            <span className="agents-section-label">{item.label}</span>
            {counted ? <span className="agents-section-count">{item.count}</span> : <span className="mb-sr-only">{item.count}</span>}
            <Icon name={item.collapsed ? "chevron-right" : "chevron-down"} size={14} className="agents-caret" />
          </button>
        </div>
        {!item.pinned && item.root !== undefined && p.onNewTaskIn && (
          <span className="agents-actions">
            <GroupMenu name={item.label} root={item.root} onNewTaskIn={p.onNewTaskIn} />
          </span>
        )}
      </div>
    );
  }
  const h = item.header;
  const meta = CLASS_META[h.statusClass];
  const isPinned = pinned.has(h.taskId);
  const isTab = tabStop === h.taskId;
  const facts = p.facts?.get(h.taskId);
  const sub = rowLine(h, p.subtitleFields, facts, p.factFields ?? ALL_FACTS);
  const words = h.statusLabel || meta.short;
  const pa = p.projectActions;
  const menu = pa ? projectMenuFor(h, p.projects ?? EMPTY_PROJECTS) : [];
  const inProject = (p.projects ?? EMPTY_PROJECTS).find((x) => x.projectId === h.projectId);
  const files = changedFilesShown(h);
  const parentTitle = item.depth > 0 ? titles.get(p.parents?.get(h.taskId) ?? "") : undefined;
  // The mark of a pending approval is the state's own glyph while the Core's class is attention; a dot of its own otherwise.
  const attentionOnGlyph = h.pendingApproval && h.statusClass === "NEEDS_ATTENTION";
  return (
    <div
      className="agents-row"
      style={{ top, height: item.twoLine ? metrics.rowTwoLine : metrics.row, "--agents-depth": Math.min(item.depth, 3) } as CSSProperties}
      data-testid="agent-row"
      data-agent-id={h.taskId}
      data-task-id={h.taskId}
      data-state={h.taskState}
      data-subagent={h.subagent}
      data-class={h.statusClass}
      data-unread={h.unread}
      data-selected={selected === h.taskId}
      data-pinned={isPinned}
      data-lines={item.twoLine ? 2 : 1}
      data-depth={item.depth}
      data-preview={previewId === h.taskId}
      data-project-id={h.projectId || undefined}
      draggable={pa !== undefined}
      onDragStart={(e) => {
        e.dataTransfer.setData(TASK_DRAG, h.taskId);
        e.dataTransfer.effectAllowed = "move";
      }}
      onMouseEnter={() => onRowEnter(h.taskId)}
      onMouseLeave={onRowLeave}
    >
      <button
        type="button"
        className="agents-open"
        data-row-id={h.taskId}
        data-testid="agent-open"
        tabIndex={isTab ? 0 : -1}
        aria-current={selected === h.taskId ? "true" : undefined}
        aria-label={rowLabel(h, isPinned, facts, parentTitle)}
        title={rowTitle(h, facts, localMachine(), p.nowMs)}
        onClick={() => onOpen(h.taskId)}
      >
        {item.depth > 0 && <span className="agents-tree" data-testid="agent-child-mark" aria-hidden="true" />}
        <span className="agents-text">
          <span className="agents-title" data-testid="agent-title">
            {h.origin === "automation" && (
              <span className="agents-origin" data-testid="agent-origin-badge" data-origin="automation">
                Automation
              </span>
            )}
            {h.title || "Untitled task"}
          </span>
          {item.twoLine && (
            <span className="agents-sub">
              <span className="agents-status" data-testid="agent-status">
                {words}
              </span>
              {sub ? <span data-testid="agent-facts"> · {sub}</span> : null}
              {inProject && p.grouping !== "project" && item.sectionKey.startsWith("project:") === false ? ` · ${inProject.name}` : ""}
            </span>
          )}
        </span>
        <span className="agents-end">
          {/* The state is never colour alone: its glyph carries the words as its text alternative, and on a one-line row those are the row's status words. */}
          <span className="agents-glyph" data-testid={attentionOnGlyph ? "agent-attention-dot" : "agent-glyph"} data-kind={attentionOnGlyph ? "attention" : undefined}>
            <span data-testid={item.twoLine ? undefined : "agent-status"}>
              <StatusDot status={meta.status} label={words} />
            </span>
          </span>
          {h.pendingApproval && !attentionOnGlyph && <span className="agents-dot" data-kind="attention" data-testid="agent-attention-dot" aria-hidden="true" />}
          {files > 0 && (
            <span className="agents-count" data-testid="agent-changes" aria-hidden="true">
              {files}
            </span>
          )}
          {h.unread && <span className="agents-dot" data-kind="unread" data-testid="agent-unread-dot" aria-hidden="true" />}
        </span>
      </button>
      <span className="agents-actions">
        {pa && menu.length > 0 && (
          <Menu
            label={`Project for ${h.title}`}
            trigger={<IconProject />}
            align="end"
            triggerTestId="agent-project-menu"
            items={menu.map((m, i) => ({ id: m.id, label: m.label, ...(m.kind === "add" ? { checked: m.checked } : {}), separatorBefore: m.kind === "remove" && i > 0, onSelect: () => (m.kind === "add" ? pa.onAdd(h.taskId, m.projectId) : pa.onRemove(h.taskId, m.projectId)) }))}
          />
        )}
        <IconButton label={isPinned ? `Unpin ${h.title}` : `Pin ${h.title}`} icon={<IconPin />} size="sm" pressed={isPinned} tabIndex={isTab ? 0 : -1} onClick={() => (isPinned ? p.onUnpin(h.taskId) : p.onPin(h.taskId))} data-testid="agent-pin" />
        <IconButton label={`Archive ${h.title}`} icon={<IconArchive />} size="sm" tabIndex={isTab ? 0 : -1} onClick={() => p.onArchive(h.taskId)} data-testid="agent-archive" />
      </span>
    </div>
  );
}

function SearchResults({ search, query, onSelect }: { search: AgentListViewProps["search"]; query: string; onSelect: (taskId: string, rowId?: string) => void }) {
  const r = search.results;
  return (
    <div className="agents-scroll agents-results" data-testid="agents-results" aria-busy={search.state === "searching"}>
      <p className="meta agents-note" role="status" data-testid="agents-search-status">
        {search.state === "searching" ? `Searching for “${query.trim()}”…` : search.state === "error" ? `Search failed: ${search.error ?? "unknown error"}` : r ? `${r.totalHits} ${r.totalHits === 1 ? "conversation matches" : "conversations match"} “${r.query || query.trim()}”.` : ""}
      </p>
      {r?.hits.map((hit) => {
        const meta = CLASS_META[hit.statusClass];
        return (
          <section key={hit.taskId} className="agents-hit" data-testid="search-hit" data-agent-id={hit.taskId} aria-label={hit.title}>
            <button type="button" className="agents-hit-title" onClick={() => onSelect(hit.taskId)} data-testid="search-hit-open">
              <StatusDot status={meta.status} label={hit.statusLabel || meta.short} />
              <span className="agents-title">{hit.title || "Untitled task"}</span>
              <span className="meta">{hit.statusLabel || meta.short}</span>
            </button>
            {hit.snippets.map((s) => (
              <button type="button" key={s.rowId} className="agents-snippet" onClick={() => onSelect(hit.taskId, s.rowId)} data-testid="search-snippet" data-row-id={s.rowId}>
                <span className="meta agents-snippet-source">{s.source.toLowerCase()}</span>
                <span className="agents-snippet-text">
                  {s.cutBefore ? "…" : ""}
                  {highlightRuns(s.text, s.matches).map((run, i) => (run.match ? <mark key={i}>{run.text}</mark> : <span key={i}>{run.text}</span>))}
                  {s.cutAfter ? "…" : ""}
                </span>
              </button>
            ))}
          </section>
        );
      })}
      {search.state === "done" && r && r.hits.length === 0 && (
        <p className="meta agents-empty" data-testid="agents-no-hits">
          Nothing matches. The search covers titles and the text of messages, commands and tool results.
        </p>
      )}
    </div>
  );
}
