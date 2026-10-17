import { useCallback, useEffect, useRef, useState } from "react";
import { Badge, Button } from "@modbit/ui";
import type { CodeView, ReviewBundleView, ReviewFileView } from "../../preload/preload.ts";
import type { ChangeEffectInfo, ChangesStateInfo, CommitPushAckInfo, RevertAckInfo } from "../../shared/changes-types.ts";
import { parseRefusal } from "../projects/project-model.ts";
import { canCommitPush, changeSummary, commitLabel, commitMode, effectWords, isReviewable, lineKind, newCommandId, rangesContain, refusalWords, stageState } from "./changes-model.ts";

/**
 * The Changes app (REQ-PX-048, PX-104's reachability half): the task's live,
 * revision-bound diff in every task state, with the actions on it. The file
 * list and hunks are the Core's review bundle; the index, the branch, the
 * remote and the earlier Commit and push attempts are the Core's changes
 * state; a file's full text is the Core's code view. Nothing is decided or
 * written here: a revert is the Core's ChangeTransaction bound to the file
 * revision this view read (a file edited since is refused STALE_REVISION, a
 * path with staged changes DIRTY_CONFLICT, a protected path PROTECTED_PATH),
 * and a checkpoint taken first makes it undoable; staging moves the git index;
 * Commit and push is one protected effect whose approval card shows the exact
 * intent the Core read from the repository. Decisions on hunks of a task that
 * is ready for review stay where the Core accepts them: the Review screen.
 */
export function ChangesApp({ sessionId, taskId, taskState, refreshKey, onOpenReview }: { sessionId: string; taskId: string; taskState: string; refreshKey: string; onOpenReview: () => void }) {
  const [bundle, setBundle] = useState<ReviewBundleView | null>(null);
  const [state, setState] = useState<ChangesStateInfo | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [path, setPath] = useState<string | null>(null);
  const [full, setFull] = useState<CodeView | null>(null);
  const [fullError, setFullError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [refusal, setRefusal] = useState<{ code: string; detail: string } | null>(null);
  const [undo, setUndo] = useState<{ ack: RevertAckInfo; note: string } | null>(null);
  const [message, setMessage] = useState("");
  const [commit, setCommit] = useState<CommitPushAckInfo | null>(null);
  const seq = useRef(0);

  const load = useCallback(async () => {
    const mine = ++seq.current;
    try {
      const [b, s] = await Promise.all([window.modbit.reviewBundle(taskId), window.modbit.changesState(taskId)]);
      if (mine !== seq.current) return;
      setBundle(b);
      setState(s);
      setError(null);
    } catch (e) {
      if (mine === seq.current) setError((e as Error).message);
    }
  }, [taskId]);
  // The diff follows the task: it is read again whenever the task moves (the caller's refresh key) and on demand.
  useEffect(() => {
    void load();
  }, [load, refreshKey]);
  useEffect(() => {
    setPath(null);
    setFull(null);
    setRefusal(null);
    setUndo(null);
    setCommit(null);
    setMessage("");
  }, [taskId]);

  const file: ReviewFileView | undefined = bundle?.files.find((f) => f.path === path) ?? bundle?.files[0];
  const showFull = async () => {
    if (!file) return;
    setFullError(null);
    try {
      setFull(await window.modbit.codeView(taskId, file.path, file.fileRevision));
    } catch (e) {
      setFull(null);
      setFullError((e as Error).message);
    }
  };

  /** Run one action: a refusal is shown in the Core's own words, and the view is read again either way. */
  const act = async <T,>(run: () => Promise<T>): Promise<T | null> => {
    setBusy(true);
    setRefusal(null);
    try {
      return await run();
    } catch (e) {
      setRefusal(parseRefusal(e));
      return null;
    } finally {
      setBusy(false);
      await load();
    }
  };

  const revert = async (f: ReviewFileView, wholeFile: boolean, hunkIndex: number) => {
    const ack = await act(() => window.modbit.revertChanges(sessionId, taskId, { path: f.path, wholeFile, hunkIndex, expectedFileRevision: f.fileRevision, commandId: newCommandId() }));
    if (ack) {
      setUndo({ ack, note: `${wholeFile ? "Reverted" : `Reverted hunk ${hunkIndex + 1} of`} ${ack.path}.` });
      setFull(null);
    }
  };
  const undoRevert = async () => {
    if (!undo) return;
    const done = await act(() => window.modbit.restoreCheckpoint(sessionId, taskId, { checkpointId: undo.ack.checkpointId }, { commandId: newCommandId() }));
    if (done?.restored) setUndo(null);
    else if (done) setRefusal({ code: done.refusal || "RESTORE_REFUSED", detail: done.detail });
  };
  const toggleStage = async (p: ChangesStateInfo["paths"][number]) => {
    const wantStage = stageState(p) !== "staged";
    await act(() => window.modbit.stageChanges(sessionId, taskId, [{ path: p.path, expectedFileRevision: wantStage ? p.fileRevision : "" }], wantStage));
  };

  const mode = state ? commitMode(state.paths.filter((p) => p.staged !== "").length, state.unpushed) : "none";
  const askToCommit = async () => {
    if (!state) return;
    const ack = await act(() => window.modbit.commitAndPush(sessionId, taskId, { message, expectedHead: state.head, expectedStagedDigest: state.stagedDigest }));
    if (ack) setCommit(ack);
  };
  const decide = async (ack: CommitPushAckInfo, ok: boolean) => {
    setBusy(true);
    setRefusal(null);
    try {
      await window.modbit.resolveApproval(sessionId, ack.approvalId, ok, ok ? "approved in the Changes app" : "denied in the Changes app", ack.intentHash);
      if (!ok) {
        setCommit({ ...ack, status: "DENIED", detail: "You denied the approval; nothing was committed or pushed." });
        return;
      }
      // Calling again with the same arguments is what runs it, now that the approval stands.
      setCommit(await window.modbit.commitAndPush(sessionId, taskId, { message: ack.message, expectedHead: ack.headBefore, expectedStagedDigest: ack.stagedDigest }));
      if (ack.pushOnly === false) setMessage("");
    } catch (e) {
      // The call did not finish (the Core stopped, or refused): the card is not a live decision any more. What the Core holds is listed below.
      setRefusal(parseRefusal(e));
      setCommit(null);
    } finally {
      setBusy(false);
      await load();
    }
  };
  // An approval card is shown only while the Core still holds that approval as pending: after a restart or a decision made elsewhere it goes, and the list below says how the attempt stands.
  useEffect(() => {
    if (state && commit?.status === "APPROVAL_PENDING" && !state.effects.some((e) => e.approvalId === commit.approvalId && e.state === "APPROVAL_PENDING")) setCommit(null);
  }, [state, commit]);

  if (error) {
    return (
      <p className="meta" role="status" data-testid="changes-error">
        The change set could not be read: {error}
      </p>
    );
  }
  if (!bundle || !state) {
    return (
      <p className="meta" role="status" data-testid="changes-loading">
        Reading the change set…
      </p>
    );
  }
  const stateOf = (p: string) => state.paths.find((x) => x.path === p);
  const locked = busy || state.taskRunning;
  return (
    <div className="changes-app" data-testid="app-changes" data-task-state={taskState} data-workspace-revision={bundle.workspaceRevision} data-staged-digest={state.stagedDigest}>
      <div className="changes-head">
        <span data-testid="changes-summary">{changeSummary(bundle.files)}</span>
        <Badge tone={isReviewable(taskState) ? "ok" : "info"}>{taskState}</Badge>
        <Button size="sm" onClick={() => void load()} data-testid="changes-refresh">
          Refresh
        </Button>
        <Button size="sm" variant="primary" onClick={onOpenReview} data-testid="panel-open-review">
          {isReviewable(taskState) ? "Open review" : "Open review (read-only)"}
        </Button>
      </div>
      <p className="meta" data-testid="changes-note">
        {isReviewable(taskState)
          ? "This task is ready for review: accept or return it, hunk by hunk, in the review."
          : "The task is still working. This is its change set as of workspace revision " + bundle.workspaceRevision + "; a decision on a hunk is taken once the task is ready for review."}
      </p>
      <p className="meta" data-testid="changes-branch" data-branch={state.branch} data-remote={state.remote} data-unpushed={state.unpushed}>
        {state.branch ? `On ${state.branch}` : "HEAD is detached"}
        {state.remote ? ` → ${state.remote} (${state.remoteUrl})` : " · no remote"}
        {state.unpushed > 0 ? ` · ${state.unpushed} ${state.unpushed === 1 ? "commit" : "commits"} not on the remote` : ""}
      </p>
      {state.taskRunning && (
        <p className="meta" role="status" data-testid="changes-locked">
          The task is running and owns its files; revert, stage and commit wait until it stops.
        </p>
      )}
      {refusal && (
        <div className="changes-refusal" role="alert" data-testid="changes-refusal" data-code={refusal.code}>
          <p>
            <strong>{refusal.code || "REFUSED"}</strong> {refusalWords(refusal.code)}
          </p>
          {refusal.detail && <p className="meta">{refusal.detail}</p>}
          <Button size="sm" onClick={() => { setRefusal(null); void load(); }} data-testid="changes-refusal-reload">
            Reload the changes
          </Button>
        </div>
      )}
      {undo && (
        <div className="changes-undo" role="status" data-testid="changes-undo" data-checkpoint={undo.ack.checkpointId}>
          <span data-testid="changes-undo-note">{undo.note}</span>{" "}
          <Button size="sm" onClick={() => void undoRevert()} disabled={locked} data-testid="changes-undo-button">
            Undo
          </Button>
        </div>
      )}
      {bundle.files.length === 0 ? (
        <p className="meta empty" data-testid="changes-empty">
          No file has changed yet.
        </p>
      ) : (
        <>
          <ul className="changes-files" aria-label="Changed files" data-testid="changes-files">
            {bundle.files.map((f) => {
              const s = stateOf(f.path);
              const staged = s ? stageState(s) : "none";
              return (
                <li key={f.path} className="changes-row" data-testid="changes-row" data-path={f.path} data-staged={staged} data-file-revision={s?.fileRevision ?? ""}>
                  <input
                    type="checkbox"
                    checked={staged === "staged"}
                    ref={(el) => {
                      if (el) el.indeterminate = staged === "partial";
                    }}
                    disabled={locked || !s || s.unmerged || s.protectedPath}
                    aria-label={`Stage ${f.path}`}
                    data-testid="changes-stage"
                    onChange={() => s && void toggleStage(s)}
                  />
                  <button type="button" className="changes-file" aria-pressed={f.path === file?.path} data-testid="changes-file" data-path={f.path} onClick={() => { setPath(f.path); setFull(null); setFullError(null); }}>
                    <span className="changes-status">{f.status}</span> {f.path}
                    <span className="meta"> · {f.binary ? "binary" : `${f.hunks.length} ${f.hunks.length === 1 ? "hunk" : "hunks"}`}{staged === "staged" ? " · staged" : staged === "partial" ? " · partly staged" : ""}</span>
                  </button>
                  <Button size="sm" onClick={() => void revert(f, true, 0)} disabled={locked} aria-label={`Revert ${f.path}`} data-testid="changes-revert-file">
                    Revert
                  </Button>
                </li>
              );
            })}
          </ul>
          {file && (
            <section aria-label={`Diff of ${file.path}`} data-testid="changes-diff" data-path={file.path} data-file-revision={file.fileRevision}>
              {file.binary ? (
                <p className="meta">Binary file: its change is not shown as text.</p>
              ) : (
                file.hunks.map((h) => (
                  <div key={h.index} className="hunk" data-testid="changes-hunk" data-index={h.index}>
                    <div className="hunk-head">
                      <span>{h.header}</span>
                      <Button size="sm" onClick={() => void revert(file, false, h.index)} disabled={locked} aria-label={`Revert hunk ${h.index + 1} of ${file.path}`} data-testid="changes-revert-hunk">
                        Revert hunk
                      </Button>
                    </div>
                    <pre>
                      {h.lines.map((l, i) => (
                        <span key={i} className={lineKind(l)}>
                          {l}
                          {"\n"}
                        </span>
                      ))}
                    </pre>
                  </div>
                ))
              )}
              {!file.binary && file.status !== "D" && (
                <Button size="sm" onClick={() => void showFull()} data-testid="changes-show-full">
                  Show the full file
                </Button>
              )}
              {fullError && (
                <p className="meta" role="status" data-testid="changes-full-error">
                  {fullError}
                </p>
              )}
              {full && (
                <div data-testid="changes-full" data-stale={full.stale ? "true" : "false"} data-file-revision={full.fileRevision}>
                  <p className="meta">
                    {full.path} · {full.syntaxLanguage} · file revision {full.fileRevision.slice(0, 12)}
                    {full.stale ? " · the file changed since the diff was read; refresh" : ""}
                  </p>
                  {full.text === "" ? (
                    <p className="meta">The file is too large to show inline; its content is held by the Core as {full.contentRef.slice(0, 12)}.</p>
                  ) : (
                    <pre className="codeview">
                      {full.text.split("\n").map((line, i) => (
                        <span key={i} className={rangesContain(full.changedRanges, i + 1) ? "add" : "ctx"} data-changed={rangesContain(full.changedRanges, i + 1) ? "true" : "false"}>
                          {String(i + 1).padStart(4, " ")}  {line}
                          {"\n"}
                        </span>
                      ))}
                    </pre>
                  )}
                </div>
              )}
            </section>
          )}
        </>
      )}
      <section className="changes-commit" aria-label="Commit and push" data-testid="changes-commit" data-mode={mode}>
        <h3>Commit and push</h3>
        {commit?.status === "APPROVAL_PENDING" ? (
          <ApprovalCard ack={commit} busy={busy} onDecide={(ok) => void decide(commit, ok)} />
        ) : (
          <>
            {mode !== "push" && (
              <>
                <label htmlFor="changes-message" className="meta">
                  Commit message
                </label>
                <textarea id="changes-message" data-testid="changes-commit-message" rows={3} value={message} disabled={locked} onChange={(e) => setMessage(e.target.value)} placeholder={mode === "none" ? "Stage files to commit them" : "Describe the change"} />
              </>
            )}
            <div className="changes-commit-actions">
              <Button variant="primary" size="sm" disabled={locked || !canCommitPush(mode, message, state.taskRunning)} onClick={() => void askToCommit()} data-testid="changes-commit-push">
                {commitLabel(mode, state.unpushed)}
              </Button>
              <span className="meta" data-testid="changes-commit-hint">
                {mode === "commit" ? `${state.paths.filter((p) => p.staged !== "").length} staged · asks for your approval first` : mode === "push" ? "Nothing is staged; this pushes the commits the remote lacks, after your approval." : "Nothing is staged and the remote lacks no commit."}
              </span>
            </div>
            {state.identity === "" && <p className="meta">No git identity is configured here, so the commit is authored as Modbit.</p>}
          </>
        )}
        {commit && commit.status !== "APPROVAL_PENDING" && <CommitResult ack={commit} onDismiss={() => setCommit(null)} />}
      </section>
      {state.effects.length > 0 && (
        <section className="changes-effects" aria-label="Earlier commits and pushes" data-testid="changes-effects">
          <h3>Earlier attempts</h3>
          <ul>
            {state.effects.map((e) => (
              <EffectRow key={e.toolCallId} e={e} />
            ))}
          </ul>
        </section>
      )}
    </div>
  );
}

function ApprovalCard({ ack, busy, onDecide }: { ack: CommitPushAckInfo; busy: boolean; onDecide: (ok: boolean) => void }) {
  const deny = useRef<HTMLButtonElement>(null);
  useEffect(() => deny.current?.focus(), []);
  return (
    <div className="changes-approval" role="group" aria-label="Approve this commit and push" data-testid="changes-approval" data-approval-id={ack.approvalId} data-intent-hash={ack.intentHash}>
      <p>
        <strong>{ack.pushOnly ? "Approve this push" : "Approve this commit and push"}</strong> — nothing has run yet. Approving covers exactly this and nothing else.
      </p>
      <dl className="apply-facts">
        <div>
          <dt>{ack.pushOnly ? "Latest commit" : "Message"}</dt>
          <dd data-testid="changes-approval-message">{ack.message}</dd>
        </div>
        <div>
          <dt>{ack.pushOnly ? "Files in the commits" : "Files"}</dt>
          <dd>
            <ul data-testid="changes-approval-files">
              {ack.paths.map((p) => (
                <li key={p}>
                  <code>{p}</code>
                </li>
              ))}
            </ul>
          </dd>
        </div>
        <div>
          <dt>Branch</dt>
          <dd>
            <code data-testid="changes-approval-branch">{ack.branch}</code>
          </dd>
        </div>
        <div>
          <dt>Remote</dt>
          <dd>
            <code data-testid="changes-approval-remote">{ack.remote}</code> <span className="meta" data-testid="changes-approval-remote-url">{ack.remoteUrl}</span>
          </dd>
        </div>
        <div>
          <dt>Onto</dt>
          <dd>
            <code>{ack.headBefore.slice(0, 12)}</code>
          </dd>
        </div>
        <div>
          <dt>Approves exactly</dt>
          <dd>
            <code data-testid="changes-approval-intent">{ack.intentHash.slice(0, 12)}</code>
          </dd>
        </div>
      </dl>
      <div className="dialog-actions">
        <Button ref={deny} onClick={() => onDecide(false)} disabled={busy} data-testid="changes-deny">
          Deny
        </Button>
        <Button variant="primary" onClick={() => onDecide(true)} disabled={busy} data-testid="changes-approve">
          {ack.pushOnly ? "Approve push" : "Approve commit and push"}
        </Button>
      </div>
    </div>
  );
}

function CommitResult({ ack, onDismiss }: { ack: CommitPushAckInfo; onDismiss: () => void }) {
  const done = ack.status === "COMPLETED";
  return (
    <div className="changes-result" role="status" data-testid="changes-result" data-status={ack.status} data-code={ack.code}>
      {done ? (
        <p>
          {ack.pushOnly ? "Pushed" : "Committed"} <code data-testid="changes-result-commit">{ack.commit.slice(0, 12)}</code> to <code>{ack.remote}/{ack.branch}</code>.
          {ack.replayed ? " (the remote already had it)" : ""}
        </p>
      ) : (
        <p>
          {ack.status === "DENIED" ? "Denied." : `${ack.code || "Failed"}.`} {ack.code ? refusalWords(ack.code) : ""}
        </p>
      )}
      {ack.detail && <p className="meta">{ack.detail}</p>}
      {ack.effectReceiptIds.length > 0 && (
        <p className="meta" data-testid="changes-result-receipts">
          {ack.effectReceiptIds.length} effect {ack.effectReceiptIds.length === 1 ? "receipt" : "receipts"} on the log.
        </p>
      )}
      <Button size="sm" onClick={onDismiss} data-testid="changes-result-dismiss">
        Dismiss
      </Button>
    </div>
  );
}

function EffectRow({ e }: { e: ChangeEffectInfo }) {
  return (
    <li className="changes-effect" data-testid="changes-effect" data-state={e.state} data-approval-id={e.approvalId} data-has-result-receipt={e.resultReceipt !== "" ? "true" : "false"}>
      <span>
        {e.pushOnly ? "Push" : "Commit and push"}: <q>{e.message}</q> ({e.paths.length} {e.paths.length === 1 ? "file" : "files"}, {e.branch} → {e.remote})
      </span>{" "}
      <Badge tone={e.state === "COMPLETED" ? "ok" : e.state === "AUTHORIZED_IN_DOUBT" || e.state === "FAILED" ? "warn" : "info"}>{e.state === "AUTHORIZED_IN_DOUBT" ? "in doubt" : e.state.toLowerCase().replace("_", " ")}</Badge>
      <p className="meta" data-testid="changes-effect-words">
        {effectWords(e)}
        {e.authorizedReceipt ? ` Authorized receipt ${e.authorizedReceipt.slice(0, 10)}` : ""}
        {e.resultReceipt ? `, result receipt ${e.resultReceipt.slice(0, 10)} (${e.resultStatus})` : e.authorizedReceipt ? ", no result receipt." : ""}
      </p>
    </li>
  );
}
