import { useEffect, useId, useMemo, useRef, useState, type KeyboardEvent, type ReactNode } from "react";
import { Dialog, Icon, Kbd } from "@modbit/ui";
import { rank } from "@modbit/ui/logic";
import { highlightRuns } from "../agents/snippets.ts";
import { matchFiles, usePaletteSearch, type FileWalk, type SearchState } from "./palette-search.ts";

export type PaletteKind = "agent" | "conversation" | "file" | "action" | "setting";

export interface PaletteItem {
  id: string;
  kind: PaletteKind;
  title: string;
  /** A second line: the task's state, a group, the reason a command is unavailable. */
  detail?: string | undefined;
  keywords?: readonly string[] | undefined;
  /** A chord hint (display only). */
  chord?: string | undefined;
  /** A reason string disables the item and is shown instead of running it. */
  disabled?: string | undefined;
  run: () => void;
  /** Alt+Enter: a task opens beside the one in the centre (MWS-11). Absent where the item has no second way to open. */
  runAlt?: (() => void) | undefined;
  /** The title as runs, the matched ones marked (a conversation's matched text, a file's path). */
  runs?: readonly { text: string; match: boolean }[] | undefined;
  /** Attributes the item carries for what it opens. */
  data?: Readonly<Record<string, string>> | undefined;
}

/** What the palette searches beyond its items, and how a result of each is opened. */
export interface PaletteSearchProps {
  /** The session whose conversations the Core searches; null before there is one. */
  sessionId: string | null;
  /** The task whose folder's files are searched: the task open in the centre once it has a workspace; null otherwise. */
  fileTask: { taskId: string; title: string } | null;
  /** Opens a conversation at the row the match is in. */
  onOpenMatch: (taskId: string, rowId: string) => void;
  /** Shows a file of the open task in its Files pane. */
  onOpenFile: (taskId: string, path: string) => void;
}

type GroupKey = "tasks" | "conversations" | "files" | "commands";
const GROUPS: { key: GroupKey; label: string; kinds: readonly PaletteKind[] }[] = [
  { key: "tasks", label: "Tasks", kinds: ["agent"] },
  { key: "conversations", label: "In conversations", kinds: ["conversation"] },
  { key: "files", label: "Files", kinds: ["file"] },
  { key: "commands", label: "Commands", kinds: ["action", "setting"] },
];
const ICON: Record<PaletteKind, Parameters<typeof Icon>[0]["name"]> = { agent: "layers", conversation: "at-sign", file: "file-text", action: "chevron-right", setting: "settings" };

/**
 * The command palette (AFW-A15; DR-PX-2026-11-11-021 MWS-15, REQ-PX-209): one search over tasks by title, the text of every
 * task's conversation (the Core's search), the files of the open task's folder (the Core's listing) and the commands, in one
 * list with a heading per kind. Typing filters and ranks within each kind; ArrowUp/Down move the active option
 * (aria-activedescendant, focus stays in the field); Enter opens it, Alt+Enter opens a task beside the one in the centre;
 * Escape (the layer stack) closes it and focus returns to where it was.
 */
export function CommandPalette({ open, items, onClose, announce, search }: { open: boolean; items: readonly PaletteItem[]; onClose: () => void; announce: (text: string) => void; search: PaletteSearchProps }) {
  const input = useRef<HTMLInputElement>(null);
  const pending = useRef<(() => void) | null>(null);
  // The chosen command runs once the dialog has closed and returned focus, so a command that moves focus wins.
  useEffect(() => {
    if (open || !pending.current) return;
    const run = pending.current;
    pending.current = null;
    run();
  }, [open]);
  return (
    <Dialog open={open} title="Command palette" hideTitle onClose={onClose} initialFocus={input} testId="palette" className="palette">
      <PaletteBody items={items} onClose={onClose} announce={announce} input={input} pending={pending} search={search} />
    </Dialog>
  );
}

/** What is searched, in words: the footer says it, and the empty state says it was searched. */
export function scopeText(fileTask: { title: string } | null, walk: FileWalk): string {
  const files = fileTask === null ? "no files (no task with a folder is open)" : walk.error ? `no files (the folder of “${fileTask.title}” could not be listed: ${walk.error})` : `the files of “${fileTask.title}”${walk.partial ? " as far as the listing reached" : ""}`;
  return `task titles, every task’s conversation, ${files} and the commands`;
}

function runsOfPath(path: string, at: number, length: number): { text: string; match: boolean }[] {
  return [
    { text: path.slice(0, at), match: false },
    { text: path.slice(at, at + length), match: true },
    { text: path.slice(at + length), match: false },
  ].filter((r) => r.text.length > 0);
}

function PaletteBody({ items, onClose, announce, input, pending, search }: { items: readonly PaletteItem[]; onClose: () => void; announce: (text: string) => void; input: React.RefObject<HTMLInputElement | null>; pending: React.RefObject<(() => void) | null>; search: PaletteSearchProps }) {
  const [query, setQuery] = useState("");
  const [activeId, setActiveId] = useState<string | null>(null);
  const listId = useId();
  const { sessionId, fileTask, onOpenMatch, onOpenFile } = search;
  const found = usePaletteSearch(query, sessionId, fileTask?.taskId ?? null);
  const q = query.trim();

  const groups = useMemo(() => {
    const ranked = rank(query, items);
    const of = (kinds: readonly PaletteKind[]) => ranked.filter((i) => kinds.includes(i.kind));
    const conversations: PaletteItem[] = q
      ? found.conversations.map((m) => ({
          id: `conversation:${m.taskId}:${m.rowId}`,
          kind: "conversation",
          title: m.text,
          runs: [...(m.cutBefore ? [{ text: "…", match: false }] : []), ...highlightRuns(m.text, m.matches), ...(m.cutAfter ? [{ text: "…", match: false }] : [])],
          detail: m.taskTitle || "Untitled task",
          data: { "task-id": m.taskId, "row-id": m.rowId },
          run: () => onOpenMatch(m.taskId, m.rowId),
        }))
      : [];
    const files: PaletteItem[] =
      q && fileTask
        ? matchFiles(q, found.files.paths).map((m) => ({
            id: `file:${m.path}`,
            kind: "file",
            title: m.path,
            runs: runsOfPath(m.path, m.at, m.length),
            detail: fileTask.title,
            data: { path: m.path, "task-id": fileTask.taskId },
            run: () => onOpenFile(fileTask.taskId, m.path),
          }))
        : [];
    const byKey: Record<GroupKey, PaletteItem[]> = { tasks: of(["agent"]), conversations, files, commands: of(["action", "setting"]) };
    return GROUPS.map((g) => ({ ...g, items: byKey[g.key] })).filter((g) => g.items.length > 0);
  }, [query, q, items, found.conversations, found.files.paths, fileTask, onOpenMatch, onOpenFile]);
  const results = useMemo(() => groups.flatMap((g) => g.items), [groups]);
  // The active option is kept by what it is, so results that arrive from the Core while it is chosen do not move it.
  const byId = results.findIndex((r) => r.id === activeId);
  const at = byId >= 0 ? byId : 0;
  const activeItem = results[at];
  const move = (i: number) => setActiveId(results[i]?.id ?? null);

  const choose = (item: PaletteItem | undefined, alt = false) => {
    if (!item) return;
    if (item.disabled) {
      announce(`${item.title} is unavailable: ${item.disabled}`);
      return;
    }
    pending.current = alt && item.runAlt ? item.runAlt : item.run;
    onClose();
  };
  const onKey = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      move(results.length === 0 ? 0 : (at + 1) % results.length);
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      move(results.length === 0 ? 0 : (at - 1 + results.length) % results.length);
    } else if (e.key === "Home") {
      e.preventDefault();
      move(0);
    } else if (e.key === "End") {
      e.preventDefault();
      move(Math.max(0, results.length - 1));
    } else if (e.key === "Enter") {
      e.preventDefault();
      choose(activeItem, e.altKey);
    }
  };
  const searching: SearchState = found.conversationState;
  const scope = scopeText(fileTask, found.files);
  const indexing = fileTask !== null && !found.files.done && q.length > 0;
  const optionId = (item: PaletteItem) => `${listId}-${item.id.replace(/[^A-Za-z0-9_-]/g, "_")}`;
  return (
    <>
      <div className="palette-field">
        <span className="palette-field-icon" aria-hidden="true">
          <Icon name="search" />
        </span>
        <input
          ref={input}
          type="text"
          role="combobox"
          aria-expanded="true"
          aria-controls={listId}
          aria-autocomplete="list"
          aria-activedescendant={activeItem ? optionId(activeItem) : undefined}
          aria-label="Search tasks, conversations, files and commands"
          placeholder="Search tasks, conversations, files and commands"
          className="palette-input"
          data-testid="palette-input"
          value={query}
          onChange={(e) => {
            setQuery(e.target.value);
            setActiveId(null);
          }}
          onKeyDown={onKey}
          autoComplete="off"
          spellCheck={false}
        />
        <Kbd>Esc</Kbd>
      </div>
      <div id={listId} role="listbox" aria-label="Results" className="palette-list" data-testid="palette-list" aria-busy={searching === "searching" || indexing}>
        {groups.map((g) => (
          <div key={g.key} role="group" aria-labelledby={`${listId}-h-${g.key}`} className="palette-group" data-testid="palette-group" data-group={g.key}>
            <div role="presentation" id={`${listId}-h-${g.key}`} className="palette-head">
              {g.label}
            </div>
            {g.items.map((item) => {
              const i = results.indexOf(item);
              const extra = Object.fromEntries(Object.entries(item.data ?? {}).map(([k, v]) => [`data-${k}`, v]));
              return (
                <div
                  key={item.id}
                  id={optionId(item)}
                  role="option"
                  aria-selected={i === at}
                  aria-disabled={item.disabled ? true : undefined}
                  className="palette-option"
                  data-testid="palette-option"
                  data-kind={item.kind}
                  data-id={item.id}
                  data-active={i === at}
                  {...extra}
                  onPointerMove={() => i !== at && setActiveId(item.id)}
                  onClick={(e) => choose(item, e.altKey)}
                >
                  <span className="palette-icon" aria-hidden="true">
                    <Icon name={ICON[item.kind]} />
                  </span>
                  <span className={`palette-title${item.kind === "file" ? " palette-mono" : ""}`} data-testid="palette-title">
                    {item.runs ? item.runs.map((r, k): ReactNode => (r.match ? <mark key={k}>{r.text}</mark> : <span key={k}>{r.text}</span>)) : item.title}
                  </span>
                  {(item.disabled ?? item.detail) && <span className="palette-detail">{item.disabled ? `unavailable: ${item.disabled}` : item.detail}</span>}
                  {item.chord && <Kbd chord={item.chord} />}
                </div>
              );
            })}
          </div>
        ))}
      </div>
      {results.length === 0 && (
        <p className="palette-empty" role="status" data-testid="palette-empty">
          {searching === "searching" || indexing ? `Looking for “${q}”…` : `Nothing matches “${q}” in ${scope}.`}
        </p>
      )}
      {found.conversationState === "error" && (
        <p className="palette-empty" role="status" data-testid="palette-search-error">
          The conversation search failed: {found.conversationError}
        </p>
      )}
      <div className="palette-foot" data-testid="palette-foot">
        <span className="palette-hint">
          <Kbd>↑</Kbd>
          <Kbd>↓</Kbd> move
        </span>
        <span className="palette-hint">
          <Kbd>↵</Kbd> open
        </span>
        {activeItem?.runAlt && !activeItem.disabled && (
          <span className="palette-hint" data-testid="palette-hint-split">
            <Kbd chord="alt+enter" /> open in split view
          </span>
        )}
        <span className="palette-scope" data-testid="palette-scope">
          Searches {scope}
        </span>
      </div>
    </>
  );
}
