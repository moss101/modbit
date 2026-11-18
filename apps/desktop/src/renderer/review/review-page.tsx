/**
 * The Review page (REQ-PX-208, DR-PX-2026-11-11-021 MWS-14): the tasks' pull requests and their local changes not yet in one, and
 * beside them, for the chosen task, its change set, its findings and people's comments, all read from the Core's review bundle.
 * The page shape: the top bar names the page; a 45 px head carries the one primary action (Open review); the list at the left
 * has rows of at least 44 px with a 1 px edge. Hunk decisions stay in the task's own review (review.tsx), which Open review shows:
 * its accept, return and per-hunk reject go through `decideReview` exactly as before.
 *
 * Seams left for later rows, drawn as nothing until they exist: the pull request's state, checks and merge (PX-211..213: the
 * Core has no read of a request's state, so only a request this window opened is listed as one), and the reviewer's findings one
 * by one with Fix this one (PX-214).
 */
import { useCallback, useEffect, useMemo, useState } from "react";
import { Button, Icon } from "@modbit/ui";
import type { AgentHeaderView } from "../../shared/conversation-types.ts";
import type { ReviewBundleView } from "../../preload/preload.ts";
import { commentWords } from "../apps/pull-request-model.ts";
import { changeSet, checksWords, findingsOf, reviewRows, type ReviewRow } from "./review-model.ts";
import { answeredRequest, useForgeReads, watchAnswered } from "./use-pull-request.ts";

export interface ReviewPageProps {
  headers: readonly AgentHeaderView[];
  sessionId: string | null;
  connected: boolean;
  /** Shows the task's own review (its hunks and the decision) over this page; Back returns here. */
  onOpenReview: (taskId: string) => void;
  onOpenTask: (taskId: string) => void;
}

function Row({ r, chosen, onChoose }: { r: ReviewRow; chosen: boolean; onChoose: () => void }) {
  return (
    <li>
      <button type="button" className="rp-row" aria-current={chosen ? "true" : undefined} data-testid="review-page-row" data-task-id={r.taskId} data-kind={r.pr ? "pull" : "local"} onClick={onChoose}>
        <Icon name={r.pr ? "git-compare" : "git-branch"} />
        <span className="rp-row-main">
          <span className="rp-row-title">
            {r.pr ? `#${r.pr.number} ` : ""}
            {r.title}
          </span>
          <span className="rp-row-detail">
            {r.state} · {r.where} · {r.files} {r.files === 1 ? "file" : "files"}
          </span>
        </span>
        <span className="rp-size" aria-label={`${r.added} lines added, ${r.removed} removed`}>
          <span className="rp-add">+{r.added}</span> <span className="rp-del">−{r.removed}</span>
        </span>
      </button>
    </li>
  );
}

function Detail({ row, sessionId, bundle, error, reload, onOpenTask }: { row: ReviewRow; sessionId: string; bundle: ReviewBundleView | null; error: string | null; reload: () => Promise<void>; onOpenTask: () => void }) {
  const forge = useForgeReads(sessionId, row.taskId, reload);
  const files = bundle ? changeSet(bundle) : [];
  const findings = bundle ? findingsOf(bundle) : [];
  const checks = bundle ? checksWords(bundle) : null;
  return (
    <section className="rp-detail" aria-labelledby="rp-detail-title" data-testid="review-page-detail" data-task-id={row.taskId}>
      <div className="rp-detail-head">
        <h3 className="rp-detail-title" id="rp-detail-title">
          {row.pr ? `#${row.pr.number} ` : ""}
          {row.title}
        </h3>
        <Button size="sm" variant="ghost" onClick={onOpenTask} data-testid="review-page-open-task">
          Open task
        </Button>
      </div>
      {error && (
        <p className="meta" role="alert" data-testid="review-page-error">
          The Core could not read this task's review: {error}
        </p>
      )}
      {!bundle && !error && (
        <p className="meta" role="status">
          Reading the review from the Core…
        </p>
      )}
      {bundle && (
        <>
          <h4 className="rp-label">Changes</h4>
          {files.length === 0 ? (
            <p className="meta" data-testid="review-page-no-files">
              No file has changed.
            </p>
          ) : (
            <ul className="rp-list rp-files" data-testid="review-page-files">
              {files.map((f) => (
                <li key={f.path} className="rp-file" data-testid="review-page-file" data-path={f.path}>
                  <code className="rp-path">{f.path}</code>
                  <span className="meta">{f.status}</span>
                  <span className="rp-size">
                    <span className="rp-add">+{f.added}</span> <span className="rp-del">−{f.removed}</span>
                  </span>
                </li>
              ))}
            </ul>
          )}
          <h4 className="rp-label">Findings</h4>
          {checks && (
            <p className="meta" data-testid="review-page-checks">
              {checks}.
            </p>
          )}
          {findings.length === 0 ? (
            <p className="meta" data-testid="review-page-no-findings">
              The Core recorded no findings for this change.
            </p>
          ) : (
            <ul className="rp-list" data-testid="review-page-findings">
              {findings.map((f, i) => (
                <li key={`${f.kind}-${i}`} className="rp-finding" data-testid="review-page-finding" data-kind={f.kind} data-open={f.open ? "true" : "false"}>
                  <span className="rp-tag" data-kind={f.kind}>
                    {f.label}
                    {f.open ? "" : ", resolved"}
                  </span>
                  <span className="rp-finding-text">
                    {f.text}
                    {f.path && <code className="rp-path"> {f.path}</code>}
                  </span>
                </li>
              ))}
            </ul>
          )}
          <h4 className="rp-label">Comments from people</h4>
          {bundle.reviewComments.length === 0 ? (
            <p className="meta" data-testid="review-page-no-comments">
              {row.pr ? "No comment has been read from the pull request yet." : "Comments arrive once the changes are in a pull request."}
            </p>
          ) : (
            <ul className="rp-list" data-testid="review-page-comments">
              {bundle.reviewComments.map((c) => {
                const w = commentWords(c);
                return (
                  <li key={`${c.commentId}-${c.inputId}`} className="rp-finding" data-testid="review-page-comment">
                    <span className="rp-tag">{w.author}</span>
                    <span className="rp-finding-text">
                      {c.body}
                      <span className="meta">
                        {w.where ? ` ${w.where} · ` : " "}
                        {w.outcome}
                      </span>
                    </span>
                  </li>
                );
              })}
            </ul>
          )}
          {row.pr && (
            <div className="rp-actions">
              <Button size="sm" variant="outline" onClick={() => void forge.ingest("comments")} disabled={forge.busy} data-testid="review-page-read-comments">
                Read comments
              </Button>
              <Button size="sm" variant="outline" onClick={() => void forge.ingest("ci")} disabled={forge.busy} data-testid="review-page-read-checks">
                Read checks
              </Button>
              {forge.note && <span className="meta">{forge.note}</span>}
              {forge.error && (
                <span className="meta" role="alert">
                  {forge.error}
                </span>
              )}
            </div>
          )}
          <p className="meta rp-foot">Open review shows each change, to accept the task's work, return it, or reject single changes.</p>
        </>
      )}
    </section>
  );
}

export function ReviewPage({ headers, sessionId, connected, onOpenReview, onOpenTask }: ReviewPageProps) {
  const [known, setKnown] = useState(0);
  useEffect(() => watchAnswered(() => setKnown((n) => n + 1)), []);
  // eslint-disable-next-line react-hooks/exhaustive-deps
  const { pulls, local } = useMemo(() => reviewRows(headers, answeredRequest), [headers, known]);
  const all = useMemo(() => [...pulls, ...local], [pulls, local]);
  const [chosenId, setChosenId] = useState<string | null>(null);
  const chosen = all.find((r) => r.taskId === chosenId) ?? all[0] ?? null;
  const chosenHeader = chosen ? headers.find((h) => h.taskId === chosen.taskId) : undefined;
  const [bundle, setBundle] = useState<{ taskId: string; b: ReviewBundleView } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const taskId = chosen?.taskId ?? null;
  const load = useCallback(async () => {
    if (!taskId || !connected) return;
    try {
      const b = await window.modbit.reviewBundle(taskId);
      setBundle({ taskId, b });
      setError(null);
    } catch (e) {
      setError((e as Error).message.replace(/^Error invoking remote method '[^']*': (Error: )?/, ""));
    }
  }, [taskId, connected]);
  // Read again whenever the chosen task's log moves on (a hunk decided, a check run, a comment read).
  useEffect(() => {
    setError(null);
    void load();
  }, [load, chosenHeader?.lastOffset]);
  const shown = bundle && bundle.taskId === taskId ? bundle.b : null;
  return (
    <div className="page review-page" data-testid="review-page">
      <header className="rp-head">
        <h2 className="mb-sr-only">Review</h2>
        <span className="meta" data-testid="review-page-count">
          {all.length === 0 ? "Nothing to review" : `${all.length} ${all.length === 1 ? "task has" : "tasks have"} changes`}
        </span>
        <span className="rp-spacer" />
        <Button size="sm" variant="primary" disabled={!chosen || !sessionId} onClick={() => chosen && onOpenReview(chosen.taskId)} data-testid="review-page-open-review" title={chosen ? undefined : "Choose a task with changes"}>
          Open review
        </Button>
      </header>
      <div className="rp-body">
        <nav className="rp-col" aria-label="Changes to review">
          {pulls.length > 0 && (
            <>
              <h3 className="rp-label">Pull requests from tasks</h3>
              <ul className="rp-list" data-testid="review-page-pulls">
                {pulls.map((r) => (
                  <Row key={r.taskId} r={r} chosen={r.taskId === chosen?.taskId} onChoose={() => setChosenId(r.taskId)} />
                ))}
              </ul>
            </>
          )}
          <h3 className="rp-label">Local changes not in a pull request</h3>
          {local.length === 0 ? (
            <p className="meta" data-testid="review-page-empty">
              {pulls.length === 0 ? "No task has changes to review." : "Every task's changes are in a pull request."}
            </p>
          ) : (
            <ul className="rp-list" data-testid="review-page-local">
              {local.map((r) => (
                <Row key={r.taskId} r={r} chosen={r.taskId === chosen?.taskId} onChoose={() => setChosenId(r.taskId)} />
              ))}
            </ul>
          )}
        </nav>
        {chosen && sessionId ? (
          <Detail key={chosen.taskId} row={chosen} sessionId={sessionId} bundle={shown} error={error} reload={load} onOpenTask={() => onOpenTask(chosen.taskId)} />
        ) : (
          <section className="rp-detail" aria-label="Findings and comments">
            <p className="meta">When a task changes files, its changes, findings and comments show here.</p>
          </section>
        )}
      </div>
    </div>
  );
}
