/**
 * The plan card (REQ-PX-200; DR-PX-2026-11-11-021 MWS-06): the steps of the task's plan with the state the Core's work graph
 * holds for each, in the graph's order. It is drawn from GetWorkGraph and GetPlan alone and decides nothing: a step is done
 * when the Core says it is. The state of every step is a glyph and words (the words are in the accessibility tree; the active
 * and the troubled ones are also on the line).
 */
import { memo } from "react";
import { StatusDot } from "@modbit/ui";
import type { PlanInfo, WorkGraphInfo } from "../../shared/conversation-types.ts";
import { planProgress, planSteps } from "./model.ts";

export interface PlanCardProps {
  graph: WorkGraphInfo;
  plan: PlanInfo | null;
  /** The task's turn is running: an active step is moving. */
  running: boolean;
  /** The task waits on the person (an approval, a question). */
  needsPerson: boolean;
}

const SHOWN = new Set(["BLOCKED", "FAILED", "CANCELLED"]);

export const PlanCard = memo(function PlanCard({ graph, plan, running, needsPerson }: PlanCardProps) {
  const steps = planSteps(graph.steps, { running, needsPerson });
  const progress = planProgress(graph.steps);
  const version = plan?.currentVersion || graph.planVersion;
  return (
    <section className="conv-plan" aria-label={`Plan: ${progress}`} data-testid="plan-card" data-plan-version={version} data-steps={steps.length}>
      <header className="conv-plan-head">
        <h3 className="conv-plan-title">Plan</h3>
        <span className="conv-plan-progress" data-testid="plan-progress">
          {progress}
        </span>
        {plan && plan.versions > 1 && (
          <span className="conv-plan-progress" data-testid="plan-version">
            revised {plan.versions - 1} {plan.versions === 2 ? "time" : "times"}
          </span>
        )}
      </header>
      {plan?.outcome && (
        <p className="conv-plan-outcome" data-testid="plan-outcome">
          {plan.outcome}
        </p>
      )}
      <ol className="conv-plan-steps">
        {steps.map((s) => (
          <li key={s.id} className="conv-plan-step" data-testid="plan-step" data-step-id={s.id} data-status={s.status} data-current={s.current} aria-current={s.current ? "step" : undefined}>
            <StatusDot status={s.tone} label={s.words} />
            <span className="conv-plan-step-title">{s.title}</span>
            {(SHOWN.has(s.status) || (s.current && !running)) && <span className="conv-plan-step-state">{s.words}</span>}
            {s.blockers.length > 0 && (
              <span className="conv-plan-step-state" data-testid="plan-step-blockers">
                {s.blockers.join("; ")}
              </span>
            )}
          </li>
        ))}
      </ol>
    </section>
  );
});
