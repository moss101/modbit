/**
 * The agent list, presentational (REQ-PX-046): headers in, callbacks out. The
 * connected list and the state gallery both render it, so the gallery shows
 * the real component with fixture headers. It shows the status class it is
 * given (glyph and words, never colour alone), unread and attention dots,
 * pins, hover and focus actions, grouping, filters and search results, and it
 * windows its rows: only those in the viewport (plus an overscan) are in the
 * document, so a thousand tasks cost the same as twenty.
 */
import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, type KeyboardEvent, type ReactNode } from "react";
import { Badge, IconButton, Menu, StatusDot, type MenuItem } from "@modbit/ui";
import type { AgentHeaderView, ConversationSearchView } from "../../shared/conversation-types.ts";
import type { ProjectInfo } from "../../shared/project-types.ts";
import { CLASS_META, GROUPINGS, LOCATION_FILTERS, NO_FILTERS, NO_PROJECT, ORIGIN_FILTERS, PROJECTS_KEY, ROW_HEIGHT, SORTS, STATE_FILTERS, SUBTITLE_FIELDS, filterCount, layoutOf, listItems, passes, projectKey, relativeAge, rowLabel, subtitleOf, viewMatches, visibleProjects, visibleRange, type Filters, type Grouping, type ListItem, type SortKey, type StoredView, type SubtitleField } from "./list-model.ts";
import { ProjectGlyph } from "../projects/project-glyph.tsx";
import { projectMenuFor, rollupWords } from "../projects/project-model.ts";
import { highlightRuns } from "./snippets.ts";
import { IconArchive, IconFilter, IconPin, IconProject, IconRename, IconRestore, IconViews } from "./icons.tsx";

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
}

const toggle = <T,>(xs: readonly T[], x: T): T[] => (xs.includes(x) ? xs.filter((y) => y !== x) : [...xs, x]);
const EMPTY_PROJECTS: readonly ProjectInfo[] = [];

export function AgentListView(p: AgentListViewProps) {
  const [panelOpen, setPanelOpen] = useState(false);
  const scroller = useRef<HTMLDivElement>(null);
  const [scrollTop, setScrollTop] = useState(0);
  const [viewport, setViewport] = useState(480);
  const [activeId, setActiveId] = useState<string | null>(null);
  const pendingFocus = useRef<string | null>(null);
  const pinned = useMemo(() => new Set(p.pins), [p.pins]);

  const visible = useMemo(() => p.headers.filter((h) => passes(h, p.filters)), [p.headers, p.filters]);
  const projects = p.projects ?? EMPTY_PROJECTS;
  const sort = p.sort ?? "recent";
  const items = useMemo(() => listItems({ headers: visible, projects, grouping: p.grouping, pins: p.pins, collapsed: p.collapsed, nowMs: p.nowMs, sort, showArchived: p.filters.showArchived }), [visible, projects, p.grouping, p.pins, p.collapsed, p.nowMs, sort, p.filters.showArchived]);
  const projectChoices = useMemo(() => visibleProjects(projects, p.filters.showArchived), [projects, p.filters.showArchived]);
  const now = { grouping: p.grouping, filters: p.filters, sort };
  const layout = useMemo(() => layoutOf(items), [items]);
  const range = visibleRange(layout, items.length, scrollTop, viewport);

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
      if (top < el.scrollTop) el.scrollTop = top;
      else if (top + ROW_HEIGHT > el.scrollTop + el.clientHeight) el.scrollTop = top + ROW_HEIGHT - el.clientHeight;
      setScrollTop(el.scrollTop);
    },
    [rowIds, layout],
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
    const page = Math.max(1, Math.floor(viewport / ROW_HEIGHT) - 1);
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

  const f = filterCount(p.filters);
  const searching = p.query.trim().length > 0;

  return (
    <div className="agents" data-testid="agent-list" data-count={p.headers.length} data-visible={visible.length}>
      <div className="agents-controls">
        <input type="search" className="agents-search" aria-label="Search conversations" placeholder="Search tasks and conversations" value={p.query} onChange={(e) => p.onQuery(e.target.value)} data-testid="agents-search" autoComplete="off" spellCheck={false} />
        <div className="agents-tools">
          <label className="agents-group">
            <span className="mb-sr-only">Group by</span>
            <select value={p.grouping} onChange={(e) => p.onGrouping(e.target.value as Grouping)} data-testid="agents-grouping">
              {GROUPINGS.map((g) => (
                <option key={g.id} value={g.id}>
                  Group: {g.label}
                </option>
              ))}
            </select>
          </label>
          <IconButton label={f > 0 ? `Filters, ${f} active` : "Filters"} icon={<IconFilter />} pressed={panelOpen} onClick={() => setPanelOpen((o) => !o)} aria-expanded={panelOpen} aria-controls="agents-filter-panel" data-testid="agents-filter-toggle" />
          {f > 0 && <Badge tone="accent">{f}</Badge>}
          {p.viewActions && <ViewsMenu v={p.viewActions} now={now} />}
          {p.projectActions && <IconButton label="New project" icon={<IconProject />} onClick={p.projectActions.onNew} data-testid="agents-new-project" />}
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
            <FilterGroup legend="Row details">
              {SUBTITLE_FIELDS.map((s) => (
                <Chip key={s.id} on={p.subtitleFields.includes(s.id)} testId={`subtitle-${s.id}`} onClick={() => p.onSubtitleFields(toggle(p.subtitleFields, s.id))}>
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
      </div>
      {p.note && (
        <p className="agents-note meta" role="status" data-testid="agents-note">
          {p.note}
        </p>
      )}
      {searching ? (
        <SearchResults search={p.search} query={p.query} onSelect={p.onSelect} />
      ) : (
        <div ref={scroller} className="agents-scroll" onScroll={(e) => setScrollTop(e.currentTarget.scrollTop)} onKeyDown={onKeyDown} data-testid="agents-scroll">
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
          {p.loaded && items.length === 0 && (
            <p className="meta agents-empty" data-testid="agents-empty">
              {p.headers.length === 0 ? "No tasks yet. New task starts one." : "No task matches these filters."}
            </p>
          )}
          <div className="agents-canvas" style={{ height: layout.total }}>
            {items.slice(range.start, range.end).map((item, i) => (
              <ItemView key={item.key} item={item} top={layout.offsets[range.start + i]!} p={p} pinned={pinned} selected={p.selectedId} tabStop={tabStop} />
            ))}
          </div>
        </div>
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

function ProjectRow({ item, top, p }: { item: Extract<ListItem, { kind: "project" }>; top: number; p: AgentListViewProps }) {
  const pa = p.projectActions;
  const pr = item.project;
  const [over, setOver] = useState(false);
  const words = rollupWords(pr.rollup);
  return (
    <div
      className="agents-project"
      style={{ top, height: 40 }}
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
        <span className="agents-caret" aria-hidden="true">
          {item.collapsed ? "▸" : "▾"}
        </span>
      </button>
      <button type="button" className="agents-project-open" onClick={() => pa?.onOpen(pr.projectId)} aria-current={pa?.selectedId === pr.projectId ? "true" : undefined} title={`${pr.name}\n${pr.workspaceRoot}`} data-testid="project-open">
        <ProjectGlyph icon={pr.icon} color={pr.color} />
        <span className="agents-text">
          <span className="agents-title" data-testid="project-name-text">
            {pr.name}
            {pr.archived ? " (archived)" : ""}
          </span>
          <span className="agents-sub" data-testid="project-rollup">
            {words}
          </span>
        </span>
        {pr.rollup.attentionTasks > 0 && <span className="agents-dot" data-kind="attention" data-testid="project-attention-dot" aria-hidden="true" />}
      </button>
      {pa && (
        <span className="agents-actions">
          <IconButton label={`Rename ${pr.name}`} icon={<IconRename />} size="sm" onClick={() => pa.onRename(pr.projectId)} data-testid="project-rename" />
          {pr.archived ? <IconButton label={`Restore ${pr.name}`} icon={<IconRestore />} size="sm" onClick={() => pa.onRestore(pr.projectId)} data-testid="project-restore" /> : <IconButton label={`Archive ${pr.name}`} icon={<IconArchive />} size="sm" onClick={() => pa.onArchive(pr.projectId)} data-testid="project-archive" />}
        </span>
      )}
    </div>
  );
}

function ItemView({ item, top, p, pinned, selected, tabStop }: { item: ListItem; top: number; p: AgentListViewProps; pinned: ReadonlySet<string>; selected: string | null; tabStop: string | null }) {
  if (item.kind === "projects-head") {
    return (
      <div className="agents-section" style={{ top }} data-testid="agents-section" data-section={PROJECTS_KEY}>
        <button type="button" className="agents-section-btn" aria-expanded={!item.collapsed} onClick={() => p.onToggleSection(PROJECTS_KEY)}>
          <span className="agents-caret" aria-hidden="true">
            {item.collapsed ? "▸" : "▾"}
          </span>
          <span className="agents-section-label">{item.label}</span>
          <span className="meta">{item.count}</span>
        </button>
      </div>
    );
  }
  if (item.kind === "project") return <ProjectRow item={item} top={top} p={p} />;
  if (item.kind === "new-project") {
    return (
      <div className="agents-newproject" style={{ top, height: 32 }}>
        <button type="button" className="agents-link agents-newproject-btn" onClick={() => p.projectActions?.onNew()} data-testid="project-new-row">
          + New project
        </button>
      </div>
    );
  }
  if (item.kind === "section") {
    const key = item.key.slice(2);
    return (
      <div className="agents-section" style={{ top }} data-testid="agents-section" data-section={key}>
        <button type="button" className="agents-section-btn" aria-expanded={!item.collapsed} onClick={() => p.onToggleSection(key)}>
          <span className="agents-caret" aria-hidden="true">
            {item.collapsed ? "▸" : "▾"}
          </span>
          <span className="agents-section-label">{item.label}</span>
          <span className="meta">{item.count}</span>
        </button>
      </div>
    );
  }
  const h = item.header;
  const meta = CLASS_META[h.statusClass];
  const isPinned = pinned.has(h.taskId);
  const isTab = tabStop === h.taskId;
  const sub = subtitleOf(h, p.subtitleFields);
  const words = h.statusLabel || meta.short;
  const pa = p.projectActions;
  const menu = pa ? projectMenuFor(h, p.projects ?? EMPTY_PROJECTS) : [];
  const inProject = (p.projects ?? EMPTY_PROJECTS).find((x) => x.projectId === h.projectId);
  return (
    <div
      className="agents-row"
      style={{ top, height: ROW_HEIGHT }}
      data-testid="agent-row"
      data-agent-id={h.taskId}
      data-class={h.statusClass}
      data-unread={h.unread}
      data-selected={selected === h.taskId}
      data-pinned={isPinned}
      data-project-id={h.projectId || undefined}
      draggable={pa !== undefined}
      onDragStart={(e) => {
        e.dataTransfer.setData(TASK_DRAG, h.taskId);
        e.dataTransfer.effectAllowed = "move";
      }}
    >
      <button
        type="button"
        className="agents-open"
        data-row-id={h.taskId}
        data-testid="agent-open"
        tabIndex={isTab ? 0 : -1}
        aria-current={selected === h.taskId ? "true" : undefined}
        aria-label={rowLabel(h, isPinned)}
        title={`${h.title || "Untitled task"}${h.workspaceRoot ? `\n${h.workspaceRoot}` : ""}`}
        onClick={() => p.onSelect(h.taskId)}
      >
        <span className="agents-glyph">
          <StatusDot status={meta.status} label={words} />
        </span>
        <span className="agents-text">
          <span className="agents-title">
            {h.origin === "automation" && (
              <span className="agents-origin" data-testid="agent-origin-badge" data-origin="automation">
                Automation
              </span>
            )}
            {h.title || "Untitled task"}
          </span>
          <span className="agents-sub">
            <span className="agents-status" data-testid="agent-status">
              {words}
            </span>
            {sub ? ` · ${sub}` : ""}
            {inProject && p.grouping !== "project" && item.sectionKey.startsWith("project:") === false ? ` · ${inProject.name}` : ""}
          </span>
        </span>
        <span className="agents-end">
          {h.pendingApproval && <span className="agents-dot" data-kind="attention" data-testid="agent-attention-dot" aria-hidden="true" />}
          {h.unread && <span className="agents-dot" data-kind="unread" data-testid="agent-unread-dot" aria-hidden="true" />}
          <span className="agents-age">{relativeAge(h.updatedAtMs, p.nowMs)}</span>
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
