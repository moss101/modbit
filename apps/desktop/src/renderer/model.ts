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
  /** Security events the Core recorded on this task (M7.7): an
   *  instruction-shaped passage marked in what a tool returned, a call
   *  refused for carrying a credential. The shape and the runtime's answer,
   *  never a secret. */
  security?: { kind: string; toolName: string; patterns: string[]; action: string }[];
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
  /** The watermark: the highest event offset applied (or the snapshot's).
   *  The Core delivers a session's events in strictly increasing offset
   *  order, so an event at or below it is a replay of something already
   *  folded in — dropped in O(1), with no set of every offset ever seen. */
  cursor: string;
  tasks: Map<string, TaskCard>;
  /** Agent node status by agent id, per task (M6.6). */
  agents: Map<string, Map<string, string>>;
  /** Child task → parent task, learned from `SubagentAdmitted` (which may
   *  arrive before or after the child's own `TaskCreated`). */
  parents: Map<string, string>;
}

export function emptyModel(): Model {
  return { sessionId: null, cursor: "0", tasks: new Map(), agents: new Map(), parents: new Map() };
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

/** Which counter a node status belongs to; `null` for a status the Fleet does not count. */
function bucketOf(status: string): Exclude<keyof AgentCounts, "total"> | null {
  switch (status) {
    case "RUNNING":
    case "ADMITTED":
    case "ADMISSION_PENDING":
      return "running";
    case "BACKGROUND":
      return "background";
    case "WAITING":
    case "PARKED":
      return "waiting";
    case "COMPLETED":
      return "done";
    case "FAILED":
    case "CANCELLED":
      return "failed";
    default:
      return null;
  }
}

/** Record `agentId` at `to` and return the counts after: the one counter it
 *  leaves and the one it joins move, so an agent event costs O(1) however
 *  many agents the task has (it used to re-count every one of them). */
function setAgent(counts: AgentCounts, statuses: Map<string, string>, agentId: string, to: string): AgentCounts {
  const c = { ...counts };
  const was = statuses.get(agentId);
  if (was === undefined) c.total += 1;
  else {
    const left = bucketOf(was);
    if (left) c[left] -= 1;
  }
  const joined = bucketOf(to);
  if (joined) c[joined] += 1;
  statuses.set(agentId, to);
  return c;
}

/** Link a child card to its parent once both are known: the parents map
 *  and the child's side; the parent's `children` is kept by the parent's
 *  own event arm (its card is rewritten there). */
function link(d: Draft, childId: string, parentId: string): void {
  d.parents().set(childId, parentId);
  const child = d.m.tasks.get(childId);
  if (child && child.parentTaskId !== parentId) d.tasks().set(childId, { ...child, parentTaskId: parentId });
}

/** A model being written. The reducer folds events into a `Draft`: its maps
 *  are copied the first time one is written (once for a whole batch), never
 *  per event and never when an event touches none of them; the model the
 *  caller holds is not changed. The watermark is a string, so replaying the
 *  same event twice (a React updater run twice) gives the same answer. */
class Draft {
  readonly m: Model;
  private ownTasks: boolean;
  private ownAgents: boolean;
  private ownParents: boolean;
  private readonly ownStatuses = new Set<string>();
  constructor(base: Model, ownAll: boolean) {
    this.m = { ...base };
    this.ownTasks = ownAll;
    this.ownAgents = ownAll;
    this.ownParents = ownAll;
    if (ownAll) {
      this.m.tasks = new Map(base.tasks);
      this.m.agents = new Map(base.agents);
      this.m.parents = new Map(base.parents);
    }
  }
  tasks(): Map<string, TaskCard> {
    if (!this.ownTasks) {
      this.m.tasks = new Map(this.m.tasks);
      this.ownTasks = true;
    }
    return this.m.tasks;
  }
  agents(): Map<string, Map<string, string>> {
    if (!this.ownAgents) {
      this.m.agents = new Map(this.m.agents);
      this.ownAgents = true;
    }
    return this.m.agents;
  }
  /** The writable status map of one task's agents: copied the first time
   *  this draft writes it — once per task for a whole batch. */
  statuses(taskId: string): Map<string, string> {
    const outer = this.agents();
    if (this.ownStatuses.has(taskId)) return outer.get(taskId)!;
    const own = new Map(outer.get(taskId) ?? []);
    outer.set(taskId, own);
    this.ownStatuses.add(taskId);
    return own;
  }
  parents(): Map<string, string> {
    if (!this.ownParents) {
      this.m.parents = new Map(this.m.parents);
      this.ownParents = true;
    }
    return this.m.parents;
  }
}

/** Offset order for decimal offset strings (no leading zeros): by length,
 *  then lexicographically — no BigInt per event. */
function offsetAfter(a: string, b: string): boolean {
  return a.length !== b.length ? a.length > b.length : a > b;
}

/** The newest security records a card keeps: a bounded list, not a log. */
const SECURITY_KEEP = 50;

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
  if (!offsetAfter(e.offset, m.cursor)) return m;
  const d = new Draft(m, false);
  fold(d, e, taskIdHint);
  return d.m;
}

/** Fold a batch of events — each with the task id it belongs to — in one
 *  pass: the maps are copied once for the whole batch and written in place,
 *  so a replay of n events costs O(n) however many cards there are. Events
 *  at or below the watermark are dropped, exactly as `applyEvent` does. */
export function applyEvents(m: Model, events: readonly { event: Event; taskId?: string }[]): Model {
  let d: Draft | null = null;
  let cursor = m.cursor;
  for (const { event, taskId } of events) {
    if (!offsetAfter(event.offset, cursor)) continue;
    d ??= new Draft(m, true);
    cursor = event.offset;
    fold(d, event, taskId);
  }
  return d ? d.m : m;
}

function fold(d: Draft, e: Event, taskIdHint?: string): void {
  const next = d.m;
  next.cursor = e.offset;
  const p = (e.payload ?? {}) as Record<string, unknown>;
  const target = taskIdHint ?? (typeof p["task_id"] === "string" ? hexId(p["task_id"]) : undefined);
  switch (e.eventType) {
    case "TaskCreated": {
      if (!target) return;
      const card = freshCard(target, String(p["goal_text"] ?? ""), "Created", 1, e.occurredAtMs, String(p["origin"] ?? ""));
      d.tasks().set(target, card);
      if (next.agents.has(target)) d.agents().delete(target);
      const parent = next.parents.get(target);
      if (parent) {
        link(d, target, parent);
        const pc = next.tasks.get(parent);
        if (pc) d.tasks().set(parent, { ...pc, children: withChild(pc.children, target) });
      }
      return;
    }
    default: {
      if (!target) return;
      const card = next.tasks.get(target);
      if (!card) return;
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
        case "SecurityEventRecorded":
          updated.security = [...(card.security ?? []).slice(-(SECURITY_KEEP - 1)), { kind: String(p["kind"] ?? ""), toolName: String(p["tool_name"] ?? ""), patterns: Array.isArray(p["patterns"]) ? (p["patterns"] as unknown[]).map(String) : [], action: String(p["action"] ?? "") }];
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
          updated.agents = setAgent(card.agents, d.statuses(target), id, String(node["status"] ?? ""));
          if (String(node["kind"]) === "SUBAGENT") updated.phase = "delegating";
          const child = hexId(node["child_task_id"]);
          if (child) {
            link(d, child, target);
            updated.children = withChild(updated.children, child);
          }
          break;
        }
        case "AgentNodeTransitioned": {
          updated.agents = setAgent(card.agents, d.statuses(target), String(p["agent_id"] ?? ""), String(p["to"] ?? ""));
          break;
        }
        case "SubagentAdmitted": {
          const child = hexId(p["child_task_id"]);
          if (child) {
            link(d, child, target);
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
          return;
      }
      if (updated.state === "Waiting" && (updated.waitReason === "Approval" || updated.waitReason === "UserInput")) updated.phase = "awaitingHuman";
      if (updated.state === "Completed") updated.phase = "done";
      updated.lastOffset = e.offset;
      d.tasks().set(target, updated);
      return;
    }
  }
}

/** The children of a task, in admission order. */
export function childrenOf(m: Model, taskId: string): TaskCard[] {
  const c = m.tasks.get(taskId);
  if (!c) return [];
  return c.children.map((id) => m.tasks.get(id)).filter((x): x is TaskCard => Boolean(x));
}

/** PRD "Home / Fleet" columns from card state. Needs Attention is the
 *  Core's: a task is in it exactly while the Core's attention view
 *  (`GetAttention`, REQ-EV-0151 / 0275) holds an item for it — derived from
 *  canonical unresolved state — and the task is not over. Until the first
 *  attention answer arrives (`attention` undefined) the card's own
 *  `nextAction` stands in, so a screen never shows nothing for a moment it
 *  knows better; once the Core has answered, the renderer's reading of
 *  `nextAction` has no say. */
export function columnOf(c: TaskCard, attention?: ReadonlySet<string>): FleetColumn {
  const live = c.state === "Waiting" || c.state === "Running" || c.state === "Queued";
  // A decision the person is being asked for (an approval or a question, both from the task's own events) is attention at once: the
  // Core's view lists the same item, but it arrives a moment later, and a card that moves column then is re-mounted under the
  // pointer - a click on Approve in that moment is lost.
  const asked = live && (c.approval !== null || c.question !== null);
  if (asked || (attention ? live && attention.has(c.taskId) : c.nextAction && (c.state === "Waiting" || c.state === "Running"))) return "needsAttention";
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

export function columns(m: Model, attention?: readonly { taskId: string }[]): Record<FleetColumn, TaskCard[]> {
  const out: Record<FleetColumn, TaskCard[]> = { needsAttention: [], readyForReview: [], running: [], waiting: [], completed: [], failed: [] };
  const attended = attention ? new Set(attention.map((i) => i.taskId)) : undefined;
  // A subagent's task is its parent's business: it shows under the parent
  // card, never as a top-level card (M6.6, one primary owns the outcome).
  for (const c of [...m.tasks.values()].filter((c) => c.parentTaskId === null && c.origin !== "subagent").sort((a, b) => b.createdAtMs - a.createdAtMs || a.taskId.localeCompare(b.taskId))) out[columnOf(c, attended)].push(c);
  return out;
}
