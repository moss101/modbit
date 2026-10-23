/**
 * The three dialogs of the automations surface (REQ-PX-086; docs/68):
 *
 * - The enable approval (AUT-A04, AUT-D02, AUT-D03): shows the exact version,
 *   the full hash, the effects ceiling and the exact capabilities, paths and
 *   hosts the person is approving (and for a repository file, its path and
 *   revision), and sends EnableAutomation with exactly those values. It holds
 *   a snapshot taken when it opened, so an approval of a version that was
 *   edited meanwhile is refused by the Core (APPROVAL_MISMATCH) and the dialog
 *   says so, rather than quietly approving something else.
 * - The test run (AUT-E02): a dry run, optionally with a sample payload, that
 *   lists the protected effects it denied as what would have been asked.
 * - The kill confirmation (AUT-D05): says what it will stop and reports what
 *   it did, from the Core's counts.
 */
import { useEffect, useRef, useState } from "react";
import { Button, Dialog, StatusDot } from "@modbit/ui";
import type { AutoKillReport, AutoRun, AutoRunStarted, AutoView } from "../../shared/automation-types.ts";
import { newCommandId, type AutomationCalls } from "./calls.ts";
import { effectWords, enableApprovalOf, refusalOf, refusalSentence, runStatusMeta, samplePayloadFor, wouldHaveAskedOf } from "./model.ts";

function ListFacts({ label, items, testId }: { label: string; items: readonly string[]; testId: string }) {
  return (
    <>
      <dt>{label}</dt>
      <dd>
        {items.length === 0 ? (
          <span data-testid={testId} data-count={0}>
            none
          </span>
        ) : (
          <ul className="autos-list" data-testid={testId} data-count={items.length}>
            {items.map((x) => (
              <li key={x}>
                <code>{x}</code>
              </li>
            ))}
          </ul>
        )}
      </dd>
    </>
  );
}

// ------------------------------------------------------------ enable approval

export interface EnableDialogProps {
  /** The version being approved, as the Core described it when the dialog opened; null = closed. */
  snapshot: AutoView | null;
  /** The repository revision the file was loaded at (a repository definition only). */
  sourceRevision: string;
  calls: AutomationCalls;
  onClose: () => void;
  onEnabled: (view: AutoView) => void;
  /** Re-open the dialog on the Core's current version (after a stale refusal). */
  onReview: () => void;
}

export function EnableDialog({ snapshot, sourceRevision, calls, onClose, onEnabled, onReview }: EnableDialogProps) {
  const [ack, setAck] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<{ code: string; text: string } | null>(null);
  const commandId = useRef("");
  const key = snapshot ? `${snapshot.automationId}:${snapshot.currentVersion}:${snapshot.definitionHash}` : "";
  useEffect(() => {
    setAck(false);
    setBusy(false);
    setError(null);
    commandId.current = newCommandId();
  }, [key]);

  const v = snapshot;
  const listed = v?.needsListedApproval ?? false;
  const go = async () => {
    if (!v) return;
    setBusy(true);
    setError(null);
    try {
      // Exactly the values on this dialog; nothing is looked up again.
      const view = await calls.enable(enableApprovalOf(v), commandId.current);
      onEnabled(view);
    } catch (e) {
      const msg = (e as Error).message;
      setError({ code: refusalOf(msg).code, text: refusalSentence(msg) });
    } finally {
      setBusy(false);
    }
  };

  return (
    <Dialog open={v !== null} onClose={onClose} title="Approve this automation?" role="alertdialog" size="lg" testId="enable-dialog">
      {v && (
        <div className="autos-dialog" data-testid="enable-body" data-automation-id={v.automationId} data-version={v.currentVersion}>
          <p>
            You are approving exactly the version below. It lets triggers start runs for you, unattended, within the limits listed here. Any change to the definition makes a new version that needs a new approval.
          </p>
          <dl className="autos-facts">
            <dt>Name</dt>
            <dd data-testid="enable-name">{v.name}</dd>
            <dt>Version</dt>
            <dd data-testid="enable-version">{v.currentVersion}</dd>
            <dt>Definition hash</dt>
            <dd>
              <code className="autos-hash" data-testid="enable-hash">
                {v.definitionHash}
              </code>
            </dd>
            <dt>Effects ceiling</dt>
            <dd data-testid="enable-effects" data-effects={v.effects}>
              <strong>{v.effects}</strong>: {effectWords(v.effects)}
            </dd>
            <ListFacts label="Capabilities" items={v.capabilities} testId="enable-capabilities" />
            <ListFacts label="Paths it may write" items={v.paths} testId="enable-paths" />
            <ListFacts label="Hosts it may reach" items={v.hosts} testId="enable-hosts" />
            <dt>Runs as</dt>
            <dd data-testid="enable-principal">{v.principal === "creator" ? "you (the creator): your policy ceiling and credentials" : v.principal}</dd>
            <dt>Workspace</dt>
            <dd>
              <code>{v.workspaceRoot}</code>
            </dd>
            <dt>Triggers</dt>
            <dd>
              <ul className="autos-list">
                {v.triggers.map((t) => (
                  <li key={t.id}>
                    {t.id}: {t.summary}
                  </li>
                ))}
              </ul>
            </dd>
            <dt>Limits per run</dt>
            <dd>
              {v.limitDeadlineMinutes} min, {v.limitMaxCostMinor > 0 ? `${v.limitMaxCostMinor} minor units` : "no cost cap"}, a parked approval waits {v.limitApprovalWaitMinutes} min before it expires
            </dd>
            {v.sourceKind === "repository" && (
              <>
                <dt>Supplied by</dt>
                <dd data-testid="enable-source" data-path={v.sourcePath} data-revision={sourceRevision}>
                  the repository file <code>{v.sourcePath}</code> at revision <code>{sourceRevision || "(none recorded)"}</code>. The approval is bound to the bytes of that file as the Core read them, and the Core checks they are still the bytes on disk when you approve and before every run.
                </dd>
              </>
            )}
          </dl>
          {listed && (
            <label className="autos-ack">
              <input type="checkbox" checked={ack} onChange={(e) => setAck(e.target.checked)} data-testid="enable-ack" /> I have read the capabilities, paths and hosts above and approve exactly these.
            </label>
          )}
          {error && (
            <div className="autos-note" role="alert" data-testid="enable-error" data-code={error.code}>
              <p>{error.text}</p>
              {(error.code === "APPROVAL_MISMATCH" || error.code === "SOURCE_CHANGED") && (
                <Button size="sm" onClick={onReview} data-testid="enable-review-current">
                  Review the current version
                </Button>
              )}
            </div>
          )}
          <div className="dialog-actions">
            <Button onClick={onClose} disabled={busy} data-testid="enable-cancel">
              Cancel
            </Button>
            <Button variant="primary" disabled={busy || (listed && !ack)} onClick={() => void go()} data-testid="enable-confirm">
              Approve and enable version {v.currentVersion}
            </Button>
          </div>
        </div>
      )}
    </Dialog>
  );
}

// ------------------------------------------------------------------ test run

export interface TestRunDialogProps {
  view: AutoView | null;
  runs: readonly AutoRun[];
  calls: AutomationCalls;
  onClose: () => void;
  onRefresh: () => void;
  onOpenTask: (taskId: string) => void;
  /** The gallery pins a finished dry run. */
  fixed?: { started: AutoRunStarted } | undefined;
}

export function TestRunDialog({ view, runs, calls, onClose, onRefresh, onOpenTask, fixed }: TestRunDialogProps) {
  const [triggerId, setTriggerId] = useState("");
  const [payload, setPayload] = useState("");
  const [inputs, setInputs] = useState("");
  const [busy, setBusy] = useState(false);
  const [started, setStarted] = useState<AutoRunStarted | null>(fixed?.started ?? null);
  const [error, setError] = useState<string | null>(null);
  const commandId = useRef("");
  const key = view?.automationId ?? "";
  useEffect(() => {
    if (!view) return;
    const first = view.triggers.find((t) => t.kind === "manual") ?? view.triggers[0];
    setTriggerId(first?.id ?? "");
    setPayload(samplePayloadFor(first?.kind ?? ""));
    setInputs("");
    setStarted(fixed?.started ?? null);
    setError(null);
    setBusy(false);
    commandId.current = newCommandId();
    // Re-initialise when a different definition is opened.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key]);

  const trigger = view?.triggers.find((t) => t.id === triggerId);
  const takesPayload = trigger?.kind === "event" || trigger?.kind === "webhook";
  const run = started?.dispatchKey ? runs.find((r) => r.dispatchKey === started.dispatchKey) : undefined;
  const asked = run ? wouldHaveAskedOf(run.outputsJson) : null;
  const finished = run ? ["succeeded", "failed", "skipped", "cancelled"].includes(run.status) : false;

  const go = async () => {
    if (!view) return;
    setBusy(true);
    setError(null);
    setStarted(null);
    try {
      const res = await calls.run({ automationId: view.automationId, triggerId, test: true, ...(takesPayload ? { payloadJson: payload, source: trigger?.kind === "webhook" ? "webhook" : "forge" } : {}), ...(inputs.trim() ? { inputsJson: inputs } : {}) }, commandId.current);
      commandId.current = newCommandId();
      setStarted(res);
      onRefresh();
    } catch (e) {
      setError(refusalSentence((e as Error).message));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Dialog open={view !== null} onClose={onClose} title="Test run" size="lg" testId="test-dialog">
      {view && (
        <div className="autos-dialog" data-testid="test-body" data-automation-id={view.automationId}>
          <p>
            A test run is a dry run: it is read-only, every protected effect it tries is denied and listed below as what would have been asked, and it never changes your files. {takesPayload ? "The sample payload goes through the trigger's filters first." : ""}
          </p>
          <label className="autos-field">
            <span>Trigger to simulate</span>
            <select value={triggerId} onChange={(e) => { setTriggerId(e.target.value); const k = view.triggers.find((t) => t.id === e.target.value)?.kind ?? ""; setPayload(samplePayloadFor(k)); }} data-testid="test-trigger">
              {view.triggers.map((t) => (
                <option key={t.id} value={t.id}>
                  {t.id} ({t.kind}): {t.summary}
                </option>
              ))}
            </select>
          </label>
          {takesPayload && (
            <label className="autos-field">
              <span>Sample payload (JSON; untrusted data, never instructions)</span>
              <textarea className="autos-json" value={payload} onChange={(e) => setPayload(e.target.value)} rows={8} spellCheck={false} data-testid="test-payload" />
            </label>
          )}
          <details>
            <summary>Inputs (optional)</summary>
            <label className="autos-field">
              <span>A JSON object of the definition's typed inputs</span>
              <textarea className="autos-json" value={inputs} onChange={(e) => setInputs(e.target.value)} rows={3} spellCheck={false} data-testid="test-inputs" />
            </label>
          </details>
          {error && (
            <p className="autos-note" role="alert" data-testid="test-error">
              {error}
            </p>
          )}
          {started?.wouldSkipByFilter && (
            <p className="autos-note" role="status" data-testid="test-skipped">
              The trigger&apos;s filters would have skipped this payload, so nothing ran.
            </p>
          )}
          {started && !started.wouldSkipByFilter && (
            <div role="status" aria-live="polite" data-testid="test-result" data-status={run?.status ?? started.status} data-finished={finished}>
              <p>
                <StatusDot status={runStatusMeta(run?.status ?? started.status).tone} label={runStatusMeta(run?.status ?? started.status).label} showLabel /> {started.reason ? `(${started.reason}) ` : ""}
                {started.detail}
              </p>
              {run && finished && (
                <>
                  <p data-testid="test-denied-note">No protected effect was performed. {run.detail}</p>
                  {asked && asked.entries.length > 0 ? (
                    <>
                      <p>What it tried that would have needed an approval or was denied:</p>
                      <ul className="autos-list" data-testid="would-have-asked" data-count={asked.entries.length}>
                        {asked.entries.map((e, i) => (
                          <li key={i} data-testid="would-have-asked-entry" data-outcome={e.outcome} data-tool={e.tool}>
                            <code>{e.tool || "(tool)"}</code> {e.outcome === "WOULD_HAVE_ASKED" ? "would have asked for approval" : "was denied"}
                          </li>
                        ))}
                      </ul>
                    </>
                  ) : (
                    <p className="meta" data-testid="would-have-asked-none">
                      Nothing it tried needed an approval.
                    </p>
                  )}
                </>
              )}
              {run?.taskId && (
                <Button size="sm" onClick={() => onOpenTask(run.taskId)} data-testid="test-open-task">
                  Open the test run&apos;s task
                </Button>
              )}
            </div>
          )}
          <div className="dialog-actions">
            <Button onClick={onClose} data-testid="test-close">
              Close
            </Button>
            <Button variant="primary" disabled={busy || !trigger} onClick={() => void go()} data-testid="test-start">
              Run test
            </Button>
          </div>
        </div>
      )}
    </Dialog>
  );
}

// ----------------------------------------------------------------------- kill

export interface KillDialogProps {
  /** null = closed; automationId "" = every automation. */
  target: { automationId: string; name: string } | null;
  calls: AutomationCalls;
  onClose: () => void;
  onDone: () => void;
  /** The gallery pins a report. */
  fixedReport?: AutoKillReport | undefined;
}

export function KillDialog({ target, calls, onClose, onDone, fixedReport }: KillDialogProps) {
  const [busy, setBusy] = useState(false);
  const [report, setReport] = useState<AutoKillReport | null>(fixedReport ?? null);
  const [error, setError] = useState<string | null>(null);
  const key = target?.automationId ?? "\u0000";
  useEffect(() => {
    setBusy(false);
    setReport(fixedReport ?? null);
    setError(null);
    // Re-initialise when a different target is opened.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key]);
  const go = async () => {
    if (!target) return;
    setBusy(true);
    setError(null);
    try {
      setReport(await calls.kill(target.automationId));
      onDone();
    } catch (e) {
      setError(refusalSentence((e as Error).message));
    } finally {
      setBusy(false);
    }
  };
  const all = target?.automationId === "";
  return (
    <Dialog open={target !== null} onClose={onClose} title={all ? "Kill all automations?" : `Kill “${target?.name ?? ""}”?`} role="alertdialog" testId="kill-dialog">
      {target && (
        <div className="autos-dialog" data-testid="kill-body">
          {!report ? (
            <p>
              This pauses {all ? "every automation" : "this automation"}, drops the runs still waiting to start and cancels the runs in progress, which are put under an emergency stop. Nothing is deleted: resume {all ? "them" : "it"} from the list when you are ready.
            </p>
          ) : (
            <p role="status" data-testid="kill-report" data-cancelled={report.cancelledRuns} data-dropped={report.droppedQueued} data-paused={report.paused}>
              Done. {report.cancelledRuns} {report.cancelledRuns === 1 ? "run was" : "runs were"} cancelled and {report.droppedQueued} waiting {report.droppedQueued === 1 ? "run was" : "runs were"} dropped. {all ? "All automations are" : "This automation is"} paused.
            </p>
          )}
          {error && (
            <p className="autos-note" role="alert" data-testid="kill-error">
              {error}
            </p>
          )}
          <div className="dialog-actions">
            <Button onClick={onClose} data-testid="kill-close">
              {report ? "Close" : "Cancel"}
            </Button>
            {!report && (
              <Button variant="danger" disabled={busy} onClick={() => void go()} data-testid="kill-confirm">
                {all ? "Kill all" : "Kill"}
              </Button>
            )}
          </div>
        </div>
      )}
    </Dialog>
  );
}

