/**
 * Archiving a project with its undo (PX-064; the same pattern as archiving a
 * task, AFW-B08). Archive is a Core command; the toast it leaves offers the
 * Core's own reversal (ArchiveProject with archived = false), and says what
 * archiving did not do: it did not archive or stop a task. The undo stays up
 * longer than the 8 s the spec asks for.
 */
import { trays } from "@modbit/ui";
import type { ProjectsState } from "./use-projects.ts";
import { refusalSentence } from "./project-model.ts";

/** The undo stays at least this long (AFW-B08: not less than 8 s). */
export const PROJECT_UNDO_MS = 12_000;

const timers = new Map<string, ReturnType<typeof setTimeout>>();

export interface ArchiveHooks {
  /** A refusal or failure, as a sentence. */
  onError: (message: string) => void;
  /** The project's archived state changed (after the Core accepted it). */
  onChanged: (projectId: string, archived: boolean) => void;
}

export async function archiveProject(projects: ProjectsState, sessionId: string, projectId: string, name: string, archived: boolean, hooks: ArchiveHooks): Promise<boolean> {
  const id = `archive-project:${projectId}`;
  try {
    await projects.archive(sessionId, projectId, archived);
  } catch (e) {
    hooks.onError(refusalSentence(`Could not ${archived ? "archive" : "restore"} “${name}”`, e));
    return false;
  }
  hooks.onChanged(projectId, archived);
  const pending = timers.get(id);
  if (pending) clearTimeout(pending);
  timers.delete(id);
  if (!archived) {
    trays.dismiss(id);
    return true;
  }
  trays.present({
    id,
    tone: "info",
    title: `Archived the project “${name}”`,
    body: "Its tasks were not archived or stopped; they are listed where they were before. Undo brings the project back with its tasks.",
    actions: [
      {
        id: "undo",
        label: "Undo",
        primary: true,
        run: () => {
          void projects.archive(sessionId, projectId, false).then(
            () => {
              hooks.onChanged(projectId, false);
              trays.dismiss(id);
              const t = timers.get(id);
              if (t) clearTimeout(t);
              timers.delete(id);
            },
            (e: unknown) => hooks.onError(refusalSentence(`Could not undo archiving “${name}”`, e)),
          );
        },
      },
    ],
  });
  timers.set(
    id,
    setTimeout(() => {
      timers.delete(id);
      trays.dismiss(id);
    }, PROJECT_UNDO_MS),
  );
  return true;
}
