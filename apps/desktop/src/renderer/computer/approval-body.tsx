/**
 * The body of the approval card for a native-control action or observation
 * grant (PX-074; docs/66 CUC-C01, CUC-C03): the exact per-call intent - the
 * application's identity as the operating system vouches for it, the window,
 * the element or screenshot the input lands on, the action and its arguments in
 * full, the scope and when the approval expires - and a before-thumbnail of the
 * last masked frame. There is no Always, no allowlist and no run-everything
 * path: native control is approved one call at a time, and any change in the
 * application, window or element is a different intent that asks again.
 * Everything the application or the model wrote is quoted text.
 */
import type { ComputerIntentView } from "../../shared/computer-types.ts";
import { ComputerFrame } from "./action-card.tsx";
import { modalityWords } from "./model.ts";

const clock = (ms: number): string => new Date(ms).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" });

export function targetLine(c: ComputerIntentView): string {
  const t = c.target;
  if (!t) return "";
  if (t.kind === "element") return `${t.role || "element"} “${t.name}” (${t.snapshot}/${t.id})`;
  if (t.kind === "coordinate") return `a point in screenshot ${t.frame} (window version ${t.windowVersion})`;
  return "whatever has the keyboard focus";
}

export function ComputerApprovalBody({ taskId, intent: c, expiresAtMs }: { taskId: string; intent: ComputerIntentView; expiresAtMs: number }) {
  const grant = c.kind === "observation_grant";
  return (
    <div className="cc-appr" data-testid="computer-approval" data-kind={c.kind} data-scope={c.scope} data-tool={c.tool}>
      <dl className="appr-intent" data-testid="approval-intent">
        <dt>Application</dt>
        <dd data-testid="computer-approval-app">
          <q>{c.applicationName || c.application.bundleId}</q> <code data-testid="computer-approval-bundle">{c.application.bundleId}</code>
        </dd>
        <dt>Identity</dt>
        <dd data-testid="computer-approval-identity">
          process {c.application.pid}, <code>{c.application.executablePath}</code>, signed by <code>{c.application.signingIdentity || "no signature"}</code>
        </dd>
        <dt>Window</dt>
        <dd data-testid="computer-approval-window">
          <q>{c.window.title}</q> <code>{c.window.id}</code>
        </dd>
        {!grant && (
          <>
            <dt>Where</dt>
            <dd data-testid="computer-approval-target">{targetLine(c)}</dd>
            <dt>Action</dt>
            <dd data-testid="computer-approval-action">
              <strong>{c.action}</strong> {modalityWords(c.modality)}
            </dd>
            <dt>Arguments</dt>
            <dd data-testid="computer-approval-args">
              <code className="appr-command">{c.argumentsText || "none"}</code>
              {c.argumentsTruncated && <span className="cc-fine"> (a very long value is clipped here)</span>}
            </dd>
          </>
        )}
        <dt>Scope</dt>
        <dd data-testid="computer-approval-scope">
          {c.scope === "SCREEN" ? (
            <>
              <strong>Whole screen.</strong> This takes the display and the real mouse and shows a Stop control for as long as it lasts. It ends with this turn.
            </>
          ) : (
            <>
              <strong>This application only.</strong> It works in the background through the application's interface; your mouse and screen stay yours.
            </>
          )}
        </dd>
        {grant && c.grant && (
          <>
            <dt>Lasts</dt>
            <dd data-testid="computer-approval-grant">
              {c.grant.endsWith}, at most {Math.round(c.grant.maxSeconds / 60)} minutes, {c.grant.callCap} observations. Every action it takes is approved separately.
            </dd>
            {c.reason && (
              <>
                <dt>The agent says</dt>
                <dd data-testid="computer-approval-reason">
                  <q>{c.reason}</q> <span className="cc-fine">(the agent's own words)</span>
                </dd>
              </>
            )}
          </>
        )}
        <dt>Expires</dt>
        <dd data-testid="computer-approval-expiry">{expiresAtMs > 0 ? `${clock(expiresAtMs)}, then it can no longer be approved` : "when the turn ends"}</dd>
        <dt>Can be undone</dt>
        <dd>{grant ? "Observing changes nothing." : c.reversible ? "Yes." : "No. An input to an application cannot be taken back from here."}</dd>
      </dl>
      {c.beforeArtifact && (
        <div data-testid="computer-approval-before">
          <p className="cc-fine">Before this {grant ? "request" : "action"}, the agent last saw:</p>
          <ComputerFrame taskId={taskId} artifactRef={c.beforeArtifact} expired={false} />
        </div>
      )}
      <p className="cc-fine" data-testid="computer-approval-rule">
        Approved one call at a time. No “always allow”, no allowlist and no run mode covers native control, and if the application, window or element changes this approval stops applying and you are asked again.
      </p>
    </div>
  );
}
