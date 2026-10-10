/**
 * The projects' data: the Core's ListProjects, kept current by the event
 * stream. A project event or any event of a task schedules one coalesced
 * read, so a Core-recorded change shows on the list within a frame or two of
 * the read; the renderer recomputes nothing and keeps no membership of its
 * own (a mutation's answer is the Core's project, and the next read is the
 * truth). The list also reads again when the Core (re)connects.
 */
import { useCallback, useEffect, useRef, useState } from "react";
import type { ProjectChangedInfo, ProjectInfo } from "../../shared/project-types.ts";

interface WireEventLike {
  eventType?: string;
  aggregateType?: string;
  taskId?: string | null;
}

/** Events that cannot change a project's facts or rollup. */
const IGNORED_EVENT_TYPES = new Set(["AssistantTextDelta"]);

export interface ProjectsState {
  projects: ProjectInfo[];
  loaded: boolean;
  error: string | null;
  refresh: () => void;
  /** Read once now (after a command, so the answer is the Core's, not a guess). */
  reload: () => Promise<void>;
  create: (sessionId: string, input: { name: string; color?: string; icon?: string; workspaceRoot: string }) => Promise<ProjectChangedInfo>;
  rename: (sessionId: string, projectId: string, patch: { name?: string; color?: string; icon?: string }) => Promise<ProjectChangedInfo>;
  archive: (sessionId: string, projectId: string, archived: boolean) => Promise<ProjectChangedInfo>;
  addMember: (sessionId: string, projectId: string, taskId: string) => Promise<ProjectChangedInfo>;
  removeMember: (sessionId: string, projectId: string, taskId: string) => Promise<ProjectChangedInfo>;
}

export function useProjects(connected: boolean): ProjectsState {
  const [projects, setProjects] = useState<ProjectInfo[]>([]);
  const [loaded, setLoaded] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const inFlight = useRef(false);
  const again = useRef(false);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const alive = useRef(true);
  useEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
    };
  }, []);

  const run = useCallback(async () => {
    if (inFlight.current) {
      again.current = true;
      return;
    }
    inFlight.current = true;
    try {
      // Archived projects are read too: whether the list shows them is a viewer's filter, not the Core's.
      const v = await window.modbit.listProjects({ includeArchived: true });
      if (alive.current) {
        setProjects(v.projects);
        setLoaded(true);
        setError(null);
      }
    } catch (e) {
      if (alive.current) setError((e as Error).message);
    } finally {
      inFlight.current = false;
      if (again.current) {
        again.current = false;
        void run();
      }
    }
  }, []);

  const refresh = useCallback(() => {
    if (timer.current) return;
    timer.current = setTimeout(() => {
      timer.current = null;
      void run();
    }, 30);
  }, [run]);

  useEffect(() => {
    if (connected) void run();
  }, [connected, run]);

  useEffect(() => {
    const off = window.modbit.onEvent((raw) => {
      const e = raw as WireEventLike;
      const mine = e.aggregateType === "project";
      if (!mine && (!e.taskId || (e.eventType && IGNORED_EVENT_TYPES.has(e.eventType)))) return;
      refresh();
    });
    return () => {
      off();
      if (timer.current) clearTimeout(timer.current);
      timer.current = null;
    };
  }, [refresh]);

  const after = useCallback(
    async (p: Promise<ProjectChangedInfo>): Promise<ProjectChangedInfo> => {
      const r = await p;
      refresh();
      return r;
    },
    [refresh],
  );
  const cid = () => {
    const b = new Uint8Array(16);
    crypto.getRandomValues(b);
    return Array.from(b, (x) => x.toString(16).padStart(2, "0")).join("");
  };

  return {
    projects,
    loaded,
    error,
    refresh,
    reload: run,
    create: (sid, input) => after(window.modbit.createProject(sid, input, cid())),
    rename: (sid, id, patch) => after(window.modbit.renameProject(sid, id, patch, cid())),
    archive: (sid, id, archived) => after(window.modbit.archiveProject(sid, id, archived, cid())),
    addMember: (sid, id, tid) => after(window.modbit.addProjectMember(sid, id, tid, cid())),
    removeMember: (sid, id, tid) => after(window.modbit.removeProjectMember(sid, id, tid, cid())),
  };
}
