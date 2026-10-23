/**
 * Which typed apps a task can show in the apps panel (AFW-A01, AFW-I01). The
 * panel is hidden until a task has an artifact. A task that has not started has
 * none. Once it has run it has a change set (the Core serves it in every task
 * state), a workspace to browse and evidence to read; a terminal appears when
 * the Core lists one for the task, and a browser when its session is open.
 */
import type { AppKind } from "./prefs.ts";

export interface TaskLike {
  taskId: string;
  state: string;
}

export interface ArtifactContext {
  /** The task whose browser session is open, if any. */
  browsingTaskId: string | null;
  /** The tasks the Core lists at least one terminal for. */
  terminalTaskIds?: ReadonlySet<string> | undefined;
}

/** States before the task has a workspace to show. */
const NOT_STARTED = new Set(["Created", "Queued"]);

/** The apps with something to show for this task, in tab order. */
export function artifactsOf(task: TaskLike | undefined, ctx: ArtifactContext): AppKind[] {
  if (!task) return [];
  const started = !NOT_STARTED.has(task.state);
  const out: AppKind[] = [];
  if (started) out.push("changes");
  if (ctx.terminalTaskIds?.has(task.taskId)) out.push("terminal");
  if (ctx.browsingTaskId === task.taskId) out.push("browser");
  if (started) out.push("files", "evidence");
  return out;
}

/** The task the panel shows: the selected one if it has artifacts, else the most recent task that does. */
export function panelTaskFor(tasks: readonly (TaskLike & { createdAtMs: number })[], selectedTaskId: string | null, ctx: ArtifactContext): string | null {
  const selected = tasks.find((t) => t.taskId === selectedTaskId);
  if (selected && artifactsOf(selected, ctx).length > 0) return selected.taskId;
  const withArtifacts = tasks.filter((t) => artifactsOf(t, ctx).length > 0).sort((a, b) => b.createdAtMs - a.createdAtMs);
  return withArtifacts[0]?.taskId ?? null;
}

export const APP_LABEL: Record<AppKind, string> = { changes: "Changes", terminal: "Terminal", browser: "Browser", files: "Files", evidence: "Evidence" };
