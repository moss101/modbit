import { useEffect, useState } from "react";
import type { DashboardSummary } from "../../preload/preload.ts";
import "../state/types.ts";

/** An amount in minor units, shown in the registry's currency when priced. */
function money(minor: string, currency: string, scale: number): string {
  if (!currency) return `${minor} (minor units, unpriced)`;
  const n = BigInt(minor);
  const unit = 10n ** BigInt(scale);
  const frac = (n % unit).toString().padStart(scale, "0");
  return `${currency} ${(n / unit).toString()}${scale > 0 ? `.${frac}` : ""}`;
}

/** A latency figure; -1 means the rung was never reached. */
function ms(v: string): string {
  return v === "-1" ? "—" : `${v} ms`;
}

/**
 * M10.1: the session's telemetry, cost and SLO dashboard (docs/34), exactly
 * as the Core aggregates it from its log — the usage ledger, the SLO
 * ladder, tool outcomes, task states, recent failures and provider health.
 * Nothing here is estimated or sampled; a refresh asks the Core again.
 */
export function Dashboard({ sessionId, onClose }: { sessionId: string; onClose: () => void }) {
  const [view, setView] = useState<DashboardSummary | null>(null);
  const [error, setError] = useState<string | null>(null);
  const load = () => {
    window.modbit
      .dashboard(sessionId)
      .then((v) => {
        setView(v);
        setError(null);
      })
      .catch((e: unknown) => setError(String(e)));
  };
  useEffect(load, [sessionId]);
  return (
    <section className="dashboard" data-testid="dashboard" aria-label="Operations dashboard">
      <h2>Operations</h2>
      <p>
        <button type="button" data-testid="dashboard-refresh" onClick={load}>Refresh</button>{" "}
        <button type="button" data-testid="dashboard-close" onClick={onClose}>Close</button>
      </p>
      {error && <p className="error" data-testid="dashboard-error">{error}</p>}
      {!view && !error && <p className="meta" data-testid="dashboard-loading">Loading from the Core…</p>}
      {view && (
        <>
          <h3>Cost</h3>
          <p data-testid="dashboard-cost" data-cost-minor={view.cost.minor} data-currency={view.cost.currency}>
            {money(view.cost.minor, view.cost.currency, view.cost.scale)} — {view.cost.priced} call(s) priced, {view.cost.unpriced} unpriced, {view.cost.unreported} with usage never reported
          </p>
          <p data-testid="dashboard-tokens" data-input={view.tokens.input} data-output={view.tokens.output}>
            {view.tokens.input} input token(s) ({view.tokens.cached} cached), {view.tokens.output} output
          </p>
          <table data-testid="dashboard-models">
            <thead><tr><th>Model</th><th>Calls</th><th>Input</th><th>Output</th><th>Cost</th></tr></thead>
            <tbody>
              {view.models.map((m) => (
                <tr key={m.model} data-testid="dashboard-model-row" data-model={m.model} data-calls={m.calls} data-cost-minor={m.costMinor}>
                  <td>{m.model}</td><td>{m.calls}</td><td>{m.inputTokens}</td><td>{m.outputTokens}</td><td>{money(m.costMinor, view.cost.currency, view.cost.scale)}{m.unpricedCalls > 0 ? ` (+${m.unpricedCalls} unpriced)` : ""}</td>
                </tr>
              ))}
            </tbody>
          </table>
          <h3>Tasks</h3>
          <p data-testid="dashboard-states">{Object.entries(view.tasksByState).map(([k, n]) => `${k} ${n}`).join(", ") || "none"}</p>
          <table data-testid="dashboard-tasks">
            <thead><tr><th>Task</th><th>State</th><th>Model calls</th><th>Tool calls</th><th>Cost</th><th>Starts</th></tr></thead>
            <tbody>
              {view.tasks.map((t) => (
                <tr key={t.taskId} data-testid="dashboard-task-row" data-task-id={t.taskId} data-cost-minor={t.costMinor} data-model-calls={t.modelCalls} data-tool-calls={t.toolCalls}>
                  <td>{t.taskId.slice(0, 8)}</td><td>{t.state}</td><td>{t.modelCalls}</td><td>{t.toolCalls}</td><td>{money(t.costMinor, view.cost.currency, view.cost.scale)}{t.costComplete ? "" : " (incomplete)"}</td><td>{t.starts}</td>
                </tr>
              ))}
            </tbody>
          </table>
          <h3>Tools</h3>
          <p data-testid="dashboard-tools" data-succeeded={view.tools.succeeded} data-failed={view.tools.failed}>
            {view.tools.succeeded} succeeded, {view.tools.failed} failed, {view.tools.unknownOutcome} unknown outcome, {view.tools.cancelled} cancelled
          </p>
          <h3>SLO (cloud starts)</h3>
          <p data-testid="dashboard-slo-cold" data-starts={view.slo.cold.starts}>
            Cold: {view.slo.cold.starts} start(s); ready p50 {ms(view.slo.cold.readyP50Ms)}, max {ms(view.slo.cold.readyMaxMs)}; first token p50 {ms(view.slo.cold.firstTokenP50Ms)}, max {ms(view.slo.cold.firstTokenMaxMs)}
          </p>
          <p data-testid="dashboard-slo-warm" data-starts={view.slo.warm.starts}>
            Warm: {view.slo.warm.starts} start(s); ready p50 {ms(view.slo.warm.readyP50Ms)}, max {ms(view.slo.warm.readyMaxMs)}; first token p50 {ms(view.slo.warm.firstTokenP50Ms)}, max {ms(view.slo.warm.firstTokenMaxMs)}
          </p>
          <h3>Recent failures</h3>
          {view.failures.length === 0 ? (
            <p className="empty" data-testid="dashboard-no-failures">None</p>
          ) : (
            <ul data-testid="dashboard-failures">
              {view.failures.map((f) => (
                <li key={f.offset} data-testid="dashboard-failure" data-code={f.code}>{f.eventType} {f.class} {f.code} (task {f.taskId.slice(0, 8)})</li>
              ))}
            </ul>
          )}
          <h3>Providers</h3>
          <ul data-testid="dashboard-providers">
            {view.providers.map((p) => <li key={p}>{p}</li>)}
          </ul>
        </>
      )}
    </section>
  );
}
