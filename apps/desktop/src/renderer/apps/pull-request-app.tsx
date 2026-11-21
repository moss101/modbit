/**
 * The Pull request pane (REQ-PX-202; DR-PX-2026-11-11-021 MWS-10): the request, its state, its checks and people's comments.
 * Opening or updating the request is the protected effect the Review drives (review/use-pull-request.ts): the Core answers with
 * the approval to decide, and pushes and opens only once the exact intent shown is approved. The request's number and its state
 * (open, merged, closed, and its checks) are what the Core recorded and last read through the forge adapter (REQ-PX-211, MWS-17:
 * the task's header, or the answer to Refresh), each with when it was read; before a read the pane says so and names no state.
 * The checks and the comments are what the Core recorded the last time it was asked to read the forge (`reviewBundle`). Refresh
 * asks it to read all three again. The renderer calls no forge. Auto-fix (REQ-PX-212) is the task's switch held by the Core: with it
 * on, the Core asks for a repair turn under the task's own run mode when it reads failing checks or accepts a comment; the pane
 * shows the Core's count and says when the cap stopped it. It starts nothing itself.
 */
import { useCallback, useEffect, useState } from "react";
import { Button, Icon, IconButton } from "@modbit/ui";
import type { ReviewBundleView } from "../../preload/preload.ts";
import type { PullRequestStateView } from "../../shared/conversation-types.ts";
import { relativeAge } from "../agents/list-model.ts";
import { usePullRequest } from "../review/use-pull-request.ts";
import { PaneActions } from "../shell/pane.tsx";
import { useDelivered } from "./delivered.ts";
import { approvalOutcome, autoFixWords, canOpenRequest, checkWords, commentWords, forgeErrorWords, refusedWords, requestStateWords, requestWords } from "./pull-request-model.ts";

export interface PullRequestAppProps {
  sessionId: string;
  taskId: string;
  /** Changes whenever the task moves: the recorded checks and comments are read again. */
  refreshKey: string;
  /** The task's pull request as its header carries it (REQ-PX-211): the Core's record and its latest read; absent when none. */
  pullRequest?: PullRequestStateView | undefined;
}

/** The newer of two reads of the same request (by the offset of their records). */
function newer(a: PullRequestStateView | undefined, b: PullRequestStateView | null): PullRequestStateView | undefined {
  if (!b) return a;
  if (!a) return b;
  return BigInt(b.readOffset || "0") > BigInt(a.readOffset || "0") ? b : a;
}

export function PullRequestApp({ sessionId, taskId, refreshKey, pullRequest }: PullRequestAppProps) {
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
  // REQ-PX-211: the answer to this window's last read of the request's state, and whether that read archived the task.
  const [stateRead, setStateRead] = useState<{ taskId: string; pr: PullRequestStateView | null; archived: boolean } | null>(null);
  const [stateError, setStateError] = useState<string | null>(null);
  const [undone, setUndone] = useState<string | null>(null);
  const [nowMs, setNowMs] = useState(() => Date.now());
  useEffect(() => {
    const t = setInterval(() => setNowMs(Date.now()), 30_000);
    return () => clearInterval(t);
  }, []);
  // A header the list no longer holds (the task was archived, or the list is filtered) is read here, archived ones included, each
  // time the task moves: the pane goes on showing the Core's latest read of the request instead of none.
  const [ownHeader, setOwnHeader] = useState<{ taskId: string; pr: PullRequestStateView | undefined } | null>(null);
  const listed = pullRequest !== undefined;
  useEffect(() => {
    if (listed || !sessionId) return;
    let live = true;
    void window.modbit
      .agentHeaders(sessionId, true)
      .then((v) => live && setOwnHeader({ taskId, pr: v.headers.find((h) => h.taskId === taskId)?.pullRequest }))
      .catch(() => {});
    return () => {
      live = false;
    };
  }, [listed, sessionId, taskId, refreshKey]);
  const header = pullRequest ?? (ownHeader?.taskId === taskId ? ownHeader.pr : undefined);
  const mine = stateRead?.taskId === taskId ? stateRead : null;
  const current = newer(header, mine?.pr ?? null);
  const state = requestStateWords(current);
  // REQ-PX-212: the switch's answer from the Core, newer than the header until the header catches up (by the record's offset).
  const [autoFixSet, setAutoFixSet] = useState<{ taskId: string; enabled: boolean; attempts: number; cap: number; stopped: boolean } | null>(null);
  const [autoFixError, setAutoFixError] = useState<string | null>(null);
  const [autoFixBusy, setAutoFixBusy] = useState(false);
  const mineFix = autoFixSet?.taskId === taskId ? autoFixSet : null;
  const fixView = current && mineFix && current.autoFix !== mineFix.enabled ? { ...current, autoFix: mineFix.enabled, autoFixAttempts: mineFix.attempts, autoFixCap: mineFix.cap, autoFixStopped: mineFix.stopped } : current;
  const fix = autoFixWords(fixView);
  const changeAutoFix = async (on: boolean) => {
    setAutoFixBusy(true);
    setAutoFixError(null);
    try {
      const r = await window.modbit.setAutoFix(sessionId, taskId, on);
      setAutoFixSet({ taskId, enabled: r.enabled, attempts: r.attempts, cap: r.cap, stopped: r.stopped });
    } catch (e) {
      setAutoFixError((e as Error).message.replace(/^Error invoking remote method '[^']*': (Error: )?/, ""));
    } finally {
      setAutoFixBusy(false);
    }
  };
  // Refresh asks the Core to read the request's state (REQ-PX-211), then the forge's check runs, then the request's comments, and
  // lists what it recorded. Each read stands alone: one the Core refuses says why without hiding the others.
  const refresh = async () => {
    if (refreshing) return;
    setRefreshing(true);
    setRefreshError(null);
    setStateError(null);
    setUndone(null);
    if (current) {
      try {
        const r = await window.modbit.refreshPullRequest(sessionId, taskId);
        setStateRead({ taskId, pr: r.pullRequest, archived: r.archived });
      } catch (e) {
        setStateError(forgeErrorWords((e as Error).message));
      }
    }
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
  // The approval is answered on the docked approval card (the one approval surface). Its answer reaches this window as an event
  // of that approval; the pane then asks the Core to go on (approved) or says the attempt ended (denied). It reads nothing for it.
  const given = useDelivered(taskId);
  const outcome = pr?.status === "APPROVAL_PENDING" ? approvalOutcome(given.events, pr.approvalId) : null;
  const revision = bundle?.workspaceRevision ?? "";
  const settled = flow.settled;
  useEffect(() => {
    if (outcome === null || !revision) return;
    void settled(outcome === "approved", revision);
    // One answer per approval: the flow leaves APPROVAL_PENDING, which clears `outcome`.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [outcome, revision]);
  const request = requestWords(pr);
  const undoArchive = async () => {
    try {
      await window.modbit.archiveTask(sessionId, taskId, false);
      setStateRead((s) => (s ? { ...s, archived: false } : s));
      setUndone("Brought back: the task is in the list again.");
    } catch (e) {
      setUndone((e as Error).message.replace(/^Error invoking remote method '[^']*': (Error: )?/, ""));
    }
  };
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
      {current && state && (
        <section
          className="pr-sec"
          aria-label="State"
          data-testid="pr-pane-state"
          data-number={current.number}
          data-state={current.state || "UNREAD"}
          data-checks={current.checks || "UNREAD"}
          data-read-offset={current.readOffset}
          data-trigger={current.trigger}
        >
          <p className="pr-title" data-testid="pr-pane-state-title" data-src={current === mine?.pr ? "refreshPullRequest" : "agentHeaders"}>
            <Icon name="git-compare" size={14} /> Pull request {state.number} · <span data-testid="pr-pane-state-word" data-tone={state.tone}>{state.state}</span>
          </p>
          <p className="meta pr-url" data-testid="pr-pane-state-url" data-src={current === mine?.pr ? "refreshPullRequest" : "agentHeaders"}>
            {current.url}
          </p>
          {state.read ? (
            <>
              {state.checks && (
                <p className="meta" data-testid="pr-pane-state-checks" data-tone={state.checksTone} data-src={current === mine?.pr ? "refreshPullRequest" : "agentHeaders"}>
                  Checks: {state.checks}
                  {state.failing && <span className="pr-dim"> ({state.failing})</span>}
                </p>
              )}
              <p className="meta" data-testid="pr-pane-read-at" data-src={current === mine?.pr ? "refreshPullRequest" : "agentHeaders"} title={new Date(current.readAtMs).toLocaleString()}>
                Read {relativeAge(current.readAtMs, nowMs) === "now" ? "just now" : `${relativeAge(current.readAtMs, nowMs)} ago`}
                {state.by ? `, ${state.by}` : ""}.
              </p>
            </>
          ) : (
            <p className="meta" data-testid="pr-pane-read-at" data-src={current === mine?.pr ? "refreshPullRequest" : "agentHeaders"}>
              The Core has not read this request since it opened it. Refresh asks it to.
            </p>
          )}
          {mine?.archived && (
            <p className="meta" role="status" data-testid="pr-pane-archived">
              The task was archived because its request merged and archive after merge is on.{" "}
              <Button size="sm" variant="outline" onClick={() => void undoArchive()} data-testid="pr-pane-undo-archive">
                Undo
              </Button>
            </p>
          )}
          {undone && (
            <p className="meta" role="status" data-testid="pr-pane-undone">
              {undone}
            </p>
          )}
          {current.state !== "MERGED" && current.state !== "CLOSED" && (
            <div className="pr-autofix" data-testid="pr-pane-auto-fix" data-on={fix.on ? "true" : "false"} data-tone={fix.tone} data-attempts={fixView?.autoFixAttempts ?? 0} data-cap={fixView?.autoFixCap ?? 0} data-src={mineFix && current.autoFix !== mineFix.enabled ? "setAutoFix" : "agentHeaders"}>
              <label className="pr-autofix-row">
                <input type="checkbox" className="settings-toggle" role="switch" aria-describedby="pr-autofix-detail" data-testid="pr-pane-auto-fix-switch" checked={fix.on} disabled={autoFixBusy} onChange={(e) => void changeAutoFix(e.target.checked)} />
                <span>Auto-fix</span>
              </label>
              <p className="meta" id="pr-autofix-detail" data-testid="pr-pane-auto-fix-detail">
                {fix.on ? fix.status : "Off. When on, a failing check or a comment addressed to @modbit starts a repair turn. Its pushes still ask."}
              </p>
              {autoFixError && (
                <p className="meta" role="alert" data-testid="pr-pane-auto-fix-error">
                  Auto-fix could not be changed: {autoFixError}
                </p>
              )}
            </div>
          )}
          {stateError && (
            <p className="meta" role="alert" data-testid="pr-pane-state-error">
              The request could not be read: {stateError}
              {state.read ? " What is shown is the last read." : ""}
            </p>
          )}
        </section>
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
          <div className="pr-approval" role="status" data-testid="pr-pane-approval" data-approval-id={pr.approvalId} data-intent-hash={pr.intentHash}>
            <p className="pr-title">{busy ? "Opening the request…" : "Waiting for your answer"}</p>
            <p className="meta">
              Pushing branch <code data-testid="pr-pane-branch">{pr.branch}</code> and opening the request is one protected action. It is asked on the approval card above the composer, in the task's conversation, and runs only once you allow it there. Intent{" "}
              <code data-testid="pr-pane-intent" title={pr.intentHash}>
                {pr.intentHash.slice(0, 16)}
              </code>
              , revision <code>{pr.candidateRevision}</code>.
            </p>
          </div>
        ) : (
          <>
            <p className="meta" data-testid="pr-pane-none">
              {pr?.status === "DENIED" ? "Not opened: the approval was not given, so the branch stays on this machine." : pr?.status === "FAILED" ? `Not opened: ${pr.detail}` : current ? "No pull request has been opened or updated from this window; the one above is the Core's record." : "No pull request has been opened from this window for this task."}
            </p>
            <div className="pr-actions">
              {/* A request the Core has on record is updated, not opened again (REQ-PX-211). */}
              <Button size="sm" variant="primary" disabled={busy || openable !== true} title={openable === true ? undefined : openable} onClick={() => bundle && void flow.open(current !== undefined && pr?.status !== "DENIED", bundle.workspaceRevision)} data-testid="pr-pane-open">
                {busy ? "Asking the Core…" : pr ? "Try again" : current ? "Update the request" : "Open pull request"}
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
