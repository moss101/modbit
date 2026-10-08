/**
 * Applying a task's worktree to the person's checkout (REQ-PX-068; docs/65
 * AFW-J08, AFW-G05). The modal walks the Core's answers and decides nothing:
 * the Core classifies every path, lists the choices it really offers, asks for
 * the approval of the exact plan, writes a pre-apply checkpoint before any
 * change, and refuses an overwrite whose files were not typed even if this
 * window sent it. Cancel is the selected choice when the conflict view opens,
 * so Enter changes nothing; a destructive choice needs its files typed; only a
 * non-destructive choice may be remembered for the task; Undo restores the
 * checkpoint exactly.
 */
import { useCallback, useEffect, useId, useRef, useState } from "react";
import { Button, Dialog } from "@modbit/ui";
import type { ApplyAckInfo, ApplyConflictInfo, ApplyInputArgs, WorktreeInfo } from "../../shared/project-types.ts";
import { parseRefusal } from "../projects/project-model.ts";
import { DEFAULT_OPTION, OPTION_WORDS, STOP_WORDS, canRemember, doneWords, missingPaths, optionLabel, optionsOf, requiredPaths, stepOf, type FlowKind, type Step } from "./apply-model.ts";

export interface ApplyModalProps {
  /** The worktree whose result is applied (or whose apply is undone); null closes the modal. */
  worktree: WorktreeInfo | null;
  flow: FlowKind;
  sessionId: string | null;
  titleOf: (taskId: string) => string;
  onClose: () => void;
  /** The checkout changed (or an approval was decided): lists should be read again. */
  onChanged: () => void;
}

const short = (s: string, n = 12) => s.slice(0, n);

export function ApplyModal({ worktree, flow, sessionId, titleOf, onClose, onChanged }: ApplyModalProps) {
  const [step, setStep] = useState<Step>({ kind: "working", what: "Asking the Core to classify the apply…" });
  const taskId = worktree?.taskId ?? "";

  const call = useCallback(
    async (kind: FlowKind, request: ApplyInputArgs, what: string): Promise<void> => {
      if (!sessionId || !taskId) return;
      setStep({ kind: "working", what });
      try {
        const ack: ApplyAckInfo = kind === "undo" ? await window.modbit.undoApply(sessionId, taskId, "") : await window.modbit.applyWorktree(sessionId, taskId, request);
        setStep(stepOf(ack, kind, request));
        if (ack.status === "APPLIED" || ack.status === "UNDONE" || ack.status === "NOTHING_TO_APPLY") onChanged();
      } catch (e) {
        const r = parseRefusal(e);
        setStep({ kind: "error", code: r.code || "REFUSED", detail: r.detail });
      }
    },
    [sessionId, taskId, onChanged],
  );

  // Opening the modal asks the Core to classify. Nothing is written by that call except an approval request for a plan with nothing to decide.
  const key = worktree ? `${worktree.worktreeId}:${flow}` : "";
  useEffect(() => {
    if (!key) return;
    void call(flow, {}, flow === "undo" ? "Asking the Core to restore your checkout…" : "Asking the Core to classify the apply…");
    // The start is by worktree and flow only; `call` is stable for them.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key]);

  const approve = async (s: Extract<Step, { kind: "approve" }>, ok: boolean) => {
    if (!sessionId) return;
    setStep({ kind: "working", what: ok ? "Recording your approval…" : "Recording your denial…" });
    try {
      await window.modbit.resolveApproval(sessionId, s.ack.approvalId, ok, ok ? "approved in the apply dialog" : "denied in the apply dialog", s.ack.intentHash);
    } catch (e) {
      const r = parseRefusal(e);
      setStep({ kind: "error", code: r.code || "REFUSED", detail: r.detail });
      return;
    }
    onChanged();
    if (!ok) {
      setStep({ kind: "stopped", flow: s.flow, ack: { ...s.ack, status: "DENIED", detail: "You denied the approval; nothing was written." }, reason: "DENIED" });
      return;
    }
    // Calling again with the same arguments is what applies, now that the approval stands.
    await call(s.flow, s.request, s.flow === "undo" ? "Restoring your checkout…" : "Applying…");
  };

  const title = flow === "undo" ? "Undo apply" : "Apply to my checkout";
  const subject = worktree ? (worktree.taskId ? titleOf(worktree.taskId) : worktree.worktreeId) : "";
  return (
    <Dialog open={worktree !== null} title={title} role="dialog" size="lg" onClose={onClose} testId="apply-modal" dismissOnScrim={false}>
      {worktree && (
        <div className="apply" data-step={step.kind} data-flow={flow}>
          <p className="meta" data-testid="apply-subject">
            {subject} · {worktree.branch} → {worktree.originRoot}
          </p>
          {step.kind === "working" && (
            <p role="status" aria-busy="true" data-testid="apply-working">
              {step.what}
            </p>
          )}
          {step.kind === "error" && (
            <div role="alert" data-testid="apply-error" data-code={step.code}>
              <p>
                <strong>{step.code}</strong>
              </p>
              <p>{step.detail}</p>
              <p className="meta">Nothing was changed.</p>
            </div>
          )}
          {step.kind === "approve" && <ApproveStep step={step} worktree={worktree} onDecide={(ok) => void approve(step, ok)} />}
          {step.kind === "conflict" && <ConflictStep step={step} onChoose={(request) => void call("apply", { ...request, expectedPlanDigest: step.conflict.planDigest }, "Asking the Core to apply your choice…")} onCancel={onClose} />}
          {step.kind === "done" && <DoneStep step={step} onUndo={() => void call("undo", {}, "Asking the Core to restore your checkout…")} onReapply={() => void call("apply", {}, "Asking the Core to classify the apply…")} />}
          {step.kind === "stopped" && (
            <div role="status" data-testid="apply-stopped" data-reason={step.reason}>
              <p data-testid="apply-stopped-words">{STOP_WORDS[step.reason]}</p>
              {step.ack.detail && <p className="meta">{step.ack.detail}</p>}
              {step.ack.divergentPaths.length > 0 && (
                <ul data-testid="apply-divergent">
                  {step.ack.divergentPaths.map((p) => (
                    <li key={p}>
                      <code>{p}</code>
                    </li>
                  ))}
                </ul>
              )}
              {step.reason === "STALE" && (
                <Button variant="primary" onClick={() => void call("apply", {}, "Asking the Core to classify the apply again…")} data-testid="apply-review-again">
                  Review again
                </Button>
              )}
            </div>
          )}
          {(step.kind === "done" || step.kind === "stopped" || step.kind === "error") && (
            <div className="dialog-actions">
              <Button onClick={onClose} data-testid="apply-close">
                Close
              </Button>
            </div>
          )}
        </div>
      )}
    </Dialog>
  );
}

function ApproveStep({ step, worktree, onDecide }: { step: Extract<Step, { kind: "approve" }>; worktree: WorktreeInfo; onDecide: (ok: boolean) => void }) {
  const deny = useRef<HTMLButtonElement>(null);
  useEffect(() => deny.current?.focus(), []);
  const a = step.ack;
  const undo = step.flow === "undo";
  return (
    <div data-testid="apply-approve">
      <h3 className="apply-h">{undo ? "Approve restoring your checkout" : "Approve this apply"}</h3>
      <p>
        {undo ? "The Core will put back the exact bytes your checkout had before the apply. A file you edited since is left alone and reported." : `The Core will write what the task changed (${worktree.changedFiles} ${worktree.changedFiles === 1 ? "file" : "files"}) into your checkout.`} Before any file is written it saves a checkpoint of every file it will touch, so Undo is exact.
      </p>
      <dl className="apply-facts">
        {a.option && (
          <div>
            <dt>Your choice</dt>
            <dd data-testid="apply-approve-option">{optionLabel(a.option)}</dd>
          </div>
        )}
        {a.planDigest && (
          <div>
            <dt>Plan</dt>
            <dd>
              <code data-testid="apply-approve-plan">{short(a.planDigest)}</code>
            </dd>
          </div>
        )}
        <div>
          <dt>Approves exactly</dt>
          <dd>
            <code data-testid="apply-approve-intent">{short(a.intentHash)}</code>
          </dd>
        </div>
      </dl>
      {a.detail && <p className="meta">{a.detail}</p>}
      <div className="dialog-actions">
        <Button ref={deny} onClick={() => onDecide(false)} data-testid="apply-deny">
          Deny
        </Button>
        <Button variant="primary" onClick={() => onDecide(true)} data-testid="apply-approve-button">
          {undo ? "Approve undo" : "Approve apply"}
        </Button>
      </div>
    </div>
  );
}

function ConflictStep({ step, onChoose, onCancel }: { step: Extract<Step, { kind: "conflict" }>; onChoose: (r: ApplyInputArgs) => void; onCancel: () => void }) {
  const c: ApplyConflictInfo = step.conflict;
  const options = optionsOf(c);
  const [chosen, setChosen] = useState<string>(DEFAULT_OPTION);
  const [typed, setTyped] = useState("");
  const [remember, setRemember] = useState(false);
  const typedId = useId();
  const first = useRef<HTMLInputElement>(null);
  useEffect(() => first.current?.focus(), []);
  const option = options.find((o) => o.option === chosen) ?? options[0];
  const required = option ? requiredPaths(option) : [];
  const missing = missingPaths(required, typed);
  const isCancel = !option || option.option === DEFAULT_OPTION;
  const go = () => {
    if (!option) return;
    if (isCancel) return onCancel();
    onChoose({ option: option.option, confirmPaths: required.length > 0 ? required : [], remember: canRemember(option) && remember });
  };
  return (
    <div data-testid="apply-conflict">
      <h3 className="apply-h">
        {c.conflictingPaths.length > 0 ? `${c.conflictingPaths.length} ${c.conflictingPaths.length === 1 ? "file conflicts" : "files conflict"} with your checkout` : "This apply needs a choice"}
      </h3>
      <p className="meta">Nothing has been changed. Before the Core writes anything it saves a checkpoint of every file it will touch, so Undo restores your checkout exactly.</p>
      {c.rememberedOption && (
        <p className="meta" data-testid="apply-remembered">
          You asked to remember {optionLabel(c.rememberedOption)} for this task.
        </p>
      )}
      {step.refusal && (
        <p className="proj-problem" role="alert" data-testid="apply-refusal">
          {step.refusal}
        </p>
      )}
      <table className="apply-paths" aria-label="Files this apply touches" data-testid="apply-paths">
        <thead>
          <tr>
            <th scope="col">File</th>
            <th scope="col">Change</th>
            <th scope="col">State</th>
          </tr>
        </thead>
        <tbody>
          {c.paths.map((p) => (
            <tr key={p.path} data-state={p.state} data-testid="apply-path">
              <td>
                <code>{p.path}</code>
                {p.protected ? " (protected)" : ""}
              </td>
              <td>{p.change.toLowerCase()}</td>
              <td>{p.conflict ? `conflict: ${p.conflict.toLowerCase().replace(/_/g, " ")}` : p.state.toLowerCase().replace(/_/g, " ")}</td>
            </tr>
          ))}
        </tbody>
      </table>
      {c.dirtyPaths.length > 0 && (
        <p className="meta" data-testid="apply-dirty">
          Your uncommitted work: {c.dirtyPaths.join(", ")}
        </p>
      )}
      <fieldset className="apply-options" data-testid="apply-options">
        <legend>What should happen</legend>
        {options.map((o, i) => {
          const words = OPTION_WORDS[o.option];
          const id = `apply-opt-${o.option}`;
          return (
            <label key={o.option} className="apply-option" data-testid={`apply-option-${o.option}`} data-destructive={o.destructive} data-available={o.available} htmlFor={id}>
              <input id={id} ref={i === 0 ? first : undefined} type="radio" name="apply-option" value={o.option} checked={chosen === o.option} disabled={!o.available} onChange={() => setChosen(o.option)} />
              <span>
                <strong>{words?.label ?? optionLabel(o.option)}</strong>
                {o.destructive ? " — replaces your bytes" : ""}
                <span className="meta apply-what">{o.available ? (words?.what ?? "") : (o.whyUnavailable ?? "")}</span>
              </span>
            </label>
          );
        })}
      </fieldset>
      {required.length > 0 && (
        <div className="proj-field" data-testid="apply-confirm">
          <label htmlFor={typedId}>Type these paths to confirm, one per line</label>
          <ul className="apply-required" data-testid="apply-required">
            {required.map((p) => (
              <li key={p}>
                <code>{p}</code>
              </li>
            ))}
          </ul>
          <textarea id={typedId} value={typed} onChange={(e) => setTyped(e.target.value)} rows={Math.min(8, required.length + 1)} spellCheck={false} data-testid="apply-confirm-input" aria-describedby={missing.length > 0 ? `${typedId}-missing` : undefined} />
          {missing.length > 0 && (
            <p id={`${typedId}-missing`} className="meta" data-testid="apply-missing">
              Still to type: {missing.length} {missing.length === 1 ? "path" : "paths"}.
            </p>
          )}
        </div>
      )}
      {option && canRemember(option) && (
        <label className="apply-remember">
          <input type="checkbox" checked={remember} onChange={(e) => setRemember(e.target.checked)} data-testid="apply-remember" /> Remember this choice for this task
        </label>
      )}
      <div className="dialog-actions">
        <Button onClick={onCancel} data-testid="apply-cancel">
          Cancel — change nothing
        </Button>
        <Button variant={option?.destructive ? "danger" : "primary"} disabled={isCancel || !option?.available || missing.length > 0} onClick={go} data-testid="apply-continue">
          {isCancel ? "Choose how to continue" : `Continue: ${optionLabel(option?.option ?? "")}`}
        </Button>
      </div>
    </div>
  );
}

function DoneStep({ step, onUndo, onReapply }: { step: Extract<Step, { kind: "done" }>; onUndo: () => void; onReapply: () => void }) {
  const a = step.ack;
  return (
    <div role="status" data-testid="apply-done" data-status={a.status}>
      <h3 className="apply-h">{step.flow === "undo" ? "Your checkout is restored" : a.status === "NOTHING_TO_APPLY" ? "Nothing to apply" : "Applied"}</h3>
      <p data-testid="apply-done-words">{doneWords(step.flow, a)}</p>
      {a.stashRef && (
        <p className="meta" data-testid="apply-stash">
          Your uncommitted work is in the Git stash <code>{a.stashRef}</code>. Bring it back with <code>git stash pop</code> when you are ready.
        </p>
      )}
      {a.unresolvedPaths.length > 0 && (
        <>
          <p className="meta">Resolve the conflict markers in:</p>
          <ul data-testid="apply-unresolved">
            {a.unresolvedPaths.map((p) => (
              <li key={p}>
                <code>{p}</code>
              </li>
            ))}
          </ul>
        </>
      )}
      {a.appliedPaths.length > 0 && (
        <details>
          <summary>
            {a.appliedPaths.length} {a.appliedPaths.length === 1 ? "file" : "files"}
          </summary>
          <ul data-testid="apply-applied">
            {a.appliedPaths.map((p) => (
              <li key={p}>
                <code>{p}</code>
              </li>
            ))}
          </ul>
        </details>
      )}
      {a.effectReceiptIds.length > 0 && (
        <p className="meta" data-testid="apply-receipts">
          Recorded as {a.effectReceiptIds.length} effect {a.effectReceiptIds.length === 1 ? "receipt" : "receipts"}.
        </p>
      )}
      {step.flow === "apply" && a.status === "APPLIED" && (
        <Button onClick={onUndo} data-testid="apply-undo">
          Undo this apply
        </Button>
      )}
      {step.reapply && (
        <Button variant="primary" onClick={onReapply} data-testid="apply-reapply">
          Now apply the task&apos;s result
        </Button>
      )}
    </div>
  );
}
