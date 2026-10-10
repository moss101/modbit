/**
 * The attention trays that are not approvals (REQ-PX-060; docs/65 AFW-C09,
 * AFW-H02, AFW-H03): the Core unavailable, a budget exhausted (the typed
 * `BUDGET_EXHAUSTED` stop) and a model that policy blocks (`MODEL_NOT_ALLOWED`).
 * Which tray is due comes from the Core's typed codes (`trayPlan`); each one
 * docks above the composer, is never a modal, and goes away on its own when its
 * cause does. The budget tray raises the limits through the Core's own command
 * and resumes the run; it never edits a limit anywhere else.
 */
import { useEffect, useRef, useState } from "react";
import { Button, trays } from "@modbit/ui";
import type { AccountingInfo } from "../../shared/control-types.ts";
import type { TaskCard } from "../model.ts";
import { budgetBars, countFrom, formatCount, formatWall, trayPlan, type TraySpec } from "./model.ts";

export interface TrayDirectorProps {
  core: { state: "starting" | "connected" | "restarting" | "failed"; reason: string };
  sessionId: string | null;
  /** The open conversation's task, if any. */
  taskId: string | null;
  card: TaskCard | undefined;
  onResume: (taskId: string) => void;
}

interface Diag {
  code: string;
  detail: string;
  userAction: string;
}

export function TrayDirector({ core, sessionId, taskId, card, onResume }: TrayDirectorProps) {
  const [everConnected, setEverConnected] = useState(false);
  const [status, setStatus] = useState<{ taskId: string; diag: Diag } | null>(null);
  useEffect(() => {
    if (core.state === "connected") setEverConnected(true);
  }, [core.state]);

  // The Core's own status carries the typed code when the card has not seen the diagnostic event (a reload, a restart).
  const waiting = card?.state === "Waiting" || card?.state === "Failed";
  useEffect(() => {
    if (!taskId || !waiting || card?.diagnostic || core.state !== "connected") return;
    let live = true;
    window.modbit
      .taskStatus(taskId)
      .then((s) => live && setStatus({ taskId, diag: { code: s.failureCode, detail: s.attentionReason, userAction: s.userAction } }))
      .catch(() => {});
    return () => {
      live = false;
    };
  }, [taskId, waiting, card?.diagnostic, card?.state, card?.waitReason, core.state]);

  const diagnostic: Diag | null = !taskId ? null : card?.diagnostic ? { code: card.diagnostic.code, detail: card.diagnostic.detail, userAction: card.diagnostic.userAction } : status && status.taskId === taskId && waiting ? status.diag : null;
  const specs = trayPlan({ coreState: core.state, everConnected, coreReason: core.reason, diagnostic, taskId }).filter((s): s is PresentedSpec => s.kind !== "policy");
  const presented = useRef<Set<string>>(new Set());
  const specKey = specs.map((s) => `${s.id}:${s.title}:${s.text}`).join("|");

  useEffect(() => {
    const now = new Set<string>();
    for (const s of specs) {
      now.add(s.id);
      trays.present(trayFor(s, { sessionId, onResume, dismiss: () => trays.dismiss(s.id) }));
    }
    for (const id of presented.current) if (!now.has(id)) trays.dismiss(id);
    presented.current = now;
    // The spec list is rebuilt each render; its content is what matters.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [specKey, sessionId, onResume]);
  useEffect(
    () => () => {
      for (const id of presented.current) trays.dismiss(id);
      presented.current = new Set();
    },
    [],
  );
  return null;
}

/** The policy-blocked entry is the composer's (composer/trays.tsx PolicyTray, one TrayHost entry at priority 60), so this director never presents it. */
type PresentedSpec = Exclude<TraySpec, { kind: "policy" }>;

function trayFor(s: PresentedSpec, ctx:{ sessionId: string | null; onResume: (taskId: string) => void; dismiss: () => void }) {
  switch (s.kind) {
    case "offline":
      return { id: s.id, tone: "warn" as const, title: s.title, priority: 80, dismissible: false, body: <p data-testid="tray-offline-text">{s.text}</p> };
    case "budget":
      return { id: s.id, tone: "warn" as const, title: s.title, priority: 60, dismissible: true, body: <BudgetTray spec={s} sessionId={ctx.sessionId} onResume={ctx.onResume} onDone={ctx.dismiss} /> };
  }
}

/** The limit tray: what ran out, what the caps are now, and a way to raise them and continue. */
export function BudgetTray({ spec, sessionId, onResume, onDone }: { spec: Extract<TraySpec, { kind: "budget" }>; sessionId: string | null; onResume: (taskId: string) => void; onDone: () => void }) {
  const [acct, setAcct] = useState<AccountingInfo | null>(null);
  const [cost, setCost] = useState("");
  const [wall, setWall] = useState("");
  const [note, setNote] = useState("");
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    let live = true;
    window.modbit
      .contextAccounting(spec.taskId)
      .then((a) => live && setAcct(a))
      .catch(() => {});
    return () => {
      live = false;
    };
  }, [spec.taskId]);
  const bars = budgetBars(acct);
  const raise = async () => {
    if (!sessionId) return;
    const c = cost.trim() === "" ? undefined : countFrom(cost);
    const wsec = wall.trim() === "" ? undefined : countFrom(wall);
    if (c === null || wsec === null || (c === undefined && wsec === undefined)) {
      setNote("Give a new cap as a whole number: minor currency units for cost, seconds for time. 0 removes a cap.");
      return;
    }
    setBusy(true);
    setNote("");
    try {
      const b = acct?.budgets;
      await window.modbit.setTaskBudgets(sessionId, spec.taskId, { maxCostMinor: c ?? b?.maxCostMinor ?? "0", maxWallMs: wsec === undefined ? (b?.maxWallMs ?? "0") : String(BigInt(wsec) * 1000n) });
      onResume(spec.taskId);
      onDone();
    } catch (e) {
      setNote(`The Core refused: ${(e as Error).message.replace(/^Error invoking remote method '[^']*': (Error: )?/, "")}`);
    } finally {
      setBusy(false);
    }
  };
  return (
    <div className="ctx" data-testid="tray-budget">
      <p data-testid="tray-budget-text">{spec.text}</p>
      {bars.map((b) => (
        <p key={b.label} className="meta" data-testid="tray-budget-bar">
          {b.label}: {b.unit === "ms" ? `${formatWall(b.used)} of ${formatWall(b.cap)}` : `${formatCount(b.used)} of ${formatCount(b.cap)} minor currency units`}
        </p>
      ))}
      <div className="ctx-budget">
        <label>
          New cost cap (minor units)
          <input inputMode="numeric" value={cost} onChange={(e) => setCost(e.target.value)} disabled={busy} data-testid="tray-budget-cost" />
        </label>
        <label>
          New time cap (seconds)
          <input inputMode="numeric" value={wall} onChange={(e) => setWall(e.target.value)} disabled={busy} data-testid="tray-budget-wall" />
        </label>
        <Button size="sm" variant="primary" onClick={() => void raise()} disabled={busy} data-testid="tray-budget-raise">
          Raise and continue
        </Button>
      </div>
      {note && (
        <p className="appr-note" role="alert" data-testid="tray-budget-note">
          {note}
        </p>
      )}
      <p className="meta">Raising a cap records it on the task and resumes the run with its partial work. Choosing a cheaper model is in the composer's model picker.</p>
    </div>
  );
}
