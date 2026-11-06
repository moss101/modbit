/**
 * The task's details (the conversation's overflow): every fact of a task the
 * conversation does not show as a row, each from a Core event or read. Decisions
 * are made in the conversation; this panel only states, and offers the task's
 * stop and browser.
 */
import { useEffect, useState, type ReactNode } from "react";
import { Button } from "@modbit/ui";
import { DENSITIES, type Density } from "../../shared/conversation-types.ts";
import type { ContextInspectorSummary, TaskEconomicsSummary } from "../../preload/preload.ts";
import { PHASE_LABEL, type TaskCard } from "../model.ts";

const DENSITY_LABEL: Record<Density, string> = { COMPACT: "Compact", BALANCED: "Balanced", DETAILED: "Detailed" };

function Fact({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="task-fact">
      <dt>{label}</dt>
      <dd>{children}</dd>
    </div>
  );
}

export interface TaskDetailsProps {
  taskId: string;
  card: TaskCard | undefined;
  /** The subagents the Core nested under this task. */
  subtasks: readonly TaskCard[];
  density: Density;
  onDensity: (d: Density) => void;
  /** The checkpoint surface's undo-all control. */
  undoAll: ReactNode;
  onBrowser: () => void;
  onCancel: () => void;
  onOpenTask: (taskId: string) => void;
}

export function TaskDetails({ taskId, card, subtasks, density, onDensity, undoAll, onBrowser, onCancel, onOpenTask }: TaskDetailsProps) {
  const [inspector, setInspector] = useState<ContextInspectorSummary | null>(null);
  const [economics, setEconomics] = useState<TaskEconomicsSummary | null>(null);
  const offset = card?.lastOffset;
  useEffect(() => {
    let live = true;
    window.modbit
      .contextInspector(taskId)
      .then((i) => live && setInspector(i))
      .catch(() => live && setInspector(null));
    window.modbit
      .taskEconomics(taskId)
      .then((e) => live && setEconomics(e))
      .catch(() => live && setEconomics(null));
    return () => {
      live = false;
    };
  }, [taskId, offset]);
  const active = card ? card.agents.running + card.agents.background : 0;
  const done = !card || card.state === "Completed" || card.state === "Cancelled" || card.state === "Failed";
  return (
    <div className="task-details" role="dialog" aria-label="Task details" data-testid="task-details" data-task-id={taskId} data-state={card?.state} data-phase={card?.phase}>
      {card && (
        <dl className="task-facts">
          <Fact label="State">
            <span data-testid="task-state">{card.state}</span>
            {card.waitReason ? ` · waiting on ${card.waitReason}` : ""}
          </Fact>
          <Fact label="Phase">
            <span data-testid="task-phase">{PHASE_LABEL[card.phase]}</span>
          </Fact>
          <Fact label="Agents">
            <span data-testid="task-agents" aria-label={`${active} active of ${card.agents.total} agents`}>
              {active}/{card.agents.total}
            </span>
          </Fact>
          <Fact label="Generation">{card.generation}</Fact>
          <Fact label="Attachments">
            <span data-testid="task-attachments">{card.attachments ?? 0}</span>
          </Fact>
          {card.risk && (
            <Fact label="Risk">
              <span data-testid="task-risk">{card.risk}</span>
            </Fact>
          )}
          {card.security?.length ? (
            <Fact label="Security">
              <span data-testid="task-security" data-count={card.security.length} title={card.security.map((s) => `${s.kind} in ${s.toolName}: ${s.patterns.join(", ")} (${s.action.toLowerCase()})`).join("\n")}>
                {card.security.filter((s) => s.action === "BLOCKED").length} blocked, {card.security.filter((s) => s.action === "MARKED").length} marked
              </span>
            </Fact>
          ) : null}
          {card.latestEvidence && (
            <Fact label="Evidence">
              <span data-testid="task-evidence">{card.latestEvidence}</span>
            </Fact>
          )}
          {card.nextAction && <Fact label="Next">{card.nextAction}</Fact>}
        </dl>
      )}
      {subtasks.length > 0 && (
        <ul className="task-subtasks" data-testid="task-children" aria-label="subagents">
          {subtasks.map((c) => (
            <li key={c.taskId} data-testid="child-card" data-task-id={c.taskId} data-state={c.state}>
              <button type="button" className="agents-link" onClick={() => onOpenTask(c.taskId)}>
                <span data-testid="child-goal">{c.goalText}</span>
              </button>{" "}
              · <span data-testid="child-state">{c.state}</span>
              {c.latestEvidence ? ` · ${c.latestEvidence}` : ""}
            </li>
          ))}
        </ul>
      )}
      {inspector && inspector.packId !== "" && (
        <details className="task-section" data-testid="context-inspector">
          <summary>
            Context: {inspector.entries.filter((e) => e.injected).length} of {inspector.entries.length} fragment(s) injected, {inspector.tokenUsed}/{inspector.tokenBudget} tokens, {inspector.omittedCount} omitted
            {inspector.rejectedRefs.length > 0 ? `, ${inspector.rejectedRefs.length} refused for missing provenance` : ""}
          </summary>
          {inspector.compactionEpochs > 0 && (
            <p className="meta" data-testid="context-compaction">
              Compaction: epoch {inspector.compactionEpoch} of {inspector.compactionEpochs}, {String(inspector.compactedEntries)} earlier entr{Number(inspector.compactedEntries) === 1 ? "y" : "ies"} summarised and still in the log; prompt prefix reused on {inspector.prefixCacheHits} turn(s), rebuilt on {inspector.prefixCacheMisses}.
            </p>
          )}
          <ul className="meta">
            {inspector.entries.map((e) => (
              <li key={e.entryId} data-testid="context-entry">
                {e.sourceRef}
                {e.lineEnd > 0 ? ` L${e.lineStart}-${e.lineEnd}` : ""} — {e.reason}; {e.freshness}; {e.tokenCost} tokens
                {e.injected ? "; injected" : "; not injected"}
                {e.used ? "; used" : ""}
                {e.stub ? "; stub" : ""}
              </li>
            ))}
            {inspector.omittedPaths.map((p) => (
              <li key={p} data-testid="context-omitted">
                {p} — omitted for the token budget
              </li>
            ))}
          </ul>
        </details>
      )}
      {economics && economics.modelCalls > 0 && (
        <p className="meta task-section" data-testid="task-economics">
          Economics: {economics.verified ? "verified" : "not verified"}, {economics.checksPassed}/{economics.checksPassed + economics.checksFailed} check(s) passed; {economics.modelCalls} model call(s) on {economics.model || "no model"}, {economics.inputTokens} in ({economics.cachedInputTokens} cached) and {economics.outputTokens} out{economics.pricingKnown ? `, $${economics.costUsd.toFixed(4)} at catalog list prices` : ", price unknown"}; {economics.toolCalls} tool call(s); {economics.wallMs} ms wall, {economics.modelMs} ms in the model, {economics.toolMs} ms in tools; prompt prefix reused {economics.prefixCacheHits} of {economics.prefixCacheHits + economics.prefixCacheMisses} time(s).
        </p>
      )}
      <div className="task-controls">
        <label className="conv-density">
          <span className="meta">Density</span>
          <select value={density} onChange={(e) => onDensity(e.target.value as Density)} data-testid="conv-density" aria-label="Conversation density">
            {DENSITIES.map((d) => (
              <option key={d} value={d}>
                {DENSITY_LABEL[d]}
              </option>
            ))}
          </select>
        </label>
        {undoAll}
        <span className="conv-spacer" />
        {!done && (
          <Button size="sm" onClick={onBrowser} data-testid="task-browser">
            Browser
          </Button>
        )}
        {!done && (
          <Button size="sm" variant="ghost" onClick={onCancel} data-testid="task-cancel">
            Cancel task
          </Button>
        )}
      </div>
    </div>
  );
}
