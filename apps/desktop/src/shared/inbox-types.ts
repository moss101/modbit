/** REQ-PX-206: what the bridge says of Inbox's record of updates (main's `inbox-record.ts`). */
export type UpdateKind = "READY_FOR_REVIEW" | "CHILD_FINISHED" | "AUTOMATION_RAN" | "FAILED";

export interface UpdateView {
  id: string;
  kind: UpdateKind;
  taskId: string;
  title: string;
  atMs: number;
  offset: string;
  /** The Core's read state: the task's read offset has reached this update's offset. */
  read: boolean;
}

export interface UpdatesView {
  /** Newest first. */
  updates: UpdateView[];
  unread: number;
  /** How many updates the record keeps. */
  cap: number;
}
