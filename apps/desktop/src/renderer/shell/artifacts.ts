/**
 * Which typed apps a task can show in the apps panel (AFW-A01, AFW-I01). The
 * panel is hidden until a task has an artifact. Today a task has artifacts
 * only where the Core already produced something to look at: a diff to review
 * (the task reached review, or completed) and a live browser session. The
 * Terminal, Files and Evidence apps are declared so the registry, the
 * shortcuts and the per-task tab memory are complete, and are filled by
 * PX-048; until then they have no artifacts and say so.
 */
import type { AppKind } from "./prefs.ts";

export interface TaskLike {
  taskId: string;
  state: string;
}

export interface ArtifactContext {
  /** The task whose browser session is open, if any. */
  browsingTaskId: string | null;
}

/** The apps with something to show for this task, in tab order. */
export function artifactsOf(task: TaskLike | undefined, ctx: ArtifactContext): AppKind[] {
  if (!task) return [];
  const out: AppKind[] = [];
  if (task.state === "ReadyForReview" || task.state === "Completed") out.push("changes");
  if (ctx.browsingTaskId === task.taskId) out.push("browser");
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

/** What an app without a producer says (honest placeholder; PX-048 replaces it). */
export const APP_PENDING: Partial<Record<AppKind, string>> = {
  terminal: "The terminal app arrives with PX-048; no terminal stream is wired to this panel yet.",
  files: "The read-only files app arrives with PX-048.",
  evidence: "The evidence app arrives with PX-048.",
};
