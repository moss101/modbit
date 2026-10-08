import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { TaskCard } from "../model.ts";
import { splitRows } from "../diff.ts";
import { commandFor, isEditable } from "../keyboard.ts";
import { reviewState } from "../screens.ts";
import { StateLine } from "../shell/state-line.tsx";
import type { PullRequestAckView, ReviewBundleView } from "../../preload/preload.ts";
import "../state/types.ts";

/** Review screen (docs/20 "Trusted Code Surface", REQ-EV-0036): every fact on it
 *  is a revision-bound payload from the Core; the renderer holds no buffers. */
export function Review({ taskId, sessionId, card, onClose }: { taskId: string; sessionId: string; card: TaskCard | null; onClose: () => void }) {
  const [bundle, setBundle] = useState<ReviewBundleView | null>(null);
  const [err, setErr] = useState<string | null>(null);
  // PX-007: the pull request from this candidate. The ack's states are the
  // screen's: APPROVAL_PENDING (decide the intent shown), OPENED/UPDATED
  // (the URL), DENIED (the branch stays local; a receipt exists).
  const [pr, setPr] = useState<PullRequestAckView | null>(null);
  const [prBusy, setPrBusy] = useState(false);
  const [prError, setPrError] = useState<string | null>(null);
  // The revision the pull request names: the one this review showed, which
  // is the one an acceptance records as the candidate.
  const candidateRevision = useRef<string | null>(null);
  const openPullRequest = async (update: boolean) => {
    if (!bundle || prBusy) return;
    setPrBusy(true);
    setPrError(null);
    try {
      candidateRevision.current ??= bundle.workspaceRevision;
      setPr(await window.modbit.openPullRequest(sessionId, taskId, candidateRevision.current, update));
    } catch (e) {
      setPrError((e as Error).message);
    } finally {
      setPrBusy(false);
    }
  };
  const decidePullRequest = async (approve: boolean) => {
    if (!pr || pr.status !== "APPROVAL_PENDING" || prBusy) return;
    setPrBusy(true);
    setPrError(null);
    try {
      const r = await window.modbit.resolveApproval(sessionId, pr.approvalId, approve, approve ? "approved from the review" : "denied from the review", pr.intentHash);
      if (approve) {
        // Approved: the same call now pushes and opens, under that approval.
        setPr(await window.modbit.openPullRequest(sessionId, taskId, candidateRevision.current ?? bundle!.workspaceRevision, false));
      } else {
        // Denied: terminal for this attempt; the branch stays local. A later
        // "Try again" is a new attempt with a new approval.
        setPr({ ...pr, status: "DENIED", detail: `approval ${r.approvalId.slice(0, 8)} denied at offset ${r.offset}: the branch stays local, nothing reached the forge`, receipts: 0 });
      }
    } catch (e) {
      setPrError((e as Error).message);
    } finally {
      setPrBusy(false);
    }
  };
  // PX-127: the forge's CI runs and the pull request's comments. The Core
  // reads the forge; this screen asks it to and shows what it recorded.
  const [forgeBusy, setForgeBusy] = useState(false);
  const [forgeNote, setForgeNote] = useState<string | null>(null);
  const [forgeError, setForgeError] = useState<string | null>(null);
  const ingestForge = async (what: "ci" | "comments") => {
    if (forgeBusy) return;
    setForgeBusy(true);
    setForgeError(null);
    setForgeNote(null);
    try {
      if (what === "ci") {
        const r = await window.modbit.ingestCiResults(sessionId, taskId);
        setForgeNote(`read ${r.checks} check run(s) for ${r.commit.slice(0, 12)}${r.refused > 0 ? `, refused ${r.refused} for another commit` : ""} (offset ${r.offset})`);
      } else {
        const r = await window.modbit.ingestReviewComments(sessionId, taskId);
        setForgeNote(`${r.steered} comment(s) steer the task, ${r.ignored} ignored, ${r.alreadyTaken} already taken (offset ${r.offset})`);
      }
      await load();
    } catch (e) {
      setForgeError((e as Error).message);
    } finally {
      setForgeBusy(false);
    }
  };
  const [rejected, setRejected] = useState<Set<string>>(new Set());
  // REQ-EV-0141: hunks the reviewer marks as context. This changes what
  // retrieval prefers and nothing else — it cannot edit or accept anything.
  const [context, setContext] = useState<Set<string>>(new Set());
  const [selectionError, setSelectionError] = useState<string | null>(null);
  const [note, setNote] = useState("");
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<string | null>(null);
  const load = useCallback(async () => {
    setErr(null);
    try {
      setBundle(await window.modbit.reviewBundle(taskId));
    } catch (e) {
      setErr((e as Error).message);
    }
  }, [taskId]);
  useEffect(() => {
    void load();
  }, [load]);
  // PX-024: the Review takes the focus when it opens (the fleet is hidden
  // behind it); Escape or "Back to fleet" returns it to the card.
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
      setResult(decision === "ACCEPT" ? `Accepted: commit ${d.commit.slice(0, 12)}, ${d.reverted.length} hunk(s) reverted` : "Returned to work");
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
  const [patchNote, setPatchNote] = useState<string | null>(null);
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
      setPatchNote(`applied at workspace revision ${r.workspaceRevision} (was ${r.previousRevision}) · file revision ${r.fileRevision.slice(0, 12)}`);
      setPatching(null);
      setPatchOld("");
      setPatchNew("");
      await load();
    } catch (e) {
      setPatchNote(`refused: ${(e as Error).message}`);
    } finally {
      setBusy(false);
    }
  };
  const hunkCount = bundle ? bundle.files.reduce((n, f) => n + f.hunks.length, 0) : 0;
  const state = reviewState({
    loading: bundle === null && err === null,
    // A refused patch (STALE_REVISION: the review shown is behind the
    // workspace) is this screen's error until the review is reloaded.
    error: err ?? prError ?? (patchNote?.startsWith("refused") ? patchNote : null),
    bundle,
    gate: card?.gate ?? null,
    pullRequest: pr ? { status: pr.status, detail: pr.detail, evidence: pr.status === "DENIED" && pr.receipts === 0 ? `approval ${pr.approvalId.slice(0, 8)}` : `${pr.receipts} receipt(s)${pr.url ? `; ${pr.url}` : ""}` } : null,
  });
  const selectionNote = selectionError
    ? `selection not recorded: ${selectionError}`
    : context.size > 0
      ? `${context.size} hunk(s) given to retrieval as context`
      : null;
  return (
    <section className="review" data-testid="review" aria-label="Review" tabIndex={-1} ref={section}>
      <div className="review-head">
        <h2 style={{ margin: 0 }}>Review</h2>
        <button type="button" className="small" data-testid="review-split" aria-pressed={split} onClick={() => setSplit((v) => !v)}>
          {split ? "Unified (u)" : "Split (u)"}
        </button>
        <span className="meta" data-testid="review-meta">
          {selectionNote && (
            <span className="meta" data-testid="review-selection">
              {selectionNote}
            </span>
          )}
          {bundle ? `task ${taskId.slice(0, 8)} · ${bundle.taskState} · workspace revision ${bundle.workspaceRevision} · base ${bundle.baseCommit.slice(0, 8)} · ${bundle.files.length} file(s), ${hunkCount} hunk(s), ${bundle.receipts} receipt(s)` : "loading from the Core…"}
        </span>
        <button type="button" data-testid="review-close" onClick={onClose} aria-keyshortcuts="Escape">
          Back to fleet
        </button>
      </div>
      <StateLine state={state} testid="review-state" />
      {err && (
        <div className="banner" data-kind="error" role="alert" data-testid="review-error">
          <strong>Error</strong> — {err} <button type="button" onClick={() => void load()}>Reload</button>
        </div>
      )}
      {result && (
        <div className="banner" data-kind="recovered" role="status" data-testid="review-result">
          {result}
        </div>
      )}
      {patchNote && (
        <div className="banner" data-kind={patchNote.startsWith("refused") ? "error" : "recovered"} role="status" data-testid="patch-result">
          {patchNote}
        </div>
      )}
      {bundle && (
        <div className="review-body">
          <div>
            {bundle.files.length === 0 && <p className="empty" data-testid="review-empty">The candidate has no changes.</p>}
            {bundle.files.map((f) => (
              <article className="file" key={f.path} data-testid="review-file" data-path={f.path}>
                <h3>
                  {f.path} <span className="meta">{f.status === "A" ? "added" : f.status === "D" ? "deleted" : f.status === "R" ? "renamed" : "modified"} · file revision {f.fileRevision.slice(0, 12)}</span>
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
                        Apply at revision {bundle.workspaceRevision}
                      </button>
                      <span className="meta">through the Core's change transaction; nothing is kept here</span>
                    </div>
                  </div>
                )}
                {f.binary && <p className="meta">binary file</p>}
                {f.hunks.map((h) => {
                  const key = `${f.path}#${h.index}`;
                  const isRejected = rejected.has(key);
                  return (
                    <div className="hunk" key={key} data-testid="review-hunk" data-hunk={key} data-rejected={isRejected} data-collapsed={collapsed.has(key)} tabIndex={0} role="group" aria-label={`${f.path} ${h.header}${isRejected ? ", rejected" : ""}${collapsed.has(key) ? ", collapsed" : ""}`}>
                      <div className="hunk-head">
                        <code>{h.header}</code>
                        {isRejected && <span className="meta">rejected</span>}
                        <button type="button" className="small" data-testid="hunk-toggle" aria-expanded={!collapsed.has(key)} onClick={() => toggleCollapsed(key)}>
                          {collapsed.has(key) ? "Expand" : "Collapse"}
                        </button>
                        <label>
                          <input type="checkbox" data-testid="hunk-reject" checked={isRejected} onChange={() => toggle(key)} disabled={result !== null} /> reject
                        </label>
                        <label title="Give this hunk to retrieval as context. It cannot change any file.">
                          <input type="checkbox" data-testid="hunk-context" checked={context.has(key)} onChange={() => void toggleContext(key)} disabled={result !== null} /> context
                        </label>
                      </div>
                      {collapsed.has(key) ? (
                        <p className="meta" data-testid="hunk-collapsed">
                          {h.lines.filter((l) => l.startsWith("+")).length} added, {h.lines.filter((l) => l.startsWith("-")).length} removed line(s) — collapsed
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
            <h3>Verification</h3>
            {bundle.verificationRuns.length === 0 && <p className="empty">No verification runs recorded.</p>}
            <ul data-testid="review-verification" tabIndex={-1} aria-label="verification runs">
              {bundle.verificationRuns.map((v) => (
                <li key={v.id}>
                  <strong>{v.stage}</strong> {v.status} · {v.checks.filter((c) => c.status === "PASS").length}/{v.checks.length} pass · {v.candidateRevision}
                  {v.checks.some((c) => c.status !== "PASS") && (
                    <ul>
                      {v.checks.filter((c) => c.status !== "PASS").map((c) => (
                        <li key={c.id} data-testid="review-check" data-status={c.status} tabIndex={-1}>
                          {c.status === "FAIL" ? "✖ failed: " : `${c.status.toLowerCase()}: `}
                          {c.id}
                        </li>
                      ))}
                    </ul>
                  )}
                </li>
              ))}
            </ul>
            <h3>Forge CI (external evidence)</h3>
            <div className="meta">What the forge's checks said about the pushed commit. It informs this review; it is never a verification result and never an acceptance.</div>
            {bundle.ciEvidence.length === 0 && <p className="empty" data-testid="review-ci-empty">No CI results read yet.</p>}
            <ul data-testid="review-ci" tabIndex={-1} aria-label="forge CI results">
              {bundle.ciEvidence.map((c, i) => (
                <li key={`${c.runId}-${i}`} data-testid="review-ci-run" data-conclusion={c.conclusion || c.status} data-provenance={c.provenance}>
                  <strong>{c.name}</strong> {c.conclusion === "failure" ? "✖ " : ""}
                  {c.conclusion || c.status} · run {c.runId} · {c.commit.slice(0, 12)} · provenance {c.provenance}
                  {c.url && <> · <code>{c.url}</code></>}
                  {c.logRef && <> · log <code>{c.logRef.slice(0, 12)}</code>{c.logTruncated ? " (cut)" : ""}</>}
                </li>
              ))}
              {bundle.ciRejected.map((r, i) => (
                <li key={`rej-${i}`} data-testid="review-ci-refused">
                  refused <strong>{r.name}</strong>: {r.reason} (run on {r.headSha.slice(0, 12)})
                </li>
              ))}
            </ul>
            <h3>Pull request comments (untrusted)</h3>
            {bundle.reviewComments.length === 0 && <p className="empty" data-testid="review-comments-empty">No pull request comments read yet.</p>}
            <ul data-testid="review-comments" tabIndex={-1} aria-label="pull request comments">
              {bundle.reviewComments.map((t) => (
                <li key={`${t.commentId}-${t.inputId}-${t.disposition}`} data-testid="review-comment" data-disposition={t.disposition} data-trust={t.trust} data-answered={t.answered ? "true" : "false"}>
                  <strong>@{t.author}</strong> <span className="meta">[{t.trust}]</span> {t.kind}
                  {t.path && <> on <code>{t.path}{t.line !== "0" ? `:${t.line}` : ""}</code></>} · {t.disposition === "STEERED" ? (t.answered ? "steered — the agent has answered" : "steered — not yet answered") : `ignored (${t.reason})`}
                  {t.reportedBack ? " · reported back on the pull request" : ""}
                  {t.body && <pre className="small" data-testid="review-comment-body">{t.body}</pre>}
                </li>
              ))}
            </ul>
            <div className="actions">
              <button type="button" className="small" data-testid="review-ci-ingest" onClick={() => void ingestForge("ci")} disabled={forgeBusy}>
                Read CI results
              </button>{" "}
              <button type="button" className="small" data-testid="review-comments-ingest" onClick={() => void ingestForge("comments")} disabled={forgeBusy}>
                Read pull request comments
              </button>
            </div>
            {forgeNote && <div className="meta" data-testid="review-forge-note">{forgeNote}</div>}
            {forgeError && <div role="alert" className="error" data-testid="review-forge-error">{forgeError}</div>}
            {bundle.attributions.length > 0 && (
              <>
                <h3>Attribution</h3>
                <ul data-testid="review-attribution">
                  {bundle.attributions.map((a) => (
                    <li key={a}>{a}</li>
                  ))}
                </ul>
              </>
            )}
            {bundle.quarantined.length > 0 && (
              <>
                <h3>Quarantined (flaky)</h3>
                <ul>
                  {bundle.quarantined.map((q) => (
                    <li key={q}>{q}</li>
                  ))}
                </ul>
              </>
            )}
            {bundle.invariantFindings.length > 0 && (
              <>
                <h3>Diff invariants</h3>
                <ul>
                  {bundle.invariantFindings.map((q) => (
                    <li key={q}>{q}</li>
                  ))}
                </ul>
              </>
            )}
            <h3>Plan</h3>
            <pre className="small" data-testid="review-plan" tabIndex={0} aria-label="plan">{bundle.planJson || "(no plan recorded)"}</pre>
            <h3>Self-review</h3>
            <pre className="small" tabIndex={0} aria-label="self-review">{bundle.selfReviewJson || "(none)"}</pre>
            <h3>Evidence</h3>
            <ul className="small" data-testid="review-evidence" tabIndex={0} aria-label="evidence links">
              {bundle.evidenceLinks.map((e) => (
                <li key={e}>{e}</li>
              ))}
            </ul>
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
            <div className="meta">The push and the forge write are one protected effect: it runs only after you approve the exact intent shown, with the token the Core holds.</div>
            <div className="actions" data-testid="pull-request" data-status={pr?.status ?? "NONE"}>
              {(!pr || pr.status === "DENIED" || pr.status === "FAILED") && (
                <button type="button" data-testid="pr-open" onClick={() => void openPullRequest(false)} disabled={prBusy || (bundle.taskState !== "ReadyForReview" && !result?.startsWith("Accepted") && bundle.taskState !== "Completed")}>
                  {prBusy ? "Asking the Core…" : pr ? "Try again" : "Open pull request"}
                </button>
              )}
              {pr?.status === "APPROVAL_PENDING" && (
                <span data-testid="pr-approval">
                  <span className="meta">
                    approve intent <code data-testid="pr-intent">{pr.intentHash.slice(0, 16)}</code> (branch {pr.branch}, revision {pr.candidateRevision})?
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
                  {pr.status === "OPENED" ? "opened" : "updated"} #{pr.number} at {pr.url} · head {pr.headSha.slice(0, 12)} · {pr.receipts} receipt(s){pr.replayed ? " · replayed" : ""}{" "}
                  <button type="button" className="small" data-testid="pr-update" onClick={() => void openPullRequest(true)} disabled={prBusy}>
                    Update
                  </button>
                </span>
              )}
              {pr?.status === "DENIED" && (
                <span className="meta" role="status" data-testid="pr-denied">
                  denied: {pr.detail}
                </span>
              )}
              {prError && (
                <span className="meta" role="alert" data-testid="pr-error">
                  {prError}
                </span>
              )}
            </div>
          </aside>
        </div>
      )}
    </section>
  );
}
