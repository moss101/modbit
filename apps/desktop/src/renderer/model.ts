/**
 * Renderer view model: an ordered event reducer (docs/32 "Event consumption").
 * State is advisory; every fact comes from a Core snapshot or a Core event.
 * A task card never shows "Completed" unless the Core emitted TaskCompleted.
 */
export type FleetColumn = "needsAttention" | "readyForReview" | "running" | "waiting" | "completed" | "failed";

export interface TaskCard {
  taskId: string;
  goalText: string;
  state: string;
  waitReason: string | null;
  generation: number;
  createdAtMs: number;
  nextAction: string | null;
  attachments: number;
}

export interface Snapshot {
  sessionId: string;
  state: string;
  generation: number;
  lastOffset: string;
  tasks: { taskId: string; goalText: string; state: string; generation: number; createdAtMs: number }[];
}

export interface Event {
  offset: string;
  sequence: string;
  eventType: string;
  sessionId: string;
  payload: unknown;
  occurredAtMs: number;
}

export interface Model {
  sessionId: string | null;
  cursor: string;
  tasks: Map<string, TaskCard>;
  /** Task ids whose TaskCreated event we have seen, keyed by created goal (for cards created before the event arrives). */
  seenOffsets: Set<string>;
}

export function emptyModel(): Model {
  return { sessionId: null, cursor: "0", tasks: new Map(), seenOffsets: new Set() };
}

export function fromSnapshot(s: Snapshot): Model {
  const m = emptyModel();
  m.sessionId = s.sessionId;
  m.cursor = s.lastOffset;
  for (const t of s.tasks) {
    m.tasks.set(t.taskId, { taskId: t.taskId, goalText: t.goalText, state: normalizeState(t.state), waitReason: waitReasonOf(t.state), generation: t.generation, createdAtMs: t.createdAtMs, nextAction: null, attachments: 0 });
  }
  return m;
}

function normalizeState(s: string): string {
  // Core renders `Waiting(Approval)` in Debug form; keep the outer state.
  const m = /^Waiting\((\w+)\)$/.exec(s);
  return m ? "Waiting" : s;
}

function waitReasonOf(s: string): string | null {
  const m = /^Waiting\((\w+)\)$/.exec(s);
  return m ? m[1]! : null;
}

/** The task id an event belongs to. Task events carry no aggregate id on the
 *  wire yet, so TaskCreated carries it in the ack; we key by the payload's
 *  correlation: the renderer learns task ids from CreateTask acks and
 *  snapshots, and applies state events in order per task via `taskId` when
 *  present in the event, else to the most recent task whose sequence matches. */
export function applyEvent(m: Model, e: Event, taskIdHint?: string): Model {
  if (m.seenOffsets.has(e.offset)) return m;
  const next: Model = { ...m, tasks: new Map(m.tasks), seenOffsets: new Set(m.seenOffsets) };
  next.seenOffsets.add(e.offset);
  if (BigInt(e.offset) > BigInt(next.cursor)) next.cursor = e.offset;
  const p = (e.payload ?? {}) as Record<string, unknown>;
  const target = taskIdHint ?? (typeof p["task_id"] === "string" ? (p["task_id"] as string) : undefined);
  switch (e.eventType) {
    case "TaskCreated": {
      if (!target) return next;
      next.tasks.set(target, {
        taskId: target,
        goalText: String(p["goal_text"] ?? ""),
        state: "Created",
        waitReason: null,
        generation: 1,
        createdAtMs: e.occurredAtMs,
        nextAction: null,
        attachments: 0,
      });
      return next;
    }
    default: {
      if (!target) return next;
      const card = next.tasks.get(target);
      if (!card) return next;
      const updated = { ...card, generation: card.generation + 1 };
      switch (e.eventType) {
        case "TaskQueued":
          updated.state = "Queued";
          updated.waitReason = "Capacity";
          break;
        case "TaskStarted":
        case "TaskResumed":
        case "TaskReturnedToWork":
          updated.state = "Running";
          updated.waitReason = null;
          updated.nextAction = null;
          break;
        case "TaskWaiting":
          updated.state = "Waiting";
          updated.waitReason = String(p["reason"] ?? "External");
          updated.nextAction = updated.waitReason === "Approval" ? "Approve or deny the pending effect" : updated.waitReason === "UserInput" ? "Answer the agent's question" : null;
          break;
        case "TaskReadyForReview":
          updated.state = "ReadyForReview";
          updated.nextAction = "Review the result";
          break;
        case "TaskCompleted":
          updated.state = "Completed";
          updated.nextAction = null;
          break;
        case "TaskFailed":
          updated.state = "Failed";
          updated.nextAction = String(p["failure_code"] ?? "");
          break;
        case "TaskCancelled":
          updated.state = "Cancelled";
          break;
        case "AttachmentIngested":
          updated.attachments = (card.attachments ?? 0) + 1;
          break;
        case "TaskNeedsAttention":
          updated.nextAction = String(p["reason"] ?? "Needs attention");
          break;
        default:
          return next;
      }
      next.tasks.set(target, updated);
      return next;
    }
  }
}

/** PRD "Home / Fleet" columns from card state. Needs Attention is derived
 *  from structured reasons, never from logs. */
export function columnOf(c: TaskCard): FleetColumn {
  if (c.nextAction && (c.state === "Waiting" || c.state === "Running")) return "needsAttention";
  switch (c.state) {
    case "ReadyForReview":
      return "readyForReview";
    case "Running":
      return "running";
    case "Completed":
      return "completed";
    case "Failed":
    case "Cancelled":
      return "failed";
    default:
      return "waiting"; // Created, Queued, Waiting(no user action)
  }
}

export function columns(m: Model): Record<FleetColumn, TaskCard[]> {
  const out: Record<FleetColumn, TaskCard[]> = { needsAttention: [], readyForReview: [], running: [], waiting: [], completed: [], failed: [] };
  for (const c of [...m.tasks.values()].sort((a, b) => b.createdAtMs - a.createdAtMs || a.taskId.localeCompare(b.taskId))) out[columnOf(c)].push(c);
  return out;
}
