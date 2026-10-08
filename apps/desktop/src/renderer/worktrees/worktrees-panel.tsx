/**
 * Worktree management (REQ-PX-068; docs/65 AFW-J07): the Core's worktree list
 * grouped by source repository with each worktree's state, size, age and
 * owner task, the retention policy in force, the reaper's attention lines and
 * the last cleanup's report, and the actions that are the Core's to allow:
 * apply to the checkout, undo an apply, discard a result, remove one worktree.
 * Everything shown is the Core's answer to `ListWorktrees`; the panel
 * inspects no disk, runs no Git and decides no eligibility.
 */
import { useMemo, useState } from "react";
import { Badge, Button } from "@modbit/ui";
import type { WorktreeInfo } from "../../shared/project-types.ts";
import { parseRefusal } from "../projects/project-model.ts";
import { ApplyModal } from "./apply-modal.tsx";
import { DiscardWorktreeDialog, RemoveWorktreeDialog } from "./remove-dialog.tsx";
import { ageWords, dispositionWord, groupByRepository, reportWords, sizeWords, worktreeStates } from "./worktree-model.ts";
import type { WorktreesState } from "./use-worktrees.ts";

export interface WorktreesPanelProps {
  state: WorktreesState;
  sessionId: string | null;
  titleOf: (taskId: string) => string;
  nowMs: number;
  onOpenTask: (taskId: string) => void;
  announce: (message: string) => void;
}

export function WorktreesPanel({ state, sessionId, titleOf, nowMs, onOpenTask, announce }: WorktreesPanelProps) {
  const { list, loaded, error } = state;
  const [removing, setRemoving] = useState<WorktreeInfo | null>(null);
  const [discarding, setDiscarding] = useState<WorktreeInfo | null>(null);
  const [applying, setApplying] = useState<{ worktree: WorktreeInfo; flow: "apply" | "undo" } | null>(null);
  const [busy, setBusy] = useState(false);
  const [note, setNote] = useState<string | null>(null);
  const groups = useMemo(() => groupByRepository(list?.worktrees ?? []), [list?.worktrees]);

  const runCleanup = async (dryRun: boolean) => {
    if (!sessionId) return;
    setBusy(true);
    setNote(null);
    try {
      const r = await window.modbit.runWorktreeCleanup(sessionId, { dryRun });
      announce(reportWords(r));
      await state.refresh();
    } catch (e) {
      // A second cleanup while one is running is the Core's typed refusal, named with who holds the lease.
      const r = parseRefusal(e);
      setNote(`${r.code ? `${r.code}: ` : ""}${r.detail}`);
    } finally {
      setBusy(false);
    }
  };

  if (!loaded) {
    return (
      <section className="wt-page" aria-label="Worktrees" data-testid="worktrees-page" data-state={error ? "error" : "loading"}>
        <p className="meta" role="status">
          {error ? `The worktree list is unavailable: ${error}` : "Reading the worktrees…"}
        </p>
      </section>
    );
  }
  const policy = list?.policy;
  const last = list?.lastCleanup;
  return (
    <section className="wt-page" aria-labelledby="wt-title" data-testid="worktrees-page" data-state="ready">
      <header className="wt-head">
        <h2 id="wt-title" className="proj-title">
          Worktrees
        </h2>
        <span className="meta" data-testid="worktrees-summary">
          {list?.count ?? 0} on disk · {sizeWords(list?.totalBytes ?? "0")}
        </span>
        <span className="proj-actions">
          <Button size="sm" onClick={() => void state.refresh()} data-testid="worktrees-refresh">
            Refresh
          </Button>
          <Button size="sm" disabled={busy || !sessionId} onClick={() => void runCleanup(true)} data-testid="worktrees-dry-run">
            Preview cleanup
          </Button>
          <Button size="sm" variant="primary" disabled={busy || !sessionId} onClick={() => void runCleanup(false)} data-testid="worktrees-cleanup">
            Run cleanup
          </Button>
        </span>
      </header>
      {policy && (
        <p className="meta" data-testid="worktrees-policy">
          The Core keeps at most {policy.maxWorktrees} worktrees and {sizeWords(policy.maxBytes)}, and removes only when those are exceeded, oldest unused first. A worktree younger than {Math.round(policy.protectMs / 60_000)} min, or with a running task, a dirty tree or an unapplied result, is never removed by a cleanup.
          {list && list.nextCleanupAtMs > 0 ? ` Next scheduled cleanup: ${new Date(list.nextCleanupAtMs).toLocaleString()}.` : ""}
        </p>
      )}
      {note && (
        <p className="proj-problem" role="alert" data-testid="worktrees-note">
          {note}
        </p>
      )}
      {list && list.attention.length > 0 && (
        <section aria-label="Needs a decision" className="wt-attention" data-testid="worktrees-attention">
          <h3 className="proj-h2">Needs your decision</h3>
          <ul>
            {list.attention.map((line) => (
              <li key={line} data-testid="worktrees-attention-line">
                {line}
              </li>
            ))}
          </ul>
        </section>
      )}
      {last && (
        <p className="meta" data-testid="worktrees-report" data-status={last.status}>
          Last cleanup: {reportWords(last)}
          {last.errors.length > 0 ? ` · ${last.errors.length} ${last.errors.length === 1 ? "error" : "errors"}: ${last.errors.join("; ")}` : ""}
        </p>
      )}
      {groups.length === 0 ? (
        <p className="meta empty" data-testid="worktrees-empty">
          No worktree exists. A task started in its own worktree appears here.
        </p>
      ) : (
        groups.map((g) => (
          <section key={g.origin || "none"} aria-label={`Worktrees of ${g.label}`} className="wt-group" data-testid="worktrees-group" data-origin={g.origin}>
            <h3 className="proj-h2" title={g.origin}>
              {g.label} <span className="meta">{g.items.length}</span>
            </h3>
            <ul className="wt-list">
              {g.items.map((w) => {
                const states = worktreeStates(w);
                const owner = w.taskId ? titleOf(w.taskId) : "";
                const canApply = w.taskId !== "" && w.unapplied && !w.taskRunning && w.state === "ACTIVE";
                const canUndo = w.taskId !== "" && w.disposition === "APPLIED";
                const canDiscard = w.taskId !== "" && w.unapplied && !w.taskRunning && w.state === "ACTIVE";
                return (
                  <li key={w.worktreeId} className="wt-row" data-testid="worktree-row" data-worktree-id={w.worktreeId} data-task-id={w.taskId} data-states={states.map((s) => s.id).join(" ")} data-removable={w.removable} data-running={w.taskRunning}>
                    <div className="wt-main">
                      <strong data-testid="worktree-owner">{owner || (w.orphan ? "No task owns this" : w.worktreeId)}</strong>
                      {states.map((s) => (
                        <Badge key={s.id} tone={s.tone === "danger" ? "danger" : s.tone === "warn" ? "warn" : s.tone === "ok" ? "ok" : "info"}>
                          <span data-testid="worktree-state" data-state={s.id} title={s.detail}>
                            {s.label}
                          </span>
                        </Badge>
                      ))}
                      {w.disposition && (
                        <Badge tone="info">
                          <span data-testid="worktree-disposition">{dispositionWord(w.disposition)}</span>
                        </Badge>
                      )}
                    </div>
                    <p className="meta wt-facts" data-testid="worktree-facts">
                      <span data-testid="worktree-branch">{w.branch || w.kind}</span> · {w.changedFiles} changed {w.changedFiles === 1 ? "file" : "files"} · <span data-testid="worktree-size">{sizeWords(w.bytes)}</span> · <span data-testid="worktree-age">{ageWords(w.createdAtMs, nowMs)}</span> old
                    </p>
                    <p className="meta wt-path" data-testid="worktree-path">
                      {w.path}
                    </p>
                    {states.map((s) => (
                      <p key={s.id} className="meta wt-why" data-testid="worktree-why" data-state={s.id}>
                        {s.label}: {s.detail}
                      </p>
                    ))}
                    <div className="wt-actions">
                      {w.taskId && (
                        <Button size="sm" onClick={() => onOpenTask(w.taskId)} data-testid="worktree-open-task">
                          Open task
                        </Button>
                      )}
                      {canApply && (
                        <Button size="sm" variant="primary" onClick={() => setApplying({ worktree: w, flow: "apply" })} data-testid="worktree-apply">
                          Apply to my checkout…
                        </Button>
                      )}
                      {canUndo && (
                        <Button size="sm" onClick={() => setApplying({ worktree: w, flow: "undo" })} data-testid="worktree-undo">
                          Undo apply…
                        </Button>
                      )}
                      {canDiscard && (
                        <Button size="sm" onClick={() => setDiscarding(w)} data-testid="worktree-discard">
                          Discard result…
                        </Button>
                      )}
                      <Button size="sm" variant="danger" onClick={() => setRemoving(w)} data-testid="worktree-remove">
                        Remove…
                      </Button>
                    </div>
                  </li>
                );
              })}
            </ul>
          </section>
        ))
      )}
      <RemoveWorktreeDialog
        worktree={removing}
        sessionId={sessionId}
        titleOf={titleOf}
        onClose={() => setRemoving(null)}
        onRemoved={(r) => {
          announce(`Removed ${r.path}.`);
          void state.refresh();
        }}
      />
      <DiscardWorktreeDialog worktree={discarding} sessionId={sessionId} onClose={() => setDiscarding(null)} onDiscarded={() => void state.refresh()} />
      <ApplyModal worktree={applying?.worktree ?? null} flow={applying?.flow ?? "apply"} sessionId={sessionId} titleOf={titleOf} onClose={() => setApplying(null)} onChanged={() => void state.refresh()} />
    </section>
  );
}
