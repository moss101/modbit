import { useState, useSyncExternalStore } from "react";
import type { TaskCard } from "../model.ts";
import { StateLine } from "../shell/state-line.tsx";
import type { ScreenState as DerivedScreenState } from "../screens.ts";

const DECISION_NOTES = new Map<string, string>();
const NOTE_LISTENERS = new Set<() => void>();
const subscribeNotes = (l: () => void) => {
  NOTE_LISTENERS.add(l);
  return () => void NOTE_LISTENERS.delete(l);
};
const PHASE_LABEL: Record<TaskCard["phase"], string> = {
  drafting: "drafting",
  verifying: "verifying",
  reviewing: "reviewing",
  escalating: "escalating",
  awaitingHuman: "awaiting human",
  waitingCapacity: "waiting for capacity",
  delegating: "delegating",
  done: "done",
};

/** PRD "Home / Fleet" card (M6.6): goal, state, phase, active agents, risk,
 *  latest evidence, next required action, and the subagents nested under
 *  their parent — every fact from a Core event. */
export function Card({ card, children, state, sessionId, onFocus, onStart, onReview, onBrowser }: { card: TaskCard; children?: TaskCard[]; state: DerivedScreenState; sessionId: string | null; onFocus: (id: string) => void; onStart: (id: string) => void; onReview: (id: string) => void; onBrowser: (id: string) => void }) {
  // PX-024: one line of steering input on the card (QueueInput STEER).
  const [steer, setSteer] = useState("");
  const [steerNote, setSteerNote] = useState<string | null>(null);
  const sendSteer = async () => {
    if (!sessionId || !steer.trim()) return;
    try {
      const r = await window.modbit.steerTask(sessionId, card.taskId, steer.trim());
      setSteerNote(`steered at offset ${r.offset}`);
      setSteer("");
    } catch (e) {
      setSteerNote(`refused: ${(e as Error).message}`);
    }
  };
  // PX-023 "awaiting approval with the exact intent" / "awaiting your
  // answer": the decision names the intent hash the Core showed (the Core
  // refuses any other), the answer names the question; both are the
  // Core's records, not the renderer's.
  const [deciding, setDeciding] = useState(false);
  // The note outlives the card's mount: a decided task moves between board columns, which remounts its card, and the answer must still be shown.
  const decisionNote = useSyncExternalStore(subscribeNotes, () => DECISION_NOTES.get(card.taskId) ?? null);
  const setDecisionNote = (n: string | null) => {
    if (n === null) DECISION_NOTES.delete(card.taskId);
    else DECISION_NOTES.set(card.taskId, n);
    for (const l of NOTE_LISTENERS) l();
  };
  const [answer, setAnswer] = useState("");
  const decide = async (approve: boolean) => {
    if (!sessionId || !card.approval || deciding) return;
    setDeciding(true);
    try {
      const r = await window.modbit.resolveApproval(sessionId, card.approval.approvalId, approve, approve ? "approved on the task card" : "denied on the task card", card.approval.intentHash);
      setDecisionNote(`${r.status.toLowerCase()} at offset ${r.offset}`);
    } catch (e) {
      setDecisionNote(`refused: ${(e as Error).message}`);
    } finally {
      setDeciding(false);
    }
  };
  const respond = async (optionId: string, text: string) => {
    if (!sessionId || !card.question || deciding) return;
    setDeciding(true);
    try {
      const r = await window.modbit.respondToQuestion(sessionId, card.taskId, card.question.questionId, optionId, text);
      setDecisionNote(r.alreadyAnswered ? "already answered" : `answered question ${r.questionId.slice(0, 8)}; resume to continue`);
      setAnswer("");
    } catch (e) {
      setDecisionNote(`refused: ${(e as Error).message}`);
    } finally {
      setDeciding(false);
    }
  };
  // A queued task starts; a waiting task resumes with StartTask unless it
  // waits on an approval (decide it) or capacity (the Core grants it).
  const startable = card.state === "Queued" || (card.state === "Waiting" && card.waitReason !== "Approval" && card.waitReason !== "Capacity");
  const active = card.agents.running + card.agents.background;
  return (
    <article className="card" tabIndex={0} data-testid="task-card" data-task-id={card.taskId} data-state={card.state} data-phase={card.phase} aria-label={`${card.goalText}: ${card.state}${card.waitReason ? ` waiting on ${card.waitReason}` : ""}, ${PHASE_LABEL[card.phase]}`} onFocus={() => onFocus(card.taskId)}>
      <div>{card.goalText}</div>
      <div className="meta">
        state: <span data-testid="task-state">{card.state}</span>
        {card.waitReason ? ` · waiting on ${card.waitReason}` : ""} · gen {card.generation} · local_trusted
        {" · attachments "}
        <span data-testid="task-attachments">{card.attachments ?? 0}</span>
        {card.security?.length ? (
          <>
            {" · security "}
            <span data-testid="task-security" data-count={card.security.length} title={card.security.map((s) => `${s.kind} in ${s.toolName}: ${s.patterns.join(", ")} (${s.action.toLowerCase()})`).join("\n")}>
              {card.security.filter((s) => s.action === "BLOCKED").length} blocked, {card.security.filter((s) => s.action === "MARKED").length} marked
            </span>
          </>
        ) : null}
      </div>
      <div className="meta">
        phase: <span data-testid="task-phase">{PHASE_LABEL[card.phase]}</span>
        {" · agents "}
        <span data-testid="task-agents" aria-label={`${active} active of ${card.agents.total} agents`}>
          {active}/{card.agents.total}
        </span>
        {card.risk ? (
          <>
            {" · risk "}
            <span data-testid="task-risk">{card.risk}</span>
          </>
        ) : null}
      </div>
      {card.latestEvidence && (
        <div className="meta">
          evidence: <span data-testid="task-evidence">{card.latestEvidence}</span>
        </div>
      )}
      {card.nextAction && (
        <div className="meta">
          next: <strong>{card.nextAction}</strong>
        </div>
      )}
      <StateLine state={state} testid="task-screen-state" />
      {card.state === "Waiting" && card.waitReason === "Approval" && card.approval && (
        <div className="decision" data-testid="task-approval" data-approval-id={card.approval.approvalId}>
          <span className="meta">
            {card.approval.toolName} asks for a {card.approval.effectClass} effect · intent <code data-testid="task-approval-intent">{card.approval.intentHash.slice(0, 16)}</code>
          </span>{" "}
          <button type="button" className="small" data-testid="task-approve" onClick={() => void decide(true)} disabled={deciding}>
            Approve
          </button>{" "}
          <button type="button" className="small" data-testid="task-deny" onClick={() => void decide(false)} disabled={deciding}>
            Deny
          </button>
        </div>
      )}
      {card.question && (
        <div className="decision" data-testid="task-question" data-question-id={card.question.questionId}>
          <span className="meta" data-testid="task-question-text">{card.question.text}</span>{" "}
          {card.question.options.map((o) => (
            <button type="button" className="small" key={o.id} data-testid="task-answer-option" data-option-id={o.id} onClick={() => void respond(o.id, "")} disabled={deciding}>
              {o.label}
            </button>
          ))}
          {card.question.allowFreeText && (
            <>
              <input data-testid="task-answer-text" aria-label="Your answer" value={answer} onChange={(e) => setAnswer(e.target.value)} placeholder="your answer" disabled={deciding} />
              <button type="button" className="small" data-testid="task-answer-send" onClick={() => void respond("", answer)} disabled={deciding || !answer.trim()}>
                Answer
              </button>
            </>
          )}
        </div>
      )}
      {decisionNote && (
        <div className="meta" role="status" data-testid="task-decision">
          {decisionNote}
        </div>
      )}
      {(card.state === "Running" || card.state === "Waiting" || card.state === "Queued") && (
        <div className="decision">
          <input data-testid="task-steer" aria-label={`Steer ${card.goalText}`} value={steer} onChange={(e) => setSteer(e.target.value)} onKeyDown={(e) => { if (e.key === "Enter") { e.preventDefault(); void sendSteer(); } }} placeholder="steer (s), Enter sends" />
          {steerNote && (
            <span className="meta" role="status" data-testid="task-steer-note">
              {steerNote}
            </span>
          )}
        </div>
      )}
      {children && children.length > 0 && (
        <ul className="children" data-testid="task-children" aria-label="subagents">
          {children.map((c) => (
            <li key={c.taskId} data-testid="child-card" data-task-id={c.taskId} data-state={c.state}>
              <span data-testid="child-goal">{c.goalText}</span> · <span data-testid="child-state">{c.state}</span>
              {c.latestEvidence ? ` · ${c.latestEvidence}` : ""}
            </li>
          ))}
        </ul>
      )}
      <div className="actions">
        {startable && (
          <button type="button" data-testid="task-start" onClick={() => onStart(card.taskId)}>
            {card.state === "Waiting" ? "Resume" : "Start"}
          </button>
        )}
        {card.state === "ReadyForReview" && (
          <button type="button" data-testid="task-review" onClick={() => onReview(card.taskId)}>
            Review
          </button>
        )}
        {card.state !== "Completed" && card.state !== "Cancelled" && card.state !== "Failed" && (
          <button type="button" data-testid="task-browser" onClick={() => onBrowser(card.taskId)}>
            Browser
          </button>
        )}
      </div>
    </article>
  );
}
