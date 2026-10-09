/**
 * Removing one worktree, and discarding a result (REQ-PX-068). Removal asks
 * the Core first (a dry run): the Core either refuses with a typed reason,
 * which this dialog explains and offers nothing to override, or says exactly
 * what the removal would take with it, which the person then confirms. A
 * worktree whose task is running or whose result is unapplied therefore never
 * reaches a Remove button. Discarding drops a worktree's result and needs the
 * branch name typed (the Core checks it).
 */
import { useEffect, useId, useRef, useState } from "react";
import { Button, Dialog } from "@modbit/ui";
import type { WorktreeInfo, WorktreeRemovalInfo } from "../../shared/project-types.ts";
import { parseRefusal, type Refusal } from "../projects/project-model.ts";
import { REMOVAL_ADVICE, sizeWords } from "./worktree-model.ts";

export type RemoveState = { phase: "checking" } | { phase: "refused"; refusal: Refusal } | { phase: "confirm"; preview: WorktreeRemovalInfo } | { phase: "removing"; preview: WorktreeRemovalInfo } | { phase: "failed"; preview: WorktreeRemovalInfo; refusal: Refusal };

/** The dialog's content for a state: the Core's typed refusal explained with nothing to override, or what the removal takes with it and the button that confirms. */
export function RemoveBody({ worktree, owner, state, cancelRef, onCancel, onConfirm }: { worktree: WorktreeInfo; owner: string; state: RemoveState; cancelRef?: React.RefObject<HTMLButtonElement | null>; onCancel: () => void; onConfirm: (preview: WorktreeRemovalInfo) => void }) {
  return (
    <div className="wt-dialog" data-phase={state.phase}>
      <p className="meta" data-testid="worktree-remove-subject">
        {owner ? `${owner} · ` : ""}
        {worktree.branch || worktree.worktreeId} · {worktree.path}
      </p>
      {state.phase === "checking" && (
        <p role="status" data-testid="worktree-remove-checking">
          Asking the Core whether this worktree can be removed…
        </p>
      )}
      {(state.phase === "refused" || state.phase === "failed") && (
        <div role="alert" data-testid="worktree-remove-refusal" data-code={state.refusal.code}>
          <p>
            <strong data-testid="worktree-remove-code">{state.refusal.code || "REFUSED"}</strong>
          </p>
          <p data-testid="worktree-remove-reason">{state.refusal.detail}</p>
          {REMOVAL_ADVICE[state.refusal.code] && <p className="meta">{REMOVAL_ADVICE[state.refusal.code]}</p>}
          <p className="meta">Nothing was removed.</p>
        </div>
      )}
      {(state.phase === "confirm" || state.phase === "removing" || state.phase === "failed") && (
        <>
          <p data-testid="worktree-remove-why">Removable: {state.preview.reason}.</p>
          <p>Removing it takes with it:</p>
          <ul data-testid="worktree-remove-loses">
            {state.preview.loses.map((l) => (
              <li key={l}>{l}</li>
            ))}
          </ul>
          <p className="meta">Frees {sizeWords(state.preview.bytes)}.</p>
        </>
      )}
      <div className="dialog-actions">
        <Button ref={cancelRef} onClick={onCancel} data-testid="worktree-remove-cancel">
          {state.phase === "refused" || state.phase === "failed" ? "Close" : "Keep it"}
        </Button>
        {(state.phase === "confirm" || state.phase === "removing") && (
          <Button variant="danger" disabled={state.phase === "removing"} onClick={() => onConfirm(state.preview)} data-testid="worktree-remove-confirm">
            Remove worktree
          </Button>
        )}
      </div>
    </div>
  );
}

export interface RemoveDialogProps {
  worktree: WorktreeInfo | null;
  sessionId: string | null;
  titleOf: (taskId: string) => string;
  onClose: () => void;
  /** The Core removed it: the list should be read again. */
  onRemoved: (removal: WorktreeRemovalInfo) => void;
}

export function RemoveWorktreeDialog({ worktree, sessionId, titleOf, onClose, onRemoved }: RemoveDialogProps) {
  const [state, setState] = useState<RemoveState>({ phase: "checking" });
  const cancel = useRef<HTMLButtonElement>(null);
  const id = worktree?.worktreeId ?? null;
  useEffect(() => {
    if (!id || !sessionId) return;
    let live = true;
    setState({ phase: "checking" });
    window.modbit.removeWorktree(sessionId, id, true).then(
      (preview) => live && setState({ phase: "confirm", preview }),
      (e: unknown) => live && setState({ phase: "refused", refusal: parseRefusal(e) }),
    );
    return () => {
      live = false;
    };
  }, [id, sessionId]);
  const remove = async (preview: WorktreeRemovalInfo) => {
    if (!worktree || !sessionId) return;
    setState({ phase: "removing", preview });
    try {
      const done = await window.modbit.removeWorktree(sessionId, worktree.worktreeId, false);
      onRemoved(done);
      onClose();
    } catch (e) {
      setState({ phase: "failed", preview, refusal: parseRefusal(e) });
    }
  };
  const owner = worktree?.taskId ? titleOf(worktree.taskId) : "";
  return (
    <Dialog open={worktree !== null} title={state.phase === "refused" || state.phase === "failed" ? "This worktree cannot be removed" : "Remove worktree"} role={state.phase === "confirm" ? "alertdialog" : "dialog"} onClose={onClose} testId="worktree-remove-dialog" initialFocus={cancel}>
      {worktree && <RemoveBody worktree={worktree} owner={owner} state={state} cancelRef={cancel} onCancel={onClose} onConfirm={(preview) => void remove(preview)} />}
    </Dialog>
  );
}

export interface DiscardDialogProps {
  worktree: WorktreeInfo | null;
  sessionId: string | null;
  onClose: () => void;
  onDiscarded: () => void;
}

export function DiscardWorktreeDialog({ worktree, sessionId, onClose, onDiscarded }: DiscardDialogProps) {
  const [typed, setTyped] = useState("");
  const [problem, setProblem] = useState<Refusal | null>(null);
  const [busy, setBusy] = useState(false);
  const input = useRef<HTMLInputElement>(null);
  const errId = useId();
  useEffect(() => {
    if (worktree) {
      setTyped("");
      setProblem(null);
      setBusy(false);
    }
  }, [worktree]);
  const discard = async () => {
    if (!worktree?.taskId || !sessionId) return;
    setBusy(true);
    setProblem(null);
    try {
      await window.modbit.discardWorktree(sessionId, worktree.taskId, "discarded from the worktree list", typed);
      onDiscarded();
      onClose();
    } catch (e) {
      setProblem(parseRefusal(e));
    } finally {
      setBusy(false);
    }
  };
  return (
    <Dialog open={worktree !== null} title="Discard this result" role="alertdialog" onClose={onClose} testId="worktree-discard-dialog" initialFocus={input}>
      {worktree && (
        <form
          className="wt-dialog"
          onSubmit={(e) => {
            e.preventDefault();
            if (!busy && typed.length > 0) void discard();
          }}
        >
          <p>
            This drops what the task changed in <code>{worktree.branch}</code> for good: {worktree.changedFiles} changed {worktree.changedFiles === 1 ? "file" : "files"}, never applied to your checkout. The worktree can then be removed.
          </p>
          <label className="proj-field">
            <span>
              Type the branch name <code>{worktree.branch}</code> to confirm
            </span>
            <input ref={input} type="text" value={typed} onChange={(e) => setTyped(e.target.value)} data-testid="worktree-discard-confirm-input" autoComplete="off" spellCheck={false} aria-describedby={problem ? errId : undefined} />
          </label>
          {problem && (
            <p id={errId} className="proj-problem" role="alert" data-testid="worktree-discard-problem">
              {problem.code ? `${problem.code}: ` : ""}
              {problem.detail}
            </p>
          )}
          <div className="dialog-actions">
            <Button type="button" onClick={onClose} data-testid="worktree-discard-cancel">
              Keep the result
            </Button>
            <Button type="submit" variant="danger" disabled={busy || typed.length === 0} data-testid="worktree-discard-confirm">
              Discard result
            </Button>
          </div>
        </form>
      )}
    </Dialog>
  );
}
