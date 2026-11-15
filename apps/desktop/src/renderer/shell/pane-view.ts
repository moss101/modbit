/**
 * What a pane is showing, kept per task and pane in this window (memory only), so a pane that moves between the dock and a window
 * of its own (REQ-PX-203) comes back as it was: the file chosen in Changes and how far its list is scrolled. The terminal a pane
 * shows is the shell's terminal pick and travels the same way. View choices only: nothing here is a fact of the task, and a pane
 * that finds nothing here starts as it always did.
 */
import type { PaneViewStateDto } from "../../preload/preload.ts";

const views = new Map<string, PaneViewStateDto>();
const key = (taskId: string, kind: string) => `${taskId}:${kind}`;

export const paneView = {
  get(taskId: string, kind: string): PaneViewStateDto {
    return views.get(key(taskId, kind)) ?? {};
  },
  set(taskId: string, kind: string, patch: PaneViewStateDto): void {
    const k = key(taskId, kind);
    const was = views.get(k);
    views.delete(k);
    views.set(k, { ...was, ...patch });
    // Bounded: the oldest go first.
    while (views.size > 256) views.delete(views.keys().next().value as string);
  },
  /** Replaces what is kept for a pane (what came back from its window). */
  put(taskId: string, kind: string, state: PaneViewStateDto): void {
    views.delete(key(taskId, kind));
    views.set(key(taskId, kind), { ...state });
  },
};

/** Called whenever a pane's view changes, where someone must hear of it (a pane window tells the main process). */
let listener: ((taskId: string, kind: string, state: PaneViewStateDto) => void) | null = null;
export function onPaneViewChange(fn: typeof listener): void {
  listener = fn;
}
export function notePaneView(taskId: string, kind: string, patch: PaneViewStateDto): void {
  paneView.set(taskId, kind, patch);
  listener?.(taskId, kind, paneView.get(taskId, kind));
}
