/**
 * Waiting on you (DR-PX-2026-11-11-021 MWS-12, REQ-PX-205): what needs the person, under home's tray. A pending approval the
 * Core lists is decided in place with Skip or Allow once, through the one decision path the task's card uses and bound to the
 * intent hash shown here; what only its task may decide (an effect that cannot be undone, native control, a budget raise) says
 * why and opens its task, where the card asks twice. Anything else the Core says needs the person (a question, a stall, a
 * failure) opens its task with a line in plain words. Nothing is listed that the Core did not list; nothing is decided here
 * that the Core would not take from the task's own card.
 */
import { useCallback, useState } from "react";
import { Badge, Button, Icon } from "@modbit/ui";
import type { AttentionItem } from "../../preload/preload.ts";
import type { DockApprovalView } from "../../shared/control-types.ts";
import { decideFromList } from "../approvals/decide.ts";
import { decidableFromList, isIrreversible, refusalWords, wantsLine } from "../approvals/model.ts";
import { attentionWords } from "./model.ts";

export interface WaitingProps {
  sessionId: string | null;
  approvals: readonly DockApprovalView[];
  /** The Core's attention items; approvals are listed from `approvals`, so items of that kind are not repeated. */
  attention: readonly AttentionItem[];
  titleOf: (taskId: string) => string;
  onOpenTask: (taskId: string) => void;
  /** Re-read the Core's list after a decision. */
  onDecided: () => void;
}

export function Waiting({ sessionId, approvals, attention, titleOf, onOpenTask, onDecided }: WaitingProps) {
  const [busy, setBusy] = useState<string | null>(null);
  const [note, setNote] = useState<{ kind: "ok" | "refused"; text: string } | null>(null);
  const others = attention.filter((i) => i.kind !== "APPROVAL");
  const decide = useCallback(
    async (a: DockApprovalView, approve: boolean) => {
      if (!sessionId) return;
      setBusy(a.approvalId);
      setNote(null);
      try {
        await decideFromList(sessionId, a, approve);
        setNote({ kind: "ok", text: approve ? `Allowed once: “${titleOf(a.taskId)}” runs it now.` : `Skipped: “${titleOf(a.taskId)}” is told you skipped it.` });
      } catch (e) {
        const m = (e as Error).message;
        setNote({ kind: "refused", text: m.startsWith("NOT_FROM_A_LIST: ") ? `Not decided here. ${m.slice("NOT_FROM_A_LIST: ".length)}` : refusalWords(m) });
      } finally {
        setBusy(null);
        onDecided();
      }
    },
    [sessionId, titleOf, onDecided],
  );
  if (approvals.length === 0 && others.length === 0 && !note) return null;
  return (
    <section className="home-block" data-testid="home-waiting" aria-labelledby="home-waiting-title">
      <h2 id="home-waiting-title" className="home-label">
        Waiting on you
      </h2>
      {(approvals.length > 0 || others.length > 0) && (
        <ul className="home-list">
          {approvals.map((a) => {
            const rule = decidableFromList(a);
            const irreversible = isIrreversible(a.effectClass);
            const title = titleOf(a.taskId);
            return (
              <li key={a.approvalId} className="home-li" data-testid="home-waiting-item" data-kind="APPROVAL" data-task-id={a.taskId} data-approval-id={a.approvalId} data-intent-hash={a.intentHash} data-effect-class={a.effectClass} data-decidable={rule.ok ? "true" : "false"}>
                <Icon name="shield-check" size={16} className="home-li-icon" />
                <div className="home-li-text">
                  <span className="home-li-title">{title}</span>
                  <span className="home-li-line" data-testid="home-waiting-line">
                    {wantsLine(a)}
                  </span>
                  {!rule.ok && (
                    <span className="mb-sr-only" data-testid="home-waiting-why">
                      {rule.why}
                    </span>
                  )}
                </div>
                {irreversible && <Badge tone="danger">Cannot be undone</Badge>}
                {rule.ok ? (
                  <>
                    <Button size="sm" variant="ghost" disabled={busy !== null} onClick={() => void decide(a, false)} data-testid="home-waiting-skip" aria-label={`Skip: ${title} (it does not run)`}>
                      Skip
                    </Button>
                    <Button size="sm" variant="primary" disabled={busy !== null} onClick={() => void decide(a, true)} data-testid="home-waiting-allow" aria-label={`Allow once: ${title}`}>
                      Allow once
                    </Button>
                  </>
                ) : (
                  <Button size="sm" onClick={() => onOpenTask(a.taskId)} data-testid="home-waiting-open" title={rule.why} aria-label={`Open to decide: ${title}. ${rule.why}`}>
                    Open to decide
                  </Button>
                )}
              </li>
            );
          })}
          {others.map((i) => (
            <li key={`${i.kind}:${i.taskId}:${i.reference}`} className="home-li" data-testid="home-waiting-item" data-kind={i.kind} data-task-id={i.taskId}>
              <Icon name="clock" size={16} className="home-li-icon" />
              <div className="home-li-text">
                <span className="home-li-title">{titleOf(i.taskId)}</span>
                <span className="home-li-line">{attentionWords(i)}</span>
              </div>
              <Button size="sm" onClick={() => onOpenTask(i.taskId)} data-testid="home-waiting-open" aria-label={`Open: ${titleOf(i.taskId)}`}>
                Open
              </Button>
            </li>
          ))}
        </ul>
      )}
      {note && (
        <p className="meta home-note" role="status" data-testid="home-waiting-note" data-kind={note.kind}>
          {note.text}
        </p>
      )}
    </section>
  );
}
