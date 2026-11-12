/**
 * The Pull request pane (REQ-PX-202; DR-PX-2026-11-11-021 MWS-10): the request, its checks and people's comments.
 * Opening or updating the request is the protected effect the Review drives (review/use-pull-request.ts): the Core answers with
 * the approval to decide, and pushes and opens only once the exact intent shown is approved. The checks and the comments are
 * what the Core recorded the last time it was asked to read the forge (`reviewBundle`); Refresh asks it to read again. The
 * renderer calls no forge. The request's state (open, merged, closed) is not shown anywhere: the Core has no read of it (PX-211).
 */
import { useCallback, useEffect, useState } from "react";
import { Button, Icon, IconButton } from "@modbit/ui";
import type { ReviewBundleView } from "../../preload/preload.ts";
import { usePullRequest } from "../review/use-pull-request.ts";
import { PaneActions } from "../shell/pane.tsx";
import { canOpenRequest, checkWords, commentWords, forgeErrorWords, refusedWords, requestWords } from "./pull-request-model.ts";

export interface PullRequestAppProps {
  sessionId: string;
  taskId: string;
  /** Changes whenever the task moves: the recorded checks and comments are read again. */
  refreshKey: string;
}

export function PullRequestApp({ sessionId, taskId, refreshKey }: PullRequestAppProps) {
  const [bundle, setBundle] = useState<ReviewBundleView | null>(null);
  const [readError, setReadError] = useState<string | null>(null);
  const load = useCallback(async () => {
    try {
      setBundle(await window.modbit.reviewBundle(taskId));
      setReadError(null);
    } catch (e) {
      setReadError((e as Error).message.replace(/^Error invoking remote method '[^']*': (Error: )?/, ""));
    }
  }, [taskId]);
  useEffect(() => {
    void load();
  }, [load, refreshKey]);
  const flow = usePullRequest(sessionId, taskId, "the pull request pane");
  const [refreshed, setRefreshed] = useState<{ checks: string | null; comments: string | null } | null>(null);
  const [refreshError, setRefreshError] = useState<string | null>(null);
  const [refreshing, setRefreshing] = useState(false);
  // Refresh asks the Core to read the forge's check runs, then the request's comments, and lists what it recorded.
  const refresh = async () => {
    if (refreshing) return;
    setRefreshing(true);
    setRefreshError(null);
    try {
      const ci = await window.modbit.ingestCiResults(sessionId, taskId);
      const comments = await window.modbit.ingestReviewComments(sessionId, taskId);
      setRefreshed({ checks: `${ci.checks} read for ${ci.commit.slice(0, 12)}${ci.refused > 0 ? `, ${ci.refused} not counted` : ""}`, comments: `${comments.steered} passed to the agent, ${comments.ignored} ignored, ${comments.alreadyTaken} already taken` });
      await load();
    } catch (e) {
      setRefreshError(forgeErrorWords((e as Error).message));
    } finally {
      setRefreshing(false);
    }
  };
  const pr = flow.pr;
  const request = requestWords(pr);
  const openable = bundle ? canOpenRequest(bundle.taskState) : "Reading the task…";
  const busy = flow.busy;
  return (
    <div className="pr-app" data-testid="app-pull-request" data-status={pr?.status ?? "NONE"} data-task-state={bundle?.taskState ?? ""}>
      <PaneActions className="pr-tools">
        <IconButton label="Read the checks and comments again" title="Refresh" icon={<Icon name="rotate-ccw" size={14} />} size="sm" disabled={refreshing} onClick={() => void refresh()} data-testid="pr-pane-refresh" />
      </PaneActions>
      {readError && (
        <p className="meta" role="alert" data-testid="pr-pane-read-error">
          The task's review could not be read: {readError}
        </p>
      )}
      <section className="pr-sec" aria-label="Request" data-testid="pr-pane-request">
        {request ? (
          <>
            <p className="pr-title" data-testid="pr-pane-opened" data-number={pr?.number} data-src="openPullRequest">
              {request.title}
            </p>
            <p className="meta pr-url" data-testid="pr-pane-url" data-src="openPullRequest">
              {request.where}
            </p>
            <p className="meta" data-testid="pr-pane-head" data-src="openPullRequest">
              {request.head} · {request.receipts}
            </p>
            <div className="pr-actions">
              <Button size="sm" variant="outline" disabled={busy || !bundle} onClick={() => bundle && void flow.open(true, bundle.workspaceRevision)} data-testid="pr-pane-update">
                {busy ? "Asking the Core…" : "Update the request"}
              </Button>
            </div>
          </>
        ) : pr?.status === "APPROVAL_PENDING" ? (
          <div className="pr-approval" role="group" aria-label="Approve opening the pull request" data-testid="pr-pane-approval" data-approval-id={pr.approvalId}>
            <p className="pr-title">Push the branch and open the pull request?</p>
            <p className="meta">
              Branch <code data-testid="pr-pane-branch">{pr.branch}</code>, revision <code>{pr.candidateRevision}</code>. The push and the request are one protected action; it runs only once you allow this exact intent:{" "}
              <code data-testid="pr-pane-intent" title={pr.intentHash}>
                {pr.intentHash.slice(0, 16)}
              </code>
            </p>
            <div className="pr-actions">
              <Button size="sm" variant="ghost" disabled={busy} onClick={() => bundle && void flow.decide(false, bundle.workspaceRevision)} data-testid="pr-pane-deny">
                Deny
              </Button>
              <Button size="sm" variant="primary" disabled={busy || !bundle} onClick={() => bundle && void flow.decide(true, bundle.workspaceRevision)} data-testid="pr-pane-approve">
                {busy ? "Asking the Core…" : "Allow and open"}
              </Button>
            </div>
          </div>
        ) : (
          <>
            <p className="meta" data-testid="pr-pane-none">
              {pr?.status === "DENIED" ? "Not opened: you denied it, so the branch stays on this machine." : pr?.status === "FAILED" ? `Not opened: ${pr.detail}` : "No pull request has been opened from this window for this task."}
            </p>
            <div className="pr-actions">
              <Button size="sm" variant="primary" disabled={busy || openable !== true} title={openable === true ? undefined : openable} onClick={() => bundle && void flow.open(false, bundle.workspaceRevision)} data-testid="pr-pane-open">
                {busy ? "Asking the Core…" : pr ? "Try again" : "Open pull request"}
              </Button>
              {openable !== true && bundle && (
                <span className="meta" data-testid="pr-pane-why">
                  {openable}
                </span>
              )}
            </div>
          </>
        )}
        {flow.error && (
          <p className="meta" role="alert" data-testid="pr-pane-error">
            {flow.error}
          </p>
        )}
      </section>
      {bundle && (
        <>
          <section className="pr-sec" aria-label="Checks" data-testid="pr-pane-checks" data-count={bundle.ciEvidence.length}>
            <h3 className="pr-h">
              Checks <span className="pr-h-aside">as the Core last read them</span>
            </h3>
            {bundle.ciEvidence.length === 0 && bundle.ciRejected.length === 0 ? (
              <p className="meta" data-testid="pr-pane-checks-none">
                No check has been read.
              </p>
            ) : (
              <ul className="pr-list">
                {bundle.ciEvidence.map((c, i) => {
                  const w = checkWords(c);
                  return (
                    <li key={`${c.runId}-${i}`} className="pr-row" data-testid="pr-pane-check" data-conclusion={c.conclusion || c.status} data-run-id={c.runId} data-tone={w.tone} data-src="reviewBundle">
                      <span className="pr-mark" data-tone={w.tone} aria-hidden="true">
                        <Icon name={w.tone === "ok" ? "check" : w.tone === "bad" ? "x" : "clock"} size={14} />
                      </span>
                      <span className="pr-x" title={`${w.name} · ${w.detail}`}>
                        {w.name} <span className="pr-dim">{w.detail}</span>
                      </span>
                      <span className="pr-result" data-tone={w.tone}>
                        {w.result}
                      </span>
                    </li>
                  );
                })}
                {bundle.ciRejected.map((r, i) => (
                  <li key={`refused-${i}`} className="pr-row pr-dim" data-testid="pr-pane-check-refused" data-src="reviewBundle">
                    {refusedWords(r)}
                  </li>
                ))}
              </ul>
            )}
          </section>
          <section className="pr-sec" aria-label="Comments" data-testid="pr-pane-comments" data-count={bundle.reviewComments.length}>
            <h3 className="pr-h">
              Comments <span className="pr-h-aside">other people's words, never instructions</span>
            </h3>
            {bundle.reviewComments.length === 0 ? (
              <p className="meta" data-testid="pr-pane-comments-none">
                No comment has been read.
              </p>
            ) : (
              <ul className="pr-list">
                {bundle.reviewComments.map((t) => {
                  const w = commentWords(t);
                  return (
                    <li key={`${t.commentId}-${t.inputId}-${t.disposition}`} className="pr-comment" data-testid="pr-pane-comment" data-disposition={t.disposition} data-answered={t.answered ? "true" : "false"} data-src="reviewBundle">
                      <span className="pr-comment-head">
                        <strong>{w.author}</strong>
                        {w.where && <code>{w.where}</code>}
                        <span className="pr-dim">{w.outcome}</span>
                      </span>
                      {t.body && (
                        <pre className="pr-comment-body" data-testid="pr-pane-comment-body">
                          {t.body}
                        </pre>
                      )}
                    </li>
                  );
                })}
              </ul>
            )}
          </section>
        </>
      )}
      {refreshed && !refreshError && (
        <p className="meta" role="status" data-testid="pr-pane-refreshed">
          Read just now. Checks: {refreshed.checks}. Comments: {refreshed.comments}.
        </p>
      )}
      {refreshError && (
        <p className="meta" role="alert" data-testid="pr-pane-refresh-error">
          {refreshError}
        </p>
      )}
    </div>
  );
}
