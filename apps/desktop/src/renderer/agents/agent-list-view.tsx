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
import { Badge, IconButton, StatusDot } from "@modbit/ui";
import type { AgentHeaderView, ConversationSearchView } from "../../shared/conversation-types.ts";
import { CLASS_META, GROUPINGS, LOCATION_FILTERS, ORIGIN_FILTERS, ROW_HEIGHT, STATE_FILTERS, SUBTITLE_FIELDS, filterCount, flatten, groupHeaders, layoutOf, passes, relativeAge, rowLabel, subtitleOf, visibleRange, type Filters, type Grouping, type ListItem, type SubtitleField } from "./list-model.ts";
import { highlightRuns } from "./snippets.ts";
import { IconArchive, IconFilter, IconPin } from "./icons.tsx";

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
  /** The row to focus after the list changes (the nearest sibling of an archived row). */
  focusRequest?: { taskId: string; n: number } | null | undefined;
}

const toggle = <T,>(xs: readonly T[], x: T): T[] => (xs.includes(x) ? xs.filter((y) => y !== x) : [...xs, x]);

export function AgentListView(p: AgentListViewProps) {
  const [panelOpen, setPanelOpen] = useState(false);
  const scroller = useRef<HTMLDivElement>(null);
  const [scrollTop, setScrollTop] = useState(0);
  const [viewport, setViewport] = useState(480);
  const [activeId, setActiveId] = useState<string | null>(null);
  const pendingFocus = useRef<string | null>(null);
  const pinned = useMemo(() => new Set(p.pins), [p.pins]);

  const visible = useMemo(() => p.headers.filter((h) => passes(h, p.filters)), [p.headers, p.filters]);
  const sections = useMemo(() => groupHeaders(visible, p.grouping, p.pins, p.nowMs), [visible, p.grouping, p.pins, p.nowMs]);
  const items = useMemo(() => flatten(sections, p.collapsed), [sections, p.collapsed]);
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
              <button type="button" className="agents-link" data-testid="filters-clear" onClick={() => p.onFilters({ states: [], locations: [], origins: [], showArchived: false })}>
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

function ItemView({ item, top, p, pinned, selected, tabStop }: { item: ListItem; top: number; p: AgentListViewProps; pinned: ReadonlySet<string>; selected: string | null; tabStop: string | null }) {
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
  return (
    <div className="agents-row" style={{ top, height: ROW_HEIGHT }} data-testid="agent-row" data-agent-id={h.taskId} data-class={h.statusClass} data-unread={h.unread} data-selected={selected === h.taskId} data-pinned={isPinned}>
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
          <span className="agents-title">{h.title || "Untitled task"}</span>
          <span className="agents-sub">
            <span className="agents-status" data-testid="agent-status">
              {words}
            </span>
            {sub ? ` · ${sub}` : ""}
          </span>
        </span>
        <span className="agents-end">
          {h.pendingApproval && <span className="agents-dot" data-kind="attention" data-testid="agent-attention-dot" aria-hidden="true" />}
          {h.unread && <span className="agents-dot" data-kind="unread" data-testid="agent-unread-dot" aria-hidden="true" />}
          <span className="agents-age">{relativeAge(h.updatedAtMs, p.nowMs)}</span>
        </span>
      </button>
      <span className="agents-actions">
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
