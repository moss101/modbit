/**
 * Renderer view model: an ordered event reducer (docs/32 "Event consumption").
 * State is advisory; every fact comes from a Core snapshot or a Core event.
 * A task card never shows "Completed" unless the Core emitted TaskCompleted.
 */
export type FleetColumn = "needsAttention" | "readyForReview" | "running" | "waiting" | "completed" | "failed";

/** The agents of a task as the Fleet counts them (M6.6): every node the
 *  AgentGraph recorded, by status. */
export interface AgentCounts {
  total: number;
  running: number;
  background: number;
  waiting: number;
  done: number;
  failed: number;
}

/** PRD "Home / Fleet" card phase: drafting, verifying, reviewing,
 *  escalating, awaiting human, waiting for capacity. Derived from Core
 *  events only. */
export type Phase = "drafting" | "verifying" | "reviewing" | "escalating" | "awaitingHuman" | "waitingCapacity" | "delegating" | "done";

export interface TaskCard {
  taskId: string;
  goalText: string;
  state: string;
  waitReason: string | null;
  generation: number;
  createdAtMs: number;
  nextAction: string | null;
  attachments: number;
  /** `subagent` cards nest under their parent (M6.6); null for a top-level task. */
  parentTaskId: string | null;
  /** Where the task came from (`cli`, `desktop`, `subagent`, …). */
  origin: string;
  /** Agents on this task, from the AgentGraph events. */
  agents: AgentCounts;
  /** Current phase. */
  phase: Phase;
  /** The latest evidence line (verification run, gate verdict, continuation). */
  latestEvidence: string | null;
  /** Realized risk level (`LOW` … `CRITICAL`) once derived. */
  risk: string | null;
  /** Child task ids, in admission order. */
  children: string[];
  /** The latest typed diagnostic the Core attached to a `TaskNeedsAttention`
   *  (REQ-EV-0073): class, code, what only the user can do, how the system
   *  recovers, and the evidence refs. Null until one exists (PX-023). */
  diagnostic: Diagnostic | null;
  /** The approval the task waits on, from the approval aggregate's events:
   *  the exact intent hash the decision binds (PX-023 "awaiting approval
   *  with the exact intent"). Null when none is open. */
  approval: { approvalId: string; toolName: string; effectClass: string; intentHash: string } | null;
  /** What confidence-adjusted feasibility said of the run's plan
   *  (`RoutingPlanAdmitted.feasibility`): `FEASIBLE`,
   *  `QUALITY_FLOOR_INFEASIBLE` or `QUALITY_FLOOR_UNKNOWN`. */
  feasibility: string | null;
  /** The open typed question (`UserQuestionAsked`, REQ-EV-0222): what the
   *  Task screen's "awaiting your answer" state offers. Null when none. */
  question: { questionId: string; text: string; options: { id: string; label: string }[]; allowFreeText: boolean } | null;
  /** The latest acceptance-gate verdict on the run (`AcceptanceGateEvaluated`,
   *  REQ-EPR-017): what the Review screen's INCONCLUSIVE and REJECT states
   *  name. Null until the gate has evaluated. */
  gate: { verdict: string; missingEvidence: string[]; rejectReasons: string[]; gateRef: string; humanRequired: boolean; candidateRevision: string } | null;
  /** Offsets of the events that put the card in its current state: the
   *  evidence reference a degraded state names. */
  lastOffset: string;
}

/** A typed diagnostic as the Core records it (docs/30 `TaskStatus`). */
export interface Diagnostic {
  class: string;
  code: string;
  detail: string;
  userAction: string;
  recoveryPath: string;
  retryable: boolean;
  evidenceRefs: string[];
}

function emptyCounts(): AgentCounts {
  return { total: 0, running: 0, background: 0, waiting: 0, done: 0, failed: 0 };
}

export interface Snapshot {
  sessionId: string;
  state: string;
  generation: number;
  lastOffset: string;
  tasks: { taskId: string; goalText: string; state: string; generation: number; createdAtMs: number; origin?: string; parentTaskId?: string | null }[];
}

export interface Event {
  offset: string;
  sequence: string;
  eventType: string;
  sessionId: string;
  payload: unknown;
  occurredAtMs: number;
  /** The aggregate the event is on (hex); the approval id for approval events. */
  aggregateId?: string;
  /** `task`, `run`, `approval`, … */
  aggregateType?: string;
}

export interface Model {
  sessionId: string | null;
  cursor: string;
  tasks: Map<string, TaskCard>;
  /** Task ids whose TaskCreated event we have seen, keyed by created goal (for cards created before the event arrives). */
  seenOffsets: Set<string>;
  /** Agent node status by agent id, per task (M6.6). */
  agents: Map<string, Map<string, string>>;
  /** Child task → parent task, learned from `SubagentAdmitted` (which may
   *  arrive before or after the child's own `TaskCreated`). */
  parents: Map<string, string>;
}

export function emptyModel(): Model {
  return { sessionId: null, cursor: "0", tasks: new Map(), seenOffsets: new Set(), agents: new Map(), parents: new Map() };
}

function freshCard(taskId: string, goalText: string, state: string, generation: number, createdAtMs: number, origin: string): TaskCard {
  return { taskId, goalText, state: normalizeState(state), waitReason: waitReasonOf(state), generation, createdAtMs, nextAction: null, attachments: 0, parentTaskId: null, origin, agents: emptyCounts(), phase: "drafting", latestEvidence: null, risk: null, children: [], diagnostic: null, approval: null, feasibility: null, question: null, gate: null, lastOffset: "0" };
}

export function fromSnapshot(s: Snapshot): Model {
  const m = emptyModel();
  m.sessionId = s.sessionId;
  m.cursor = s.lastOffset;
  for (const t of s.tasks) {
    m.tasks.set(t.taskId, freshCard(t.taskId, t.goalText, t.state, t.generation, t.createdAtMs, t.origin ?? ""));
  }
  // Children under their parents (M6.6): the snapshot names each
  // subagent task's parent.
  for (const t of s.tasks) {
    if (t.parentTaskId) {
      m.parents.set(t.taskId, t.parentTaskId);
      const child = m.tasks.get(t.taskId);
      if (child) m.tasks.set(t.taskId, { ...child, parentTaskId: t.parentTaskId });
      const parent = m.tasks.get(t.parentTaskId);
      if (parent) m.tasks.set(t.parentTaskId, { ...parent, children: withChild(parent.children, t.taskId), agents: { ...parent.agents } });
    }
  }
  return m;
}

function countsOf(statuses: Map<string, string> | undefined): AgentCounts {
  const c = emptyCounts();
  if (!statuses) return c;
  for (const st of statuses.values()) {
    c.total += 1;
    switch (st) {
      case "RUNNING":
      case "ADMITTED":
      case "ADMISSION_PENDING":
        c.running += 1;
        break;
      case "BACKGROUND":
        c.background += 1;
        break;
      case "WAITING":
      case "PARKED":
        c.waiting += 1;
        break;
      case "COMPLETED":
        c.done += 1;
        break;
      case "FAILED":
      case "CANCELLED":
        c.failed += 1;
        break;
      default:
        break;
    }
  }
  return c;
}

/** Link a child card to its parent once both are known: the parents map
 *  and the child's side; the parent's `children` is kept by the parent's
 *  own event arm (its card is rewritten there). */
function link(next: Model, childId: string, parentId: string): void {
  next.parents.set(childId, parentId);
  const child = next.tasks.get(childId);
  if (child && child.parentTaskId !== parentId) next.tasks.set(childId, { ...child, parentTaskId: parentId });
}

/** A task id as the renderer keys cards: 32 hex chars. Event payloads carry
 *  ids as dashed UUID text; envelopes and snapshots as bytes rendered hex. */
export function hexId(v: unknown): string {
  return typeof v === "string" ? v.replace(/-/g, "").toLowerCase() : "";
}

function withChild(children: string[], childId: string): string[] {
  return children.includes(childId) ? children : [...children, childId];
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

/** A wait reason as the wire spells it (`USER_INPUT`, the event's
 *  SCREAMING_SNAKE_CASE) or as a snapshot spells it (`UserInput`), to the
 *  one form the cards use. */
export function normalizeReason(r: string): string {
  if (!r.includes("_") && r !== r.toUpperCase()) return r;
  return r
    .toLowerCase()
    .split("_")
    .map((w) => (w ? w[0]!.toUpperCase() + w.slice(1) : ""))
    .join("");
}

/** The task id an event belongs to. Task events carry no aggregate id on the
 *  wire yet, so TaskCreated carries it in the ack; we key by the payload's
 *  correlation: the renderer learns task ids from CreateTask acks and
 *  snapshots, and applies state events in order per task via `taskId` when
 *  present in the event, else to the most recent task whose sequence matches. */
export function applyEvent(m: Model, e: Event, taskIdHint?: string): Model {
  if (m.seenOffsets.has(e.offset)) return m;
  const next: Model = { ...m, tasks: new Map(m.tasks), seenOffsets: new Set(m.seenOffsets), agents: new Map(m.agents), parents: new Map(m.parents) };
  next.seenOffsets.add(e.offset);
  if (BigInt(e.offset) > BigInt(next.cursor)) next.cursor = e.offset;
  const p = (e.payload ?? {}) as Record<string, unknown>;
  const target = taskIdHint ?? (typeof p["task_id"] === "string" ? hexId(p["task_id"]) : undefined);
  switch (e.eventType) {
    case "TaskCreated": {
      if (!target) return next;
      const card = freshCard(target, String(p["goal_text"] ?? ""), "Created", 1, e.occurredAtMs, String(p["origin"] ?? ""));
      next.tasks.set(target, card);
      const parent = next.parents.get(target);
      if (parent) {
        link(next, target, parent);
        const pc = next.tasks.get(parent);
        if (pc) next.tasks.set(parent, { ...pc, children: withChild(pc.children, target) });
      }
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
          updated.diagnostic = null;
          if (updated.phase === "awaitingHuman") updated.phase = "drafting";
          break;
        case "TaskWaiting":
          updated.state = "Waiting";
          updated.waitReason = normalizeReason(String(p["reason"] ?? "External"));
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
        case "TaskNeedsAttention": {
          updated.nextAction = String(p["reason"] ?? "Needs attention");
          if (updated.waitReason === "Capacity") updated.phase = "waitingCapacity";
          const d = p["diagnostic"];
          if (d && typeof d === "object") {
            const x = d as Record<string, unknown>;
            updated.diagnostic = {
              class: String(x["class"] ?? ""),
              code: String(x["code"] ?? ""),
              detail: String(x["detail"] ?? ""),
              userAction: String(x["user_action"] ?? ""),
              recoveryPath: String(x["recovery_path"] ?? ""),
              retryable: x["retryable"] === true,
              evidenceRefs: Array.isArray(x["evidence_refs"]) ? (x["evidence_refs"] as unknown[]).map(String) : [],
            };
          }
          break;
        }
        // The approval aggregate's events carry the task id: the exact
        // intent the decision binds is what the Task screen shows.
        case "ApprovalRequested":
          updated.approval = { approvalId: e.aggregateId ?? "", toolName: String(p["tool_name"] ?? ""), effectClass: normalizeReason(String(p["effect_class"] ?? "")), intentHash: String(p["intent_hash"] ?? "") };
          break;
        case "ApprovalResolved":
          updated.approval = null;
          break;
        case "UserQuestionAsked": {
          const options = Array.isArray(p["options"]) ? (p["options"] as Record<string, unknown>[]).map((o) => ({ id: String(o["id"] ?? ""), label: String(o["label"] ?? "") })) : [];
          updated.question = { questionId: String(p["question_id"] ?? ""), text: String(p["question"] ?? ""), options, allowFreeText: p["allow_free_text"] === true };
          break;
        }
        case "UserQuestionAnswered":
          updated.question = null;
          break;
        case "RoutingPlanAdmitted":
          updated.feasibility = String(p["feasibility"] ?? "");
          break;

        // M6.6: the AgentGraph on the card — agents by status, children
        // nested under their parent, a child's trouble surfacing on the
        // parent as its next action.
        case "AgentNodeCreated": {
          const node = (p["node"] ?? {}) as Record<string, unknown>;
          const id = String(node["agent_id"] ?? "");
          const statuses = new Map(next.agents.get(target) ?? []);
          statuses.set(id, String(node["status"] ?? ""));
          next.agents.set(target, statuses);
          updated.agents = countsOf(statuses);
          if (String(node["kind"]) === "SUBAGENT") updated.phase = "delegating";
          const child = hexId(node["child_task_id"]);
          if (child) {
            link(next, child, target);
            updated.children = withChild(updated.children, child);
          }
          break;
        }
        case "AgentNodeTransitioned": {
          const statuses = new Map(next.agents.get(target) ?? []);
          statuses.set(String(p["agent_id"] ?? ""), String(p["to"] ?? ""));
          next.agents.set(target, statuses);
          updated.agents = countsOf(statuses);
          break;
        }
        case "SubagentAdmitted": {
          const child = hexId(p["child_task_id"]);
          if (child) {
            link(next, child, target);
            updated.children = withChild(updated.children, child);
          }
          updated.phase = "delegating";
          updated.latestEvidence = `delegated ${String(p["idempotency_key"] ?? "a child")} (${String(p["mode"] ?? "")})`;
          break;
        }
        case "SubagentAdmissionRefused":
          updated.latestEvidence = `delegation refused: ${String(p["code"] ?? "")} at ${String(p["stage"] ?? "")}`;
          break;
        // REQ-EV-0046: a background child reached a protected effect; it was
        // refused and the parent decides (TaskNeedsAttention follows).
        case "SubagentProtectedEffect":
          updated.latestEvidence = `child ${String(p["idempotency_key"] ?? "")} reached a protected effect: ${String(p["tool"] ?? "")} (${String(p["effect_class"] ?? "")})`;
          break;
        case "SubagentResultRecorded": {
          const status = String(p["status"] ?? "");
          const summary = String(p["summary"] ?? "");
          updated.latestEvidence = `child ${status.toLowerCase()}: ${summary}`;
          if (status === "FAILED" || status === "WAITING") updated.nextAction = `A child agent needs attention (${status.toLowerCase()}): ${summary}`;
          break;
        }
        case "CapacityDenied":
          updated.waitReason = "Capacity";
          updated.phase = "waitingCapacity";
          updated.nextAction = `Waiting for capacity: ${String(p["dimension"] ?? "")} ${String(p["available"] ?? "")}/${String(p["needed"] ?? "")}`;
          break;
        case "CapacityTicketGranted":
          if (updated.phase === "waitingCapacity") updated.phase = "drafting";
          if (updated.nextAction?.startsWith("Waiting for capacity")) updated.nextAction = null;
          break;
        // Run-aggregate events the Core stamps with the task id: the
        // phase and the latest evidence come from them, never from logs.
        case "VerificationRunRecorded":
          updated.phase = "verifying";
          updated.latestEvidence = `${String(p["stage"] ?? "")} verification ${String(p["status"] ?? "")}`;
          break;
        case "AcceptanceGateEvaluated": {
          updated.phase = "reviewing";
          updated.latestEvidence = `acceptance gate ${String(p["verdict"] ?? "")}`;
          if (p["human_required"] === true) updated.phase = "awaitingHuman";
          const strings = (v: unknown): string[] => (Array.isArray(v) ? v.map(String) : []);
          updated.gate = { verdict: String(p["verdict"] ?? ""), missingEvidence: strings(p["missing_evidence"]), rejectReasons: strings(p["reject_reasons"]), gateRef: String(p["gate_ref"] ?? ""), humanRequired: p["human_required"] === true, candidateRevision: String(p["candidate_revision"] ?? "") };
          break;
        }
        case "ContinuationActivated":
          updated.phase = "escalating";
          updated.latestEvidence = `escalated to ${String(p["endpoint"] ?? "")}/${String(p["model"] ?? "")}`;
          break;
        case "RealizedRiskDerived":
          updated.risk = String(p["level"] ?? "");
          break;
        default:
          return next;
      }
      if (updated.state === "Waiting" && (updated.waitReason === "Approval" || updated.waitReason === "UserInput")) updated.phase = "awaitingHuman";
      if (updated.state === "Completed") updated.phase = "done";
      updated.lastOffset = e.offset;
      next.tasks.set(target, updated);
      return next;
    }
  }
}

/** The children of a task, in admission order. */
export function childrenOf(m: Model, taskId: string): TaskCard[] {
  const c = m.tasks.get(taskId);
  if (!c) return [];
  return c.children.map((id) => m.tasks.get(id)).filter((x): x is TaskCard => Boolean(x));
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
  // A subagent's task is its parent's business: it shows under the parent
  // card, never as a top-level card (M6.6, one primary owns the outcome).
  for (const c of [...m.tasks.values()].filter((c) => c.parentTaskId === null && c.origin !== "subagent").sort((a, b) => b.createdAtMs - a.createdAtMs || a.taskId.localeCompare(b.taskId))) out[columnOf(c)].push(c);
  return out;
}
