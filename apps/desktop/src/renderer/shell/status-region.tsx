import { StateLine } from "./state-line.tsx";
import type { AppState } from "../state/use-app.ts";

/** What must reach the person wherever they are: a pending confirmation, the Core's trouble, an error. Quiet states stay in the accessibility tree only. */
export function StatusRegion({ app }: { app: AppState }) {
  const { core, error, recovered, setRecovered, recovery, confirm, announcement, load, runKeyCommand, fleet } = app;
  return (
  <div role="region" aria-label="Status and notifications">
  <StateLine state={fleet} testid="fleet-state" />
  <p className="sr-only" role="status" aria-live="polite" data-testid="live-attention">{announcement}</p>
  {confirm && (
    <div className="banner" data-kind="error" role="alertdialog" aria-modal="false" aria-labelledby="confirm-text" data-testid="confirm" data-confirm-kind={confirm.kind} data-task-id={confirm.taskId}>
      <strong id="confirm-text">{confirm.text}</strong>{" "}
      <span className="meta">Enter confirms · Escape cancels</span>{" "}
      <button type="button" className="small" data-testid="confirm-yes" onClick={() => void runKeyCommand("confirm")}>Confirm</button>{" "}
      <button type="button" className="small" data-testid="confirm-no" onClick={() => void runKeyCommand("back")}>Cancel</button>
    </div>
  )}
  {core.state === "restarting" && (
    <div className="banner" data-kind="restarting" role="status" data-testid="banner-restarting">
      <strong>Core restarting</strong> — {core.reason}. Showing the last persisted state; retrying in {Math.round(core.retryInMs / 1000)}s. Nothing shown here is new progress.
    </div>
  )}
  {core.state === "failed" && (
    <div className="banner" data-kind="error" role="alert" data-testid="banner-failed">
      <strong>Core unavailable</strong> — {core.reason}. Last durable state is shown; restart the app to retry.
    </div>
  )}
  {recovered && core.state === "connected" && (
    <div className="banner" data-kind="recovered" role="status" data-testid="banner-recovered">
      <strong>Recovered</strong> — {recovered}
      {recovery ? ` (boot ${recovery.bootGeneration}): the Core verified ${recovery.eventsVerified} events across ${recovery.aggregatesVerified} aggregates and recovered ${recovery.sessions} session(s) and ${recovery.tasks} task(s) in ${recovery.recoveryMs} ms${recovery.projectionsRebuilt ? "; projections were rebuilt from the log" : ""}${recovery.notes.length ? `; still to reconcile: ${recovery.notes.join("; ")}` : "; nothing left to reconcile"}.` : "."}{" "}
      <button type="button" onClick={() => setRecovered(null)}>Dismiss</button>
    </div>
  )}
  {error && (
    <div className="banner" data-kind="error" role="alert" data-testid="banner-error">
      <strong>Error</strong> — {error}. <button type="button" onClick={() => void load()}>Retry</button>
    </div>
  )}
  </div>
  );
}
