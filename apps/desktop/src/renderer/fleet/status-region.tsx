import { StateLine } from "../shell/state-line.tsx";
import { Welcome } from "./welcome.tsx";
import type { AppState } from "../state/use-app.ts";

export function StatusRegion({ app }: { app: AppState }) {
  const { core, model, error, recovered, setRecovered, recovery, languages, inspector, economics, confirm, announcement, delivered, load, attention, runFleetCommand, fleet, notifications, open, onboarding } = app;
  return (
  <div role="region" aria-label="Status and notifications">
  <StateLine state={fleet} testid="fleet-state" />
  <p className="sr-only" role="status" aria-live="polite" data-testid="live-attention">{announcement}</p>
  {confirm && (
    <div className="banner" data-kind="error" role="alertdialog" aria-modal="false" aria-labelledby="confirm-text" data-testid="confirm" data-confirm-kind={confirm.kind} data-task-id={confirm.taskId}>
      <strong id="confirm-text">{confirm.text}</strong>{" "}
      <span className="meta">Enter confirms · Escape cancels</span>{" "}
      <button type="button" className="small" data-testid="confirm-yes" onClick={() => void runFleetCommand("confirm")}>Confirm</button>{" "}
      <button type="button" className="small" data-testid="confirm-no" onClick={() => void runFleetCommand("back")}>Cancel</button>
    </div>
  )}
  {notifications.length > 0 && (
    <section className="notifications" data-testid="notifications" aria-label="notifications" aria-live="polite">
      <ul>
        {notifications.map((n) => (
          <li key={n.id} data-testid="notification" data-kind={n.kind} data-id={n.id} data-coalesced={n.coalesced} data-count={n.count ?? 1} data-delivered={delivered.includes(`${n.id}:${n.reason}`) ? "true" : "false"}>
            <strong>{n.title}</strong> · {n.reason} · <em>{n.nextAction}</em>{" "}
            <button type="button" className="small" data-testid="notification-open" onClick={() => open(n.deepLink)}>
              Open {n.deepLink.screen}
            </button>
          </li>
        ))}
      </ul>
    </section>
  )}
  {inspector && inspector.packId !== "" && (
    <div className="banner" data-kind="info" data-testid="context-inspector">
      <strong>Context</strong> — {inspector.entries.filter((e) => e.injected).length} of {inspector.entries.length} fragment(s) injected, {inspector.tokenUsed}/{inspector.tokenBudget} tokens, {inspector.omittedCount} omitted
      {inspector.rejectedRefs.length > 0 ? `, ${inspector.rejectedRefs.length} refused for missing provenance` : ""}.
      {inspector.compactionEpochs > 0 && (
        <div data-testid="context-compaction">
          <strong>Compaction</strong> — epoch {inspector.compactionEpoch} of {inspector.compactionEpochs}, {String(inspector.compactedEntries)} earlier entr{Number(inspector.compactedEntries) === 1 ? "y" : "ies"} summarised and still in the log; prompt prefix reused on {inspector.prefixCacheHits} turn(s), rebuilt on {inspector.prefixCacheMisses}.
        </div>
      )}
      <ul>
        {inspector.entries.map((e) => (
          <li key={e.entryId} data-testid="context-entry">
            {e.sourceRef}
            {e.lineEnd > 0 ? ` L${e.lineStart}-${e.lineEnd}` : ""} — {e.reason}; {e.freshness}; {e.tokenCost} tokens
            {e.injected ? "; injected" : "; not injected"}
            {e.used ? "; used" : ""}
            {e.stub ? "; stub" : ""}
          </li>
        ))}
        {inspector.omittedPaths.map((p) => (
          <li key={p} data-testid="context-omitted">
            {p} — omitted for the token budget
          </li>
        ))}
      </ul>
    </div>
  )}
  {economics && economics.modelCalls > 0 && (
    <div className="banner" data-kind="info" data-testid="task-economics">
      <strong>Economics</strong> — {economics.verified ? "verified" : "not verified"}, {economics.checksPassed}/{economics.checksPassed + economics.checksFailed} check(s) passed; {economics.modelCalls} model call(s) on {economics.model || "no model"}, {economics.inputTokens} in ({economics.cachedInputTokens} cached) and {economics.outputTokens} out{economics.pricingKnown ? `, $${economics.costUsd.toFixed(4)} at catalog list prices` : ", price unknown"}; {economics.toolCalls} tool call(s); {economics.wallMs} ms wall, {economics.modelMs} ms in the model, {economics.toolMs} ms in tools; prompt prefix reused {economics.prefixCacheHits} of {economics.prefixCacheHits + economics.prefixCacheMisses} time(s).
    </div>
  )}
  {languages.length > 0 && (
    <div className="banner" data-kind="info" data-testid="language-labels" title={languages.map((l) => `${l.language}: ${l.label}`).join("\n")}>
      <strong>Languages</strong> — {languages.filter((l) => l.tier === "ALPHA_BASELINE").map((l) => l.language).join(", ")} at the Alpha baseline (Tier C plus compile and test evidence; structural intelligence not yet claimed); everything else unsupported.
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
  {onboarding && <Welcome app={app} />}
  </div>
  );
}
