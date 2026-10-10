/**
 * The connected agent list (REQ-PX-046): the Core's header projection kept
 * current by events, search over conversations through the Core's index,
 * pins, archive with a confirmation for a running task and an undo that lasts
 * longer than the 8 s the spec asks for, and the selection that moves to the
 * nearest remaining sibling. Every effect is a typed preload call; the status
 * class of a row is the Core's and is never recomputed here.
 */
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Button, Dialog, trays } from "@modbit/ui";
import type { ConversationSearchView } from "../../shared/conversation-types.ts";
import { AgentListView } from "./agent-list-view.tsx";
import { deleteView, listItems, nearestSibling, passes, pin as pinTask, saveView, unpin as unpinTask, visibleTaskIds, type Filters, type Grouping, type SortKey, type StoredView, type SubtitleField } from "./list-model.ts";
import { loadListPrefs, saveListPrefs, type ListPrefs } from "./prefs.ts";
import { useAgents } from "./use-agents.ts";
import { NewProjectDialog, RenameProjectDialog, SaveViewDialog } from "../projects/project-dialogs.tsx";
import { archiveProject as archiveProjectWithUndo } from "../projects/archive-project.ts";
import { newViewId, refusalSentence, workspaceChoices } from "../projects/project-model.ts";
import type { ProjectsState } from "../projects/use-projects.ts";

/** The undo stays at least this long (AFW-B08: not less than 8 s). */
export const UNDO_MS = 12_000;
const SEARCH_DEBOUNCE_MS = 250;

export interface AgentListProps {
  sessionId: string | null;
  connected: boolean;
  selectedTaskId: string | null;
  /** Opens a task's conversation; `rowId` scrolls to the row a search hit came from. */
  onSelect: (taskId: string | null, rowId?: string) => void;
  /** Tells the shell the list's current numbers (for the summary line and the palette). */
  onHeaders?: ((n: { total: number; attention: number }) => void) | undefined;
  /** The Core's projects (PX-063), read once by the shell and shared with the project page. */
  projects?: ProjectsState | undefined;
  selectedProjectId?: string | null | undefined;
  /** Opens a project's page in the centre region. */
  onOpenProject?: ((projectId: string) => void) | undefined;
  /** A project left the list (archived or renamed away): the page showing it is told. */
  onProjectArchived?: ((projectId: string, archived: boolean) => void) | undefined;
}

export function AgentList({ sessionId, connected, selectedTaskId, onSelect, onHeaders, projects, selectedProjectId = null, onOpenProject, onProjectArchived }: AgentListProps) {
  const [prefs, setPrefs] = useState<ListPrefs>(loadListPrefs);
  const update = useCallback((f: (p: ListPrefs) => ListPrefs) => setPrefs((p) => f(p)), []);
  useEffect(() => saveListPrefs(prefs), [prefs]);
  const agents = useAgents(sessionId, prefs.filters.showArchived, connected);
  const [note, setNote] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [search, setSearch] = useState<{ state: "idle" | "searching" | "done" | "error"; results: ConversationSearchView | null; error: string | null }>({ state: "idle", results: null, error: null });
  const [confirmStop, setConfirmStop] = useState<{ taskId: string; title: string } | null>(null);
  const [newProject, setNewProject] = useState(false);
  const [renaming, setRenaming] = useState<string | null>(null);
  const [savingView, setSavingView] = useState(false);
  const [focusRequest, setFocusRequest] = useState<{ taskId: string; n: number } | null>(null);
  const [nowMs, setNowMs] = useState(() => Date.now());
  const undoTimers = useRef(new Map<string, ReturnType<typeof setTimeout>>());

  // Relative ages move on; the list does not need to be exact to the second.
  useEffect(() => {
    const t = setInterval(() => setNowMs(Date.now()), 30_000);
    return () => clearInterval(t);
  }, []);
  useEffect(() => setNowMs(Date.now()), [agents.refreshedAt]);

  useEffect(() => {
    onHeaders?.({ total: agents.headers.length, attention: agents.headers.filter((h) => h.statusClass === "NEEDS_ATTENTION").length });
  }, [agents.headers, onHeaders]);

  useEffect(() => {
    if (!note) return;
    const t = setTimeout(() => setNote(null), 7000);
    return () => clearTimeout(t);
  }, [note]);

  // Search: the Core's index serves headers and bodies; a newer query supersedes an older one.
  const seq = useRef(0);
  useEffect(() => {
    const q = query.trim();
    if (!q || !sessionId) {
      seq.current++;
      setSearch({ state: "idle", results: null, error: null });
      return;
    }
    setSearch((s) => ({ ...s, state: "searching" }));
    const mine = ++seq.current;
    const t = setTimeout(() => {
      window.modbit
        .searchConversations(sessionId, q, { limit: 30, maxSnippets: 3, includeArchived: prefs.filters.showArchived })
        .then((results) => {
          if (mine === seq.current) setSearch({ state: "done", results, error: null });
        })
        .catch((e: Error) => {
          if (mine === seq.current) setSearch({ state: "error", results: null, error: e.message.replace(/^Error invoking remote method '[^']*': (Error: )?/, "") });
        });
    }, SEARCH_DEBOUNCE_MS);
    return () => clearTimeout(t);
  }, [query, sessionId, prefs.filters.showArchived]);

  const collapsed = useMemo(() => new Set(prefs.collapsed[prefs.grouping] ?? []), [prefs.collapsed, prefs.grouping]);

  /** The rows as the person sees them, for the sibling an archive hands the selection to. */
  const order = useCallback(() => {
    const visible = agents.headers.filter((h) => passes(h, prefs.filters));
    return visibleTaskIds(listItems({ headers: visible, projects: projects?.projects ?? [], grouping: prefs.grouping, pins: prefs.pins, collapsed, nowMs: Date.now(), sort: prefs.sort, showArchived: prefs.filters.showArchived }));
  }, [agents.headers, projects?.projects, prefs.filters, prefs.grouping, prefs.pins, prefs.sort, collapsed]);

  const dismissUndo = useCallback((taskId: string) => {
    const id = `archive:${taskId}`;
    const t = undoTimers.current.get(id);
    if (t) clearTimeout(t);
    undoTimers.current.delete(id);
    trays.dismiss(id);
  }, []);
  useEffect(() => {
    const timers = undoTimers.current;
    return () => {
      for (const t of timers.values()) clearTimeout(t);
      timers.clear();
    };
  }, []);

  const doArchive = useCallback(
    async (taskId: string, title: string, stopFirst: boolean) => {
      if (!sessionId) return;
      const sibling = nearestSibling(order(), taskId);
      try {
        // A running task is cancelled first, as its own recorded event; the Core then archives it.
        if (stopFirst) await window.modbit.cancelTask(sessionId, taskId);
      } catch (e) {
        setNote(`Could not stop “${title}”: ${(e as Error).message.replace(/^Error invoking remote method '[^']*': (Error: )?/, "")}`);
        return;
      }
      const r = await agents.archive(taskId);
      if (!r.ok) {
        setNote(`Could not archive “${title}”: ${r.message}`);
        return;
      }
      if (selectedTaskId === taskId) onSelect(sibling);
      if (sibling) setFocusRequest((f) => ({ taskId: sibling, n: (f?.n ?? 0) + 1 }));
      const id = `archive:${taskId}`;
      trays.present({
        id,
        tone: "info",
        title: `Archived “${title}”`,
        body: stopFirst ? "The run was stopped first. Undo restores the conversation; it does not restart the run." : "It is hidden from the list, not deleted.",
        actions: [
          {
            id: "undo",
            label: "Undo",
            primary: true,
            run: () => {
              void agents.restore(taskId).then((u) => {
                if (u.ok) {
                  dismissUndo(taskId);
                  setFocusRequest((f) => ({ taskId, n: (f?.n ?? 0) + 1 }));
                } else setNote(`Could not undo: ${u.message}`);
              });
            },
          },
        ],
      });
      undoTimers.current.set(id, setTimeout(() => dismissUndo(taskId), UNDO_MS));
    },
    [sessionId, order, agents, selectedTaskId, onSelect, dismissUndo],
  );

  const requestArchive = useCallback(
    (taskId: string) => {
      const h = agents.headers.find((x) => x.taskId === taskId);
      if (!h) return;
      if (h.taskState.toLowerCase() === "running") setConfirmStop({ taskId, title: h.title || "this task" });
      else void doArchive(taskId, h.title || "task", false);
    },
    [agents.headers, doArchive],
  );

  // ---- projects (PX-063/064): every change is a Core command; the list shows what the Core's next read says.
  const projectName = (id: string) => projects?.projects.find((x) => x.projectId === id)?.name ?? "the project";
  const addToProject = useCallback(
    async (taskId: string, projectId: string) => {
      if (!sessionId || !projects) return;
      const title = agents.headers.find((h) => h.taskId === taskId)?.title || "the task";
      try {
        await projects.addMember(sessionId, projectId, taskId);
        setNote(null);
        agents.refresh();
      } catch (e) {
        // The Core's typed refusal, as it said it; nothing on the list changed.
        setNote(refusalSentence(`Could not add “${title}” to “${projectName(projectId)}”`, e));
      }
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [sessionId, projects, agents],
  );
  const removeFromProject = useCallback(
    async (taskId: string, projectId: string) => {
      if (!sessionId || !projects) return;
      const title = agents.headers.find((h) => h.taskId === taskId)?.title || "the task";
      try {
        await projects.removeMember(sessionId, projectId, taskId);
        setNote(null);
        agents.refresh();
      } catch (e) {
        setNote(refusalSentence(`Could not remove “${title}” from “${projectName(projectId)}”`, e));
      }
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [sessionId, projects, agents],
  );
  const archiveProject = useCallback(
    async (projectId: string, archived: boolean) => {
      if (!sessionId || !projects) return;
      await archiveProjectWithUndo(projects, sessionId, projectId, projectName(projectId), archived, {
        onError: setNote,
        onChanged: (id, a) => {
          onProjectArchived?.(id, a);
          agents.refresh();
        },
      });
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [sessionId, projects, agents, onProjectArchived],
  );
  const renamingProject = renaming ? (projects?.projects.find((x) => x.projectId === renaming) ?? null) : null;
  const workspaces = useMemo(() => workspaceChoices(agents.headers, projects?.projects ?? [], agents.headers.find((h) => h.taskId === selectedTaskId)?.workspaceRoot), [agents.headers, projects?.projects, selectedTaskId]);

  // ---- stored views: a per-viewer saved query (grouping, chips, sort).
  const applyView = useCallback((v: StoredView) => update((p) => ({ ...p, grouping: v.grouping, filters: { ...v.filters }, sort: v.sort })), [update]);
  const onSaveView = (name: string): string | null => {
    const r = saveView(prefs.views, name, { grouping: prefs.grouping, filters: prefs.filters, sort: prefs.sort }, newViewId);
    if (!r.ok) return r.message;
    update((p) => ({ ...p, views: r.views }));
    setSavingView(false);
    return null;
  };

  const onPin = (taskId: string) => {
    const r = pinTask(prefs.pins, taskId);
    setNote(r.message);
    if (!r.message) update((p) => ({ ...p, pins: r.pins }));
  };

  return (
    <>
      <AgentListView
        headers={agents.headers}
        loaded={agents.loaded}
        error={agents.error}
        selectedId={selectedTaskId}
        pins={prefs.pins}
        grouping={prefs.grouping}
        filters={prefs.filters}
        subtitleFields={prefs.subtitle}
        collapsed={collapsed}
        nowMs={nowMs}
        query={query}
        search={search}
        note={note}
        focusRequest={focusRequest}
        onQuery={setQuery}
        onSelect={(id, rowId) => onSelect(id, rowId)}
        onPin={onPin}
        onUnpin={(id) => update((p) => ({ ...p, pins: unpinTask(p.pins, id) }))}
        onArchive={requestArchive}
        onToggleSection={(key) => update((p) => ({ ...p, collapsed: { ...p.collapsed, [p.grouping]: (p.collapsed[p.grouping] ?? []).includes(key) ? (p.collapsed[p.grouping] ?? []).filter((k) => k !== key) : [...(p.collapsed[p.grouping] ?? []), key] } }))}
        onGrouping={(g: Grouping) => update((p) => ({ ...p, grouping: g }))}
        onFilters={(f: Filters) => update((p) => ({ ...p, filters: f }))}
        onSubtitleFields={(s: SubtitleField[]) => update((p) => ({ ...p, subtitle: s }))}
        sort={prefs.sort}
        onSort={(s: SortKey) => update((p) => ({ ...p, sort: s }))}
        viewActions={{ views: prefs.views, onApply: applyView, onSaveRequest: () => setSavingView(true), onDelete: (id) => update((p) => ({ ...p, views: deleteView(p.views, id) })) }}
        projects={projects?.projects}
        projectActions={
          projects
            ? {
                selectedId: selectedProjectId,
                onNew: () => setNewProject(true),
                onOpen: (id) => onOpenProject?.(id),
                onRename: (id) => setRenaming(id),
                onArchive: (id) => void archiveProject(id, true),
                onRestore: (id) => void archiveProject(id, false),
                onAdd: (t, p) => void addToProject(t, p),
                onRemove: (t, p) => void removeFromProject(t, p),
              }
            : undefined
        }
      />
      {projects && (
        <>
          <NewProjectDialog
            open={newProject}
            workspaces={workspaces}
            onClose={() => setNewProject(false)}
            onCreate={async (input) => {
              if (!sessionId) throw new Error("NO_SESSION: there is no session to make the project in");
              const r = await projects.create(sessionId, input);
              setNewProject(false);
              onOpenProject?.(r.project.projectId);
            }}
          />
          <RenameProjectDialog
            project={renamingProject}
            onClose={() => setRenaming(null)}
            onRename={async (patch) => {
              if (!sessionId || !renaming) return;
              await projects.rename(sessionId, renaming, patch);
              setRenaming(null);
            }}
          />
        </>
      )}
      <SaveViewDialog open={savingView} onSave={onSaveView} onClose={() => setSavingView(false)} />
      <Dialog open={confirmStop !== null} title="Archive a running task?" role="alertdialog" onClose={() => setConfirmStop(null)} testId="archive-confirm">
        <p>
          “{confirmStop?.title}” is still running. Archiving it stops the run first, as a recorded cancellation. You can restore the conversation afterwards, but the run does not restart.
        </p>
        <div className="dialog-actions">
          <Button onClick={() => setConfirmStop(null)} data-testid="archive-confirm-cancel">
            Keep running
          </Button>
          <Button
            variant="danger"
            data-testid="archive-confirm-stop"
            onClick={() => {
              const c = confirmStop;
              setConfirmStop(null);
              if (c) void doArchive(c.taskId, c.title, true);
            }}
          >
            Stop and archive
          </Button>
        </div>
      </Dialog>
    </>
  );
}
