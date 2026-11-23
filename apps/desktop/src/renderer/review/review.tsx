import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { TaskCard } from "../model.ts";
import { splitRows } from "../diff.ts";
import { commandFor, isEditable } from "../keyboard.ts";
import { reviewState } from "../screens.ts";
import { StateLine } from "../shell/state-line.tsx";
import type { ReviewBundleView } from "../../preload/preload.ts";
import { useForgeReads, usePullRequest } from "./use-pull-request.ts";
import { attributionWords, checkName, checkOutcome, evidenceWords, fileStatusWords, plural, planWords, refusalWords, rule, runWords, selfReview, selfReviewSummary, stateWords } from "./review-model.ts";
import { commentWords } from "../apps/pull-request-model.ts";
import "../state/types.ts";

/** Review screen (docs/20 "Trusted Code Surface", REQ-EV-0036): every fact on it
 *  is a revision-bound payload from the Core; the renderer holds no buffers. */
export function Review({ taskId, sessionId, card, onClose }: { taskId: string; sessionId: string; card: TaskCard | null; onClose: () => void }) {
  const [bundle, setBundle] = useState<ReviewBundleView | null>(null);
  const [err, setErr] = useState<string | null>(null);
  // PX-007: the pull request from this candidate, and PX-127: the forge's CI runs and the request's comments. One flow, which the
  // Pull request pane drives too (review/use-pull-request.ts). The ack's states are the screen's: APPROVAL_PENDING (decide the
  // intent shown), OPENED/UPDATED (the URL), DENIED (the branch stays local; a receipt exists).
  const { pr, busy: prBusy, error: prError, open: openPr, decide: decidePr } = usePullRequest(sessionId, taskId, "the review");
  const openPullRequest = async (update: boolean) => {
    if (bundle) await openPr(update, bundle.workspaceRevision);
  };
  const decidePullRequest = async (approve: boolean) => {
    if (bundle) await decidePr(approve, bundle.workspaceRevision);
  };
  const reloadRef = useRef<() => Promise<void>>(async () => {});
  const { busy: forgeBusy, note: forgeNote, error: forgeError, ingest: ingestForge } = useForgeReads(sessionId, taskId, () => reloadRef.current());
  const [rejected, setRejected] = useState<Set<string>>(new Set());
  // REQ-EV-0141: hunks the reviewer marks as context. This changes what
  // retrieval prefers and nothing else — it cannot edit or accept anything.
  const [context, setContext] = useState<Set<string>>(new Set());
  const [selectionError, setSelectionError] = useState<string | null>(null);
  const [note, setNote] = useState("");
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<string | null>(null);
  const [reverted, setReverted] = useState<number | null>(null);
  const load = useCallback(async () => {
    setErr(null);
    try {
      setBundle(await window.modbit.reviewBundle(taskId));
    } catch (e) {
      setErr((e as Error).message);
    }
  }, [taskId]);
  reloadRef.current = load;
  useEffect(() => {
    void load();
  }, [load]);
  // PX-024: the Review takes the focus when it opens (the conversation is hidden
  // behind it); Escape or "Back to the task" closes it.
  const section = useRef<HTMLElement>(null);
  useEffect(() => {
    section.current?.focus();
  }, []);
  const toggle = (key: string) =>
    setRejected((r) => {
      const n = new Set(r);
      if (n.has(key)) n.delete(key);
      else n.add(key);
      return n;
    });
  const toggleContext = async (key: string) => {
    const next = new Set(context);
    if (next.has(key)) next.delete(key);
    else next.add(key);
    setContext(next);
    const hunks = Array.from(next);
    try {
      setSelectionError(null);
      await window.modbit.setTaskSelection(sessionId, taskId, {
        paths: Array.from(new Set(hunks.map((k) => k.split("#")[0] ?? k))),
        reviewHunks: hunks,
        source: "review",
      });
    } catch (e) {
      setSelectionError((e as Error).message);
    }
  };
  const decide = async (decision: "ACCEPT" | "RETURN") => {
    if (!bundle || busy) return;
    setBusy(true);
    setErr(null);
    try {
      const rej = Array.from(rejected).map((k) => {
        const i = k.lastIndexOf("#");
        return { path: k.slice(0, i), index: Number(k.slice(i + 1)) };
      });
      const d = await window.modbit.decideReview(sessionId, taskId, decision, rej, note, bundle.workspaceRevision);
      setResult(decision === "ACCEPT" ? `Accepted as commit ${d.commit.slice(0, 12)}; ${plural(d.reverted.length, "change")} undone` : "Returned to work");
      setReverted(decision === "ACCEPT" ? d.reverted.length : null);
    } catch (e) {
      setErr((e as Error).message);
    } finally {
      setBusy(false);
    }
  };
  // PX-005 (docs/20, docs/29): a constrained inline patch. The two fields
  // are the command's arguments and nothing more — no buffer of the file
  // lives here; the Core applies the edit at the revisions this review
  // showed, or refuses it, and the review is re-read from the Core.
  const [patching, setPatching] = useState<string | null>(null);
  // PX-024 review navigation: next/previous change (focus moves between
  // hunks), expand or collapse the focused hunk, unified or split, jump to
  // the failing check. Every hunk and check is focusable; nothing is
  // reachable by mouse only.
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());
  const [split, setSplit] = useState(false);
  const hunkElements = () => [...document.querySelectorAll<HTMLElement>('[data-testid="review-hunk"]')];
  const focusedHunk = () => document.activeElement?.closest<HTMLElement>('[data-testid="review-hunk"]') ?? null;
  const toggleCollapsed = (key: string) =>
    setCollapsed((c) => {
      const next = new Set(c);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const cmd = commandFor(e, { editable: isEditable(document.activeElement), screen: "review", confirming: false, isMac: navigator.platform.toUpperCase().includes("MAC") });
      if (!cmd) return;
      switch (cmd) {
        case "hunkNext":
        case "hunkPrevious": {
          e.preventDefault();
          const hunks = hunkElements();
          const i = hunks.indexOf(focusedHunk()!);
          hunks[i < 0 ? 0 : Math.min(hunks.length - 1, Math.max(0, i + (cmd === "hunkNext" ? 1 : -1)))]?.focus();
          return;
        }
        case "hunkToggle": {
          const h = focusedHunk();
          // Enter on a control inside the hunk (a checkbox, a button) is that control's.
          if (!h || (e.key === "Enter" && document.activeElement !== h)) return;
          e.preventDefault();
          toggleCollapsed(h.getAttribute("data-hunk") ?? "");
          return;
        }
        case "toggleSplit":
          e.preventDefault();
          setSplit((v) => !v);
          return;
        case "jumpFailing": {
          e.preventDefault();
          const failing = document.querySelector<HTMLElement>('[data-testid="review-check"][data-status="FAIL"]');
          (failing ?? document.querySelector<HTMLElement>('[data-testid="review-verification"]'))?.focus();
          return;
        }
        default:
          return;
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);
  const [patchOld, setPatchOld] = useState("");
  const [patchNew, setPatchNew] = useState("");
  // What the Core answered to the last edit: applied (with the revisions) or refused (with its code), in words.
  const [patchNote, setPatchNote] = useState<{ ok: boolean; text: string; code: string; revision: string; previous: string } | null>(null);
  const openPatch = (path: string) => {
    setPatching(patching === path ? null : path);
    setPatchOld("");
    setPatchNew("");
    setPatchNote(null);
  };
  const applyPatch = async (path: string, fileRevision: string) => {
    if (!bundle || busy) return;
    setBusy(true);
    setPatchNote(null);
    try {
      const r = await window.modbit.applyUserPatch(sessionId, taskId, { path, old: patchOld, new: patchNew, expectedWorkspaceRevision: bundle.workspaceRevision, expectedFileRevision: fileRevision });
      setPatchNote({ ok: true, text: "Your edit was applied, and the review was read again.", code: "", revision: String(r.workspaceRevision), previous: String(r.previousRevision) });
      setPatching(null);
      setPatchOld("");
      setPatchNew("");
      await load();
    } catch (e) {
      const w = refusalWords((e as Error).message);
      setPatchNote({ ok: false, text: `The edit was refused: ${w.text}.`, code: w.code, revision: "", previous: "" });
    } finally {
      setBusy(false);
    }
  };
  const hunkCount = bundle ? bundle.files.reduce((n, f) => n + f.hunks.length, 0) : 0;
  const state = reviewState({
    loading: bundle === null && err === null,
    // A refused patch (STALE_REVISION: the review shown is behind the
    // workspace) is this screen's error until the review is reloaded.
    error: err ?? prError ?? (patchNote && !patchNote.ok ? `${patchNote.code ? `${patchNote.code}: ` : ""}${patchNote.text}` : null),
    bundle,
    gate: card?.gate ?? null,
    pullRequest: pr ? { status: pr.status, detail: pr.detail, evidence: pr.status === "DENIED" && pr.receipts === 0 ? `approval ${pr.approvalId.slice(0, 8)}` : `${plural(pr.receipts, "receipt")}${pr.url ? `; ${pr.url}` : ""}` } : null,
  });
  const selectionNote = selectionError
    ? `Not kept as context: ${refusalWords(selectionError).text}`
    : context.size > 0
      ? `${plural(context.size, "change")} used as context`
      : null;
  return (
    <section className="review" data-testid="review" aria-label="Review" tabIndex={-1} ref={section}>
      <div className="review-head">
        <h2 style={{ margin: 0 }}>Review</h2>
        <button type="button" className="small" data-testid="review-split" aria-pressed={split} aria-keyshortcuts="u" onClick={() => setSplit((v) => !v)}>
          {split ? "One column" : "Side by side"}
        </button>
        <span
          className="meta"
          data-testid="review-meta"
          data-task-state={bundle?.taskState}
          data-workspace-revision={bundle?.workspaceRevision}
          data-base={bundle?.baseCommit}
          data-receipts={bundle?.receipts}
          title={bundle ? `Task ${taskId.slice(0, 8)} · workspace revision ${bundle.workspaceRevision} · base ${bundle.baseCommit.slice(0, 12)} · ${plural(bundle.receipts, "receipt")}` : undefined}
        >
          {selectionNote && (
            <span className="meta" data-testid="review-selection">
              {selectionNote}
            </span>
          )}
          {bundle ? `${stateWords(bundle.taskState)} · ${plural(bundle.files.length, "file")}, ${plural(hunkCount, "change")}${bundle.baseCommit ? ` · based on ${bundle.baseCommit.slice(0, 7)}` : ""}` : "Reading the review from the Core…"}
        </span>
        <button type="button" data-testid="review-close" onClick={onClose} aria-keyshortcuts="Escape">
          Back to the task
        </button>
      </div>
      <StateLine state={state} testid="review-state" />
      {err && (
        <div className="banner" data-kind="error" role="alert" data-testid="review-error">
          <strong>The review could not be read</strong>: {refusalWords(err).text} <button type="button" onClick={() => void load()}>Reload</button>
        </div>
      )}
      {result && (
        <div className="banner" data-kind="recovered" role="status" data-testid="review-result" data-reverted={reverted ?? undefined}>
          {result}
        </div>
      )}
      {patchNote && (
        <div className="banner" data-kind={patchNote.ok ? "recovered" : "error"} role="status" data-testid="patch-result" data-outcome={patchNote.ok ? "applied" : "refused"} data-code={patchNote.code || undefined} data-workspace-revision={patchNote.revision || undefined} data-previous-revision={patchNote.previous || undefined}>
          {patchNote.text}
        </div>
      )}
      {bundle && (
        <div className="review-body">
          <div>
            {bundle.files.length === 0 && <p className="empty" data-testid="review-empty">The candidate has no changes.</p>}
            {bundle.files.map((f) => (
              <article className="file" key={f.path} data-testid="review-file" data-path={f.path}>
                <h3>
                  {f.path}{" "}
                  <span className="meta" data-status={f.status} data-file-revision={f.fileRevision} title={`File revision ${f.fileRevision.slice(0, 12)}`}>
                    {fileStatusWords(f.status, f.binary)}
                  </span>
                  {!f.binary && f.status !== "D" && (
                    <button type="button" className="small" data-testid="file-patch" onClick={() => openPatch(f.path)} disabled={busy || result !== null || bundle.taskState !== "ReadyForReview"}>
                      {patching === f.path ? "Cancel edit" : "Edit"}
                    </button>
                  )}
                </h3>
                {patching === f.path && (
                  <div className="patch" data-testid="patch-panel" data-path={f.path}>
                    <label>
                      Replace
                      <textarea data-testid="patch-old" value={patchOld} onChange={(e) => setPatchOld(e.target.value)} placeholder="the exact text to replace (one place)" />
                    </label>
                    <label>
                      With
                      <textarea data-testid="patch-new" value={patchNew} onChange={(e) => setPatchNew(e.target.value)} placeholder="its replacement" />
                    </label>
                    <div className="actions">
                      <button type="button" data-testid="patch-apply" onClick={() => void applyPatch(f.path, f.fileRevision)} disabled={busy || patchOld.length === 0}>
                        Apply
                      </button>
                      <span className="meta">The Core applies it to the version shown here, or refuses it if the file has moved on.</span>
                    </div>
                  </div>
                )}
                {f.binary && <p className="meta">A binary file: its content is not shown.</p>}
                {f.hunks.map((h) => {
                  const key = `${f.path}#${h.index}`;
                  const isRejected = rejected.has(key);
                  return (
                    <div className="hunk" key={key} data-testid="review-hunk" data-hunk={key} data-rejected={isRejected} data-collapsed={collapsed.has(key)} tabIndex={0} role="group" aria-label={`${f.path} ${h.header}${isRejected ? ", rejected" : ""}${collapsed.has(key) ? ", collapsed" : ""}`}>
                      <div className="hunk-head">
                        <code>{h.header}</code>
                        {isRejected && <span className="meta">Rejected: undone when you accept</span>}
                        <button type="button" className="small" data-testid="hunk-toggle" aria-expanded={!collapsed.has(key)} onClick={() => toggleCollapsed(key)}>
                          {collapsed.has(key) ? "Expand" : "Collapse"}
                        </button>
                        <label>
                          <input type="checkbox" data-testid="hunk-reject" checked={isRejected} onChange={() => toggle(key)} disabled={result !== null} /> Reject this change
                        </label>
                        <label title="Give this change to the agent as context when it looks things up. It cannot change any file.">
                          <input type="checkbox" data-testid="hunk-context" checked={context.has(key)} onChange={() => void toggleContext(key)} disabled={result !== null} /> Use as context
                        </label>
                      </div>
                      {collapsed.has(key) ? (
                        <p className="meta" data-testid="hunk-collapsed">
                          Collapsed · {h.lines.filter((l) => l.startsWith("+")).length} added, {h.lines.filter((l) => l.startsWith("-")).length} removed lines
                        </p>
                      ) : split ? (
                        <table className="split" data-testid="hunk-split" aria-label="split view">
                          <tbody>
                            {splitRows(h.lines).map((r, i) => (
                              <tr key={i}>
                                <td className={r.left === null ? "gap" : r.left.startsWith("-") ? "del" : "ctx"}>{r.left ?? ""}</td>
                                <td className={r.right === null ? "gap" : r.right.startsWith("+") ? "add" : "ctx"}>{r.right ?? ""}</td>
                              </tr>
                            ))}
                          </tbody>
                        </table>
                      ) : (
                        <pre data-testid="hunk-unified">
                          {h.lines.map((l, i) => (
                            <span key={i} className={l.startsWith("+") ? "add" : l.startsWith("-") ? "del" : "ctx"}>
                              {l}
                              {"\n"}
                            </span>
                          ))}
                        </pre>
                      )}
                    </div>
                  );
                })}
              </article>
            ))}
          </div>
          <aside role="region" aria-label="Evidence and decision">
            <h3>Checks</h3>
            {bundle.verificationRuns.length === 0 && <p className="empty">The Core has not run any checks yet.</p>}
            <ul data-testid="review-verification" tabIndex={-1} aria-label="checks the Core ran">
              {bundle.verificationRuns.map((v) => (
                <li key={v.id} data-stage={v.stage} data-status={v.status} title={`Run on revision ${v.candidateRevision}`}>
                  {runWords(v)}
                  {v.checks.some((c) => c.status !== "PASS") && (
                    <ul>
                      {v.checks.filter((c) => c.status !== "PASS").map((c) => (
                        <li key={c.id} data-testid="review-check" data-status={c.status} data-check-id={c.id} tabIndex={-1}>
                          {c.status === "FAIL" ? "✖ " : ""}
                          {checkOutcome(c)}
                        </li>
                      ))}
                    </ul>
                  )}
                </li>
              ))}
            </ul>
            <h3>Checks on the pull request</h3>
            <div className="meta">What the forge's own checks said about the pushed commit. They inform this review; they never accept it.</div>
            {bundle.ciEvidence.length === 0 && <p className="empty" data-testid="review-ci-empty">None read yet.</p>}
            <ul data-testid="review-ci" tabIndex={-1} aria-label="checks on the pull request">
              {bundle.ciEvidence.map((c, i) => (
                <li key={`${c.runId}-${i}`} data-testid="review-ci-run" data-conclusion={c.conclusion || c.status} data-provenance={c.provenance} title={c.logRef ? `Log ${c.logRef.slice(0, 12)}${c.logTruncated ? " (cut)" : ""}` : undefined}>
                  <strong>{c.name}</strong> {c.conclusion === "failure" ? "✖ " : ""}
                  {(c.conclusion || c.status).replace(/_/g, " ")} · run {c.runId} · commit {c.commit.slice(0, 7)}
                  {c.url && <> · <code>{c.url}</code></>}
                </li>
              ))}
              {bundle.ciRejected.map((r, i) => (
                <li key={`rej-${i}`} data-testid="review-ci-refused" data-reason={r.reason}>
                  Not counted: <strong>{r.name}</strong> ran on another commit ({r.headSha.slice(0, 7)}), {r.reason.replace(/_/g, " ").toLowerCase()}
                </li>
              ))}
            </ul>
            <h3>Comments on the pull request</h3>
            <div className="meta">They come from outside Modbit: the agent reads an allowed person's request as a request, never as an instruction it must follow.</div>
            {bundle.reviewComments.length === 0 && <p className="empty" data-testid="review-comments-empty">None read yet.</p>}
            <ul data-testid="review-comments" tabIndex={-1} aria-label="comments on the pull request">
              {bundle.reviewComments.map((t) => {
                const w = commentWords(t);
                return (
                  <li key={`${t.commentId}-${t.inputId}-${t.disposition}`} data-testid="review-comment" data-disposition={t.disposition} data-trust={t.trust} data-reason={t.reason || undefined} data-kind={t.kind} data-answered={t.answered ? "true" : "false"}>
                    <strong>{w.author}</strong>
                    {w.where && <> on <code>{w.where}</code></>} · {w.outcome}
                    {t.body && <pre className="small" data-testid="review-comment-body">{t.body}</pre>}
                  </li>
                );
              })}
            </ul>
            <div className="actions">
              <button type="button" className="small" data-testid="review-ci-ingest" onClick={() => void ingestForge("ci")} disabled={forgeBusy}>
                Read checks
              </button>{" "}
              <button type="button" className="small" data-testid="review-comments-ingest" onClick={() => void ingestForge("comments")} disabled={forgeBusy}>
                Read comments
              </button>
            </div>
            {forgeNote && <div className="meta" data-testid="review-forge-note">{forgeNote}</div>}
            {forgeError && (
              <div role="alert" className="error" data-testid="review-forge-error" data-code={refusalWords(forgeError).code || undefined}>
                {refusalWords(forgeError).text}
              </div>
            )}
            {bundle.attributions.length > 0 && (
              <>
                <h3>What broke a check</h3>
                <ul data-testid="review-attribution">
                  {bundle.attributions.map((a) => (
                    <li key={a} data-raw={a}>
                      {attributionWords(a)}
                    </li>
                  ))}
                </ul>
              </>
            )}
            {bundle.quarantined.length > 0 && (
              <>
                <h3>Set aside as flaky</h3>
                <ul>
                  {bundle.quarantined.map((q) => (
                    <li key={q} data-check-id={q}>
                      {checkName(q)}
                    </li>
                  ))}
                </ul>
              </>
            )}
            {bundle.invariantFindings.length > 0 && (
              <>
                <h3>Change rules broken</h3>
                <ul>
                  {bundle.invariantFindings.map((q) => {
                    const r = rule(q);
                    return (
                      <li key={q} data-raw={q}>
                        {r.path && <code>{r.path}</code>} {r.text}
                      </li>
                    );
                  })}
                </ul>
              </>
            )}
            <h3>Plan</h3>
            <div className="small" data-testid="review-plan" tabIndex={0} aria-label="plan">
              {(() => {
                const plan = planWords(bundle.planJson);
                if (!bundle.planJson) return <p className="empty">No plan was recorded.</p>;
                if (!plan) return <p>{bundle.planJson}</p>;
                return (
                  <>
                    {plan.outcome && <p>{plan.outcome}</p>}
                    {plan.files.length > 0 && (
                      <p>
                        Files it expected to change: {plan.files.map((f, i) => (
                          <span key={f}>
                            {i > 0 ? ", " : ""}
                            <code>{f}</code>
                          </span>
                        ))}
                      </p>
                    )}
                  </>
                );
              })()}
            </div>
            <h3>Self-review</h3>
            <div className="small" data-testid="review-self-review" tabIndex={0} aria-label="self-review">
              {(() => {
                if (!bundle.selfReviewJson) return <p className="empty">The agent recorded no self-review.</p>;
                const summary = selfReviewSummary(bundle.selfReviewJson);
                const items = selfReview(bundle.selfReviewJson);
                return (
                  <>
                    {summary && <p>{summary}</p>}
                    {items.length === 0 ? (
                      <p className="meta">It found nothing to fix.</p>
                    ) : (
                      <ul>
                        {items.map((f, i) => (
                          <li key={i} data-open={f.open ? "true" : "false"}>
                            {f.text} <span className="meta">({f.open ? "not resolved" : "resolved"})</span>
                          </li>
                        ))}
                      </ul>
                    )}
                  </>
                );
              })()}
            </div>
            <details className="small review-evidence" data-testid="review-evidence-group">
              <summary>Evidence ({bundle.evidenceLinks.length})</summary>
              <ul data-testid="review-evidence" aria-label="evidence">
                {bundle.evidenceLinks.map((e) => {
                  const w = evidenceWords(e);
                  return (
                    <li key={e} data-link={e}>
                      {w.kind} <code title={e}>{w.short}</code>
                    </li>
                  );
                })}
              </ul>
            </details>
            <h3>Decision</h3>
            <textarea data-testid="review-note" value={note} onChange={(e) => setNote(e.target.value)} placeholder="Commit message / feedback" disabled={result !== null} />
            <div className="actions">
              <button type="button" data-testid="review-accept" onClick={() => void decide("ACCEPT")} disabled={busy || result !== null || bundle.taskState !== "ReadyForReview"}>
                Accept{rejected.size ? ` (${rejected.size} rejected)` : ""}
              </button>
              <button type="button" data-testid="review-return" onClick={() => void decide("RETURN")} disabled={busy || result !== null || bundle.taskState !== "ReadyForReview"}>
                Return to work
              </button>
            </div>
            <h3>Pull request</h3>
            <div className="meta">Pushing the branch and opening the request is one action that asks you first; the Core uses the token it holds.</div>
            <div className="actions" data-testid="pull-request" data-status={pr?.status ?? "NONE"}>
              {(!pr || pr.status === "DENIED" || pr.status === "FAILED") && (
                <button type="button" data-testid="pr-open" onClick={() => void openPullRequest(false)} disabled={prBusy || (bundle.taskState !== "ReadyForReview" && !result?.startsWith("Accepted") && bundle.taskState !== "Completed")}>
                  {prBusy ? "Asking the Core…" : pr ? "Try again" : "Open a pull request"}
                </button>
              )}
              {pr?.status === "APPROVAL_PENDING" && (
                <span data-testid="pr-approval">
                  <span className="meta">
                    Push <code>{pr.branch}</code> and open the pull request?{" "}
                    <span data-testid="pr-intent" data-intent={pr.intentHash.slice(0, 16)} data-revision={pr.candidateRevision} title={`Intent ${pr.intentHash.slice(0, 16)} · revision ${pr.candidateRevision}`}>
                      This asks once, for exactly this push.
                    </span>
                  </span>{" "}
                  <button type="button" data-testid="pr-approve" onClick={() => void decidePullRequest(true)} disabled={prBusy}>
                    Approve and push
                  </button>{" "}
                  <button type="button" data-testid="pr-deny" onClick={() => void decidePullRequest(false)} disabled={prBusy}>
                    Deny
                  </button>
                </span>
              )}
              {(pr?.status === "OPENED" || pr?.status === "UPDATED") && (
                <span className="meta" data-testid="pr-result">
                  <span title={`Head ${pr.headSha.slice(0, 12)}`} data-head={pr.headSha} data-receipts={pr.receipts}>
                    {pr.status === "OPENED" ? "Opened" : "Updated"} #{pr.number} at {pr.url} · {plural(pr.receipts, "receipt")}
                    {pr.replayed ? " · answered from the record" : ""}
                  </span>{" "}
                  <button type="button" className="small" data-testid="pr-update" onClick={() => void openPullRequest(true)} disabled={prBusy}>
                    Update
                  </button>
                </span>
              )}
              {pr?.status === "DENIED" && (
                <span className="meta" role="status" data-testid="pr-denied">
                  Denied: the branch stays local and nothing reached the forge.
                </span>
              )}
              {prError && (
                <span className="meta" role="alert" data-testid="pr-error">
                  {refusalWords(prError).text}
                </span>
              )}
            </div>
          </aside>
        </div>
      )}
    </section>
  );
}
