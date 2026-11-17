/**
 * The palette's two searches that are not over what the window already holds (DR-PX-2026-11-11-021 MWS-15, REQ-PX-209):
 * conversation text, through the Core's search (`searchConversations`, the index the agent list and the composer's @ menu
 * already read), and the files of the open task's folder, through the Core's directory listing (`listWorkspaceDir`, the read
 * the Files pane and the @ menu already make). Neither is an index of the renderer's own: the conversation text is never read
 * here, only the Core's hits; the file names are what the Core listed under its path policy, walked folder by folder and kept
 * while the palette is open. What is typed goes only to the Core's search; the listing is asked for folders, never for the text.
 */
import { useEffect, useRef, useState } from "react";
import type { WorkspaceEntry } from "../../preload/preload.ts";
import type { ConversationHitView } from "../../shared/conversation-types.ts";

/** How far one walk of the open task's folder goes: folders listed and file names kept. */
export const WALK_BOUND = { dirs: 400, files: 20_000 } as const;
/** Folders the walk does not enter: Git's own store and installed packages are not the task's files to open by name. */
export const WALK_SKIP: ReadonlySet<string> = new Set([".git", "node_modules"]);
/** At most this many results of each searched kind. */
export const KIND_LIMIT = 8;
/** The Core is asked once typing pauses, and only for two characters or more. */
export const SEARCH_DEBOUNCE_MS = 150;
export const SEARCH_MIN = 2;

type ListDir = (taskId: string, path: string) => Promise<{ entries: WorkspaceEntry[]; truncated: boolean }>;

export interface FileWalk {
  /** Every file path the Core listed so far, workspace-relative, in the order the walk met them. */
  paths: string[];
  done: boolean;
  /** The walk stopped at its bound, or a listing was cut at the Core's: some files were not looked at. */
  partial: boolean;
  error: string | null;
}

export const EMPTY_WALK: FileWalk = { paths: [], done: false, partial: false, error: null };

/**
 * Walks the task's folder breadth first through the Core's listing. Only files the Core lists and does not protect are kept;
 * protected folders are not entered. `onProgress` gets the growing list after each folder; `cancelled` stops the walk.
 */
export async function walkFolder(taskId: string, list: ListDir, onProgress: (w: FileWalk) => void, cancelled: () => boolean, bound: { dirs: number; files: number } = WALK_BOUND): Promise<FileWalk> {
  const paths: string[] = [];
  const queue: string[] = [""];
  let dirs = 0;
  let partial = false;
  while (queue.length > 0) {
    if (cancelled()) return { paths, done: false, partial, error: null };
    if (dirs >= bound.dirs || paths.length >= bound.files) {
      partial = true;
      break;
    }
    const dir = queue.shift()!;
    dirs++;
    let l: { entries: WorkspaceEntry[]; truncated: boolean };
    try {
      l = await list(taskId, dir);
    } catch (e) {
      // The root refused: there is nothing to search. A folder below it refused: the others are still searched.
      if (dir === "") return { paths, done: true, partial, error: errText(e) };
      partial = true;
      continue;
    }
    if (l.truncated) partial = true;
    for (const e of l.entries) {
      if (e.protected) continue;
      if (e.kind === "dir" && !WALK_SKIP.has(e.name)) queue.push(e.path);
      else if (e.kind === "file" && paths.length < bound.files) paths.push(e.path);
    }
    if (!cancelled()) onProgress({ paths: [...paths], done: false, partial, error: null });
  }
  return { paths, done: true, partial, error: null };
}

export interface FileMatch {
  path: string;
  /** Where the typed text is in the path (UTF-16 offsets), for the highlight. */
  at: number;
  length: number;
}

const baseOf = (p: string): string => p.slice(p.lastIndexOf("/") + 1);

/**
 * The files whose name or path holds the typed text, best first: a name that starts with it, a name that holds it, then a
 * path that holds it; shorter paths first within each. Case is ignored. No match on scattered letters: a file is found by name.
 */
export function matchFiles(query: string, paths: readonly string[], limit = KIND_LIMIT): FileMatch[] {
  const q = query.trim().toLowerCase();
  if (q.length === 0) return [];
  const scored: { m: FileMatch; score: number }[] = [];
  for (const path of paths) {
    const lower = path.toLowerCase();
    const base = baseOf(lower);
    const inBase = base.indexOf(q);
    if (inBase >= 0) {
      scored.push({ m: { path, at: lower.length - base.length + inBase, length: q.length }, score: inBase === 0 ? 3 : 2 });
      continue;
    }
    const inPath = lower.indexOf(q);
    if (inPath >= 0) scored.push({ m: { path, at: inPath, length: q.length }, score: 1 });
  }
  scored.sort((a, b) => b.score - a.score || a.m.path.length - b.m.path.length || (a.m.path < b.m.path ? -1 : a.m.path > b.m.path ? 1 : 0));
  return scored.slice(0, limit).map((s) => s.m);
}

/** One conversation result: the hit's first snippet (the Core's best), with the task it is in. A title-only hit is a task result, not this. */
export interface ConversationMatch {
  taskId: string;
  taskTitle: string;
  rowId: string;
  text: string;
  cutBefore: boolean;
  cutAfter: boolean;
  matches: { start: number; end: number }[];
}

export function conversationMatches(hits: readonly ConversationHitView[], limit = KIND_LIMIT): ConversationMatch[] {
  const out: ConversationMatch[] = [];
  for (const h of hits) {
    const s = h.snippets[0];
    if (!s) continue;
    out.push({ taskId: h.taskId, taskTitle: h.title, rowId: s.rowId, text: s.text, cutBefore: s.cutBefore, cutAfter: s.cutAfter, matches: s.matches });
    if (out.length >= limit) break;
  }
  return out;
}

export type SearchState = "idle" | "searching" | "done" | "error";

export interface PaletteSearchResult {
  conversations: ConversationMatch[];
  conversationState: SearchState;
  conversationError: string | null;
  files: FileWalk;
}

const errText = (e: unknown): string => ((e as Error)?.message ?? String(e)).replace(/^Error invoking remote method '[^']*': (Error: )?/, "");

/**
 * The palette's searches for what is typed: the Core's conversation search (debounced; a newer query supersedes an older one)
 * and the walk of the open task's folder (once per palette, started when it opens).
 */
export function usePaletteSearch(query: string, sessionId: string | null, fileTaskId: string | null): PaletteSearchResult {
  const [walk, setWalk] = useState<FileWalk>(EMPTY_WALK);
  useEffect(() => {
    setWalk(EMPTY_WALK);
    if (!fileTaskId) return;
    let stop = false;
    void walkFolder(fileTaskId, (t, p) => window.modbit.listWorkspaceDir(t, p), (w) => setWalk(w), () => stop).then((w) => {
      if (!stop) setWalk(w);
    });
    return () => {
      stop = true;
    };
  }, [fileTaskId]);

  const [conv, setConv] = useState<{ state: SearchState; hits: ConversationMatch[]; error: string | null }>({ state: "idle", hits: [], error: null });
  const seq = useRef(0);
  useEffect(() => {
    const q = query.trim();
    const mine = ++seq.current;
    if (!sessionId || q.length < SEARCH_MIN) {
      setConv({ state: "idle", hits: [], error: null });
      return;
    }
    setConv((c) => ({ ...c, state: "searching" }));
    const t = setTimeout(() => {
      window.modbit
        .searchConversations(sessionId, q, { limit: KIND_LIMIT, maxSnippets: 1 })
        .then((r) => {
          if (mine === seq.current) setConv({ state: "done", hits: conversationMatches(r.hits), error: null });
        })
        .catch((e: unknown) => {
          if (mine === seq.current) setConv({ state: "error", hits: [], error: errText(e) });
        });
    }, SEARCH_DEBOUNCE_MS);
    return () => clearTimeout(t);
  }, [query, sessionId]);

  return { conversations: conv.hits, conversationState: conv.state, conversationError: conv.error, files: walk };
}
