/**
 * The isolated reviewer's findings on the Review page (REQ-PX-214, DR-PX-2026-11-11-021 MWS-18): its verdict in words, then each
 * finding with its severity, place, claim and suggested action, whether the Core found it in the code, and what a person can do:
 * Walk through (one open finding at a time), Fix this one (the Core queues one input naming that finding for the agent) and
 * Dismiss (the Core records who and why). Every fact is the Core's: a finding the Core could not tie to the tree is said so and is
 * never shown as confirmed; a report that cannot be read back intact shows nothing of it. The page computes nothing.
 */
import { useState } from "react";
import { Button } from "@modbit/ui";
import type { ReviewerFindingView, ReviewerResultView } from "../../shared/review-findings-types.ts";
import { findingPlace, findingRefusal, fixedWords, handledWords, openReviewerFindings, reviewerWords, severityWords, verifiedWords } from "./review-model.ts";

export interface ReviewerFindingsProps {
  sessionId: string;
  taskId: string;
  reviewer: ReviewerResultView;
  /** Reads the review again (after a command on a finding, and when the Core says the report moved on). */
  reload: () => Promise<void>;
}

const MAX_REASON = 500;

function FindingCard({ f, props, busy, run, step }: { f: ReviewerFindingView; props: ReviewerFindingsProps; busy: boolean; run: (what: () => Promise<string>) => Promise<void>; step?: string }) {
  const [asking, setAsking] = useState(false);
  const [reason, setReason] = useState("");
  const handled = handledWords(f);
  const place = findingPlace(f);
  const fix = () => run(async () => fixedWords((await window.modbit.fixReviewFinding(props.sessionId, props.taskId, f.findingId, props.reviewer.resultDigest)).taskState));
  const dismiss = () =>
    run(async () => {
      await window.modbit.dismissReviewFinding(props.sessionId, props.taskId, f.findingId, props.reviewer.resultDigest, reason.trim());
      setAsking(false);
      setReason("");
      return "Set this finding aside.";
    });
  return (
    <li className="rp-finding rv-finding" data-testid="reviewer-finding" data-finding-id={f.findingId} data-severity={f.severity} data-verified={f.verified ? "true" : "false"} data-dismissed={f.dismissed ? "true" : "false"}>
      <span className="rp-tag rv-severity" data-severity={f.severity}>
        {severityWords(f.severity)}
      </span>
      <span className="rp-finding-text">
        {step && (
          <span className="meta" data-testid="reviewer-step">
            {step}
          </span>
        )}
        {place && <code className="rp-path">{place}</code>}
        <span data-testid="reviewer-claim">{f.claim}</span>
        {f.suggestedAction && (
          <span className="meta" data-testid="reviewer-suggested">
            Suggested: {f.suggestedAction}
          </span>
        )}
        <span className="meta rv-check" data-testid="reviewer-verified" data-verified={f.verified ? "true" : "false"}>
          {verifiedWords(f)}
        </span>
        {handled && (
          <span className="meta" data-testid="reviewer-handled">
            {handled}
          </span>
        )}
        {!f.dismissed && !asking && (
          <span className="rp-actions">
            <Button size="sm" variant="outline" disabled={busy} onClick={() => void fix()} data-testid="reviewer-fix">
              Fix this one
            </Button>
            <Button size="sm" variant="ghost" disabled={busy} onClick={() => setAsking(true)} data-testid="reviewer-dismiss">
              Dismiss
            </Button>
          </span>
        )}
        {!f.dismissed && asking && (
          <span className="rp-actions">
            <input className="rv-reason" aria-label="Why are you setting this finding aside?" placeholder="Why are you setting this aside?" maxLength={MAX_REASON} value={reason} autoFocus onChange={(e) => setReason(e.target.value)} onKeyDown={(e) => e.key === "Enter" && reason.trim() && !busy && void dismiss()} data-testid="reviewer-dismiss-reason" />
            <Button size="sm" variant="outline" disabled={busy || reason.trim().length === 0} onClick={() => void dismiss()} data-testid="reviewer-dismiss-confirm">
              Set aside
            </Button>
            <Button size="sm" variant="ghost" disabled={busy} onClick={() => (setAsking(false), setReason(""))} data-testid="reviewer-dismiss-cancel">
              Cancel
            </Button>
          </span>
        )}
      </span>
    </li>
  );
}

export function ReviewerFindings(props: ReviewerFindingsProps) {
  const { reviewer } = props;
  const words = reviewerWords(reviewer);
  const [walking, setWalking] = useState<number | null>(null);
  const [busy, setBusy] = useState(false);
  const [note, setNote] = useState("");
  const [error, setError] = useState("");
  if (!words) return null;
  const open = openReviewerFindings(reviewer);
  const run = async (what: () => Promise<string>) => {
    setBusy(true);
    setError("");
    setNote("");
    try {
      setNote(await what());
    } catch (e) {
      setError(findingRefusal((e as Error).message));
    } finally {
      setBusy(false);
      await props.reload();
    }
  };
  // The walk-through shows one open finding at a time; a finding set aside leaves it, so the position never runs past the end.
  const at = walking === null ? null : Math.min(walking, Math.max(open.length - 1, 0));
  const current = at !== null ? open[at] : undefined;
  return (
    <div className="rv" data-testid="reviewer" data-state={reviewer.state} data-verdict={reviewer.verdict}>
      <h4 className="rp-label">The reviewer's findings</h4>
      <p className="rv-headline" data-testid="reviewer-headline">
        {words.headline}
      </p>
      {words.detail && (
        <p className="meta" data-testid="reviewer-detail">
          {words.detail}
        </p>
      )}
      {words.stale && (
        <p className="meta" role="status" data-testid="reviewer-stale">
          This is from an earlier version of the change. The agent has changed it since, so a finding may no longer apply.
        </p>
      )}
      {!words.unreadable && (
        <>
          <div className="rp-actions">
            {walking === null ? (
              <Button size="sm" variant="outline" disabled={open.length === 0} onClick={() => setWalking(0)} data-testid="reviewer-walk">
                Walk through
              </Button>
            ) : (
              <>
                <Button size="sm" variant="ghost" disabled={at === 0} onClick={() => setWalking(Math.max((at ?? 0) - 1, 0))} data-testid="reviewer-prev">
                  Previous
                </Button>
                <Button size="sm" variant="ghost" disabled={at === open.length - 1 || open.length === 0} onClick={() => setWalking((at ?? 0) + 1)} data-testid="reviewer-next">
                  Next
                </Button>
                <Button size="sm" variant="outline" onClick={() => setWalking(null)} data-testid="reviewer-walk-done">
                  Show them all
                </Button>
              </>
            )}
            <span className="meta" data-testid="reviewer-count">
              {reviewer.findings.length === 0 ? "No findings." : `${open.length} of ${reviewer.findings.length} still open.`}
            </span>
          </div>
          {walking !== null ? (
            current ? (
              <ul className="rp-list" data-testid="reviewer-findings">
                <FindingCard key={current.findingId} f={current} props={props} busy={busy} run={run} step={`Finding ${(at ?? 0) + 1} of ${open.length}`} />
              </ul>
            ) : (
              <p className="meta" data-testid="reviewer-walk-empty">
                No finding is left open.
              </p>
            )
          ) : (
            reviewer.findings.length > 0 && (
              <ul className="rp-list" data-testid="reviewer-findings">
                {reviewer.findings.map((f) => (
                  <FindingCard key={f.findingId} f={f} props={props} busy={busy} run={run} />
                ))}
              </ul>
            )
          )}
          {reviewer.questions.length > 0 && (
            <>
              <h4 className="rp-label">What the reviewer could not settle</h4>
              <ul className="rp-list" data-testid="reviewer-questions">
                {reviewer.questions.map((q) => (
                  <li key={q} className="rp-finding">
                    <span className="rp-finding-text">{q}</span>
                  </li>
                ))}
              </ul>
            </>
          )}
        </>
      )}
      {note && (
        <p className="meta" role="status" data-testid="reviewer-note">
          {note}
        </p>
      )}
      {error && (
        <p className="meta" role="alert" data-testid="reviewer-error">
          The Core refused: {error}.
        </p>
      )}
    </div>
  );
}
