/**
 * The context ring and its tray (REQ-PX-060; docs/65 AFW-D12, AFW-H04..H06): a
 * small ring in the status row shows how much of the model's window the last
 * request used; a click docks a tray with the Core's nine categories (they sum
 * to the total the Core counted), the usage summary and the budget caps. The
 * numbers are the Core's (GetContextAccounting, GetTaskEconomics); the
 * renderer formats them and keeps only the person's choice of when the usage
 * summary shows.
 */
import { useCallback, useEffect, useRef, useState, useSyncExternalStore } from "react";
import { trays } from "@modbit/ui";
import type { AccountingInfo } from "../../shared/control-types.ts";
import type { TaskEconomicsSummary } from "../../preload/preload.ts";
import { budgetBars, categoryLabel, CATEGORY_LABELS, formatCount, formatWall, nearLimit, percentUsed, ringLabel, ringLevel, shareLabel, sumsToTotal, usageVisible, type UsageSetting } from "./model.ts";
import { loadUsageSetting, saveUsageSetting } from "./prefs.ts";

const TRAY_ID = "context-usage";
const HOLD_MS = 400;

interface WireEventLike {
  taskId: string | null;
  aggregateType: string;
}

/** The Core's accounting for one task, re-read when the task's events pass (a turn ends, a request is compiled). */
export function useAccounting(taskId: string | null, connected: boolean): { accounting: AccountingInfo | null; economics: TaskEconomicsSummary | null; refresh: () => void } {
  const [accounting, setAccounting] = useState<AccountingInfo | null>(null);
  const [economics, setEconomics] = useState<TaskEconomicsSummary | null>(null);
  const key = useRef(taskId);
  key.current = taskId;
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const inFlight = useRef(false);
  const again = useRef(false);

  const load = useCallback(async () => {
    const tid = key.current;
    if (!tid) return;
    if (inFlight.current) {
      again.current = true;
      return;
    }
    inFlight.current = true;
    try {
      const [a, e] = await Promise.all([window.modbit.contextAccounting(tid), window.modbit.taskEconomics(tid).catch(() => null)]);
      if (key.current === tid) {
        setAccounting(a);
        setEconomics(e);
      }
    } catch {
      // Unreachable Core: the last numbers stay.
    } finally {
      inFlight.current = false;
      if (again.current) {
        again.current = false;
        void load();
      }
    }
  }, []);
  const refresh = useCallback(() => {
    if (timer.current) return;
    timer.current = setTimeout(() => {
      timer.current = null;
      void load();
    }, HOLD_MS);
  }, [load]);

  useEffect(() => {
    setAccounting(null);
    setEconomics(null);
  }, [taskId]);
  useEffect(() => {
    if (taskId && connected) void load();
  }, [taskId, connected, load]);
  useEffect(() => {
    if (!taskId) return;
    const off = window.modbit.onEvent((raw) => {
      const e = raw as WireEventLike;
      if (e.taskId === taskId && e.aggregateType !== "assistant_stream") refresh();
    });
    return () => {
      off();
      if (timer.current) clearTimeout(timer.current);
      timer.current = null;
    };
  }, [taskId, refresh]);
  return { accounting, economics, refresh };
}

export function ContextBreakdown({ accounting, economics, setting, onSetting }: { accounting: AccountingInfo | null; economics: TaskEconomicsSummary | null; setting: UsageSetting; onSetting: (s: UsageSetting) => void }) {
  if (!accounting || !accounting.available) {
    return (
      <div className="ctx" data-testid="context-tray">
        <p data-testid="context-none">The Core has not compiled a request for this task yet, so there is nothing to count. The ring fills after the first turn.</p>
      </div>
    );
  }
  const total = BigInt(accounting.totalTokens);
  const bars = budgetBars(accounting);
  const near = nearLimit(accounting);
  const showUsage = usageVisible(setting, near);
  return (
    <div className="ctx" data-testid="context-tray" data-total={accounting.totalTokens} data-sums={sumsToTotal(accounting)}>
      <p data-testid="context-headline">
        <strong>{formatCount(accounting.totalTokens)}</strong> tokens in the last request
        {percentUsed(accounting) !== null ? (
          <>
            {" "}
            of <strong>{formatCount(accounting.windowTokens)}</strong> ({percentUsed(accounting)}%)
          </>
        ) : (
          <> (the model's window is not known)</>
        )}
        {accounting.model ? <> on {accounting.model}</> : null}.
      </p>
      <table aria-label="Context by category" data-testid="context-categories">
        <thead>
          <tr>
            <th scope="col">Category</th>
            <th scope="col" className="num">
              Tokens
            </th>
            <th scope="col" className="num">
              Share
            </th>
            <th scope="col">
              <span className="mb-sr-only">Bar</span>
            </th>
          </tr>
        </thead>
        <tbody>
          {accounting.categories.map((c) => (
            <tr key={c.category} data-testid="context-category" data-category={c.category} data-tokens={c.tokens} title={CATEGORY_LABELS[c.category]?.detail ?? ""}>
              <th scope="row">{categoryLabel(c.category)}</th>
              <td className="num">{formatCount(c.tokens)}</td>
              <td className="num">{shareLabel(c.shareBp)}</td>
              <td className="ctx-bar-cell" aria-hidden="true">
                <span className="ctx-bar" style={{ width: `${Math.max(1, c.shareBp / 100)}%` }} />
              </td>
            </tr>
          ))}
        </tbody>
        <tfoot>
          <tr>
            <th scope="row">Total</th>
            <td className="num" data-testid="context-total">
              {formatCount(total)}
            </td>
            <td className="num" />
            <td />
          </tr>
        </tfoot>
      </table>
      <p className="meta" data-testid="context-source">
        {accounting.totalSource === "PROVIDER_REPORTED" ? "The total is what the provider reported counting." : `The total is Modbit's estimate (it can be off by up to ${shareLabel(accounting.declaredErrorBp)}), because the provider did not report a count.`}
        {accounting.windowSource === "UNKNOWN" ? " The model's window is unknown." : ` The window comes from the ${accounting.windowSource === "REGISTRY" ? "model registry" : "endpoint"}.`}
        {accounting.compactionSummaries > 0 ? ` ${accounting.compactionSummaries} earlier ${accounting.compactionSummaries === 1 ? "stretch is" : "stretches are"} summarised.` : ""}
      </p>
      {bars.length > 0 && (
        <div data-testid="context-budgets">
          {bars.map((b) => (
            <p key={b.label} className="meta" data-testid="context-budget" data-percent={b.percent}>
              {b.label}: {b.unit === "ms" ? `${formatWall(b.used)} of ${formatWall(b.cap)}` : `${formatCount(b.used)} of ${formatCount(b.cap)} minor currency units`} ({b.percent}%)
            </p>
          ))}
        </div>
      )}
      <label className="meta">
        Usage summary{" "}
        <select value={setting} onChange={(e) => onSetting(e.target.value as UsageSetting)} data-testid="usage-setting">
          <option value="auto">only near a limit</option>
          <option value="always">always</option>
          <option value="never">never</option>
        </select>
      </label>
      {showUsage && economics && (
        <dl className="ctx-usage" data-testid="usage-summary">
          <dt>Requests</dt>
          <dd>{economics.modelCalls}</dd>
          <dt>Tokens</dt>
          <dd>
            {formatCount(economics.inputTokens)} in ({formatCount(economics.cachedInputTokens)} cached), {formatCount(economics.outputTokens)} out
          </dd>
          <dt>Cost</dt>
          <dd data-testid="usage-cost">{economics.pricingKnown ? `$${economics.costUsd.toFixed(4)}` : "not priced (the model has no price in the registry)"}</dd>
          <dt>Time</dt>
          <dd>
            {formatWall(economics.wallMs)} ({formatWall(economics.modelMs)} in the model)
          </dd>
        </dl>
      )}
    </div>
  );
}

const RADIUS = 7;
const CIRC = 2 * Math.PI * RADIUS;

/** The ring itself, as a button. */
export function RingButton({ accounting, open, onToggle }: { accounting: AccountingInfo | null; open: boolean; onToggle: () => void }) {
  const level = ringLevel(accounting);
  const p = percentUsed(accounting);
  const frac = p === null ? 0 : p / 100;
  return (
    <button type="button" className="ring" data-level={level} aria-expanded={open} aria-label={`Context: ${ringLabel(accounting)}. Show the breakdown`} onClick={onToggle} data-testid="context-ring" data-percent={p ?? ""} data-total={accounting?.available ? accounting.totalTokens : ""}>
      <svg viewBox="0 0 20 20" aria-hidden="true" focusable="false">
        <circle className="ring-track" cx="10" cy="10" r={RADIUS} />
        <circle className="ring-fill" cx="10" cy="10" r={RADIUS} strokeDasharray={`${CIRC * frac} ${CIRC}`} />
      </svg>
      <span data-testid="context-ring-text">{p === null ? (accounting?.available ? "?" : "—") : `${p}%`}</span>
    </button>
  );
}

/** The status row's context ring for the open task: shows the Core's usage and docks the breakdown tray on a click. */
export function ContextRing({ taskId, connected }: { taskId: string; connected: boolean }) {
  const { accounting, economics } = useAccounting(taskId, connected);
  const [setting, setSetting] = useState<UsageSetting>(loadUsageSetting);
  const snap = useSyncExternalStore(trays.subscribe, trays.getSnapshot, trays.getSnapshot);
  const open = snap.trays.some((t) => t.id === TRAY_ID);
  const set = useCallback((s: UsageSetting) => {
    setSetting(s);
    saveUsageSetting(s);
  }, []);
  // While the tray is docked it follows the numbers.
  useEffect(() => {
    if (!open) return;
    trays.present({ id: TRAY_ID, tone: "info", title: `Context: ${ringLabel(accounting)}`, priority: 10, body: <ContextBreakdown accounting={accounting} economics={economics} setting={setting} onSetting={set} /> });
  }, [open, accounting, economics, setting, set]);
  useEffect(() => () => trays.dismiss(TRAY_ID), [taskId]);
  const toggle = () => {
    if (open) trays.dismiss(TRAY_ID);
    else trays.present({ id: TRAY_ID, tone: "info", title: `Context: ${ringLabel(accounting)}`, priority: 10, body: <ContextBreakdown accounting={accounting} economics={economics} setting={setting} onSetting={set} /> });
  };
  return <RingButton accounting={accounting} open={open} onToggle={toggle} />;
}
