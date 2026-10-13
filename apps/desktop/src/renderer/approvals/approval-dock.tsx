/**
 * The approval stack's container (REQ-PX-058): reads the task's pending
 * approvals from the Core, docks them as one tray above the composer and makes
 * the typed decision calls. Nothing is remembered here: a thread switch, a
 * reload or a Core restart finds the same cards because the Core lists them.
 * A decision names the intent hash of the card that is on screen; the Core
 * refuses any other (INTENT_MISMATCH) and main checks it again before a rule is
 * made. Skip is a typed user decision (denied by user); there is no batch.
 */
import { useCallback, useEffect, useRef, useState } from "react";
import { trays } from "@modbit/ui";
import type { DockApprovalView } from "../../shared/control-types.ts";
import { ApprovalDeck } from "./approval-deck.tsx";
import { defaultPrefix, humanTitle, isIrreversible, refusalWords, surfacedIndex } from "./model.ts";
import { useDockApprovals } from "./use-dock-approvals.ts";

const TRAY_ID = "approvals";

export interface ApprovalDockProps {
  sessionId: string | null;
  taskId: string;
  connected: boolean;
  onChangeMode: () => void;
  titleOf: (taskId: string) => string;
  onOpenTask: (taskId: string) => void;
}

export function ApprovalDock({ sessionId, taskId, connected, onChangeMode, titleOf, onOpenTask }: ApprovalDockProps) {
  const { approvals, expired, refresh } = useDockApprovals(sessionId, taskId, connected);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [confirming, setConfirming] = useState(false);
  const [busy, setBusy] = useState(false);
  const [note, setNote] = useState<{ kind: "ok" | "refused"; text: string } | null>(null);
  const [prefix, setPrefix] = useState<string[]>([]);
  const [scope, setScope] = useState<"TASK" | "REPO">("REPO");
  // The parent's callbacks change identity on every render; the tray must not be re-presented for that.
  const outer = useRef({ onChangeMode, titleOf, onOpenTask });
  outer.current = { onChangeMode, titleOf, onOpenTask };
  const changeMode = useCallback(() => outer.current.onChangeMode(), []);
  const titleOfStable = useCallback((id: string) => outer.current.titleOf(id), []);
  const openTask = useCallback((id: string) => outer.current.onOpenTask(id), []);
  const index = surfacedIndex(approvals, selectedId);
  const current: DockApprovalView | undefined = approvals[index];
  const currentRef = useRef(current);
  currentRef.current = current;

  // A different card surfaced: the second step and the prefix start over, so a choice never carries to another effect.
  const surfaced = current?.approvalId ?? null;
  const intentOfSurfaced = current?.intentHash ?? null;
  useEffect(() => {
    setConfirming(false);
    setPrefix(current?.intent.argv ? defaultPrefix(current.intent.argv) : []);
    // Only the surfaced card's identity matters here.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [surfaced, intentOfSurfaced]);

  const decide = useCallback(
    async (approve: boolean) => {
      const a = currentRef.current;
      if (!sessionId || !a) return;
      setBusy(true);
      setNote(null);
      try {
        // The hash is the one this card showed; the Core refuses any other.
        const r = await window.modbit.resolveApproval(sessionId, a.approvalId, approve, approve ? "approved on the approval card" : "skipped by the user", a.intentHash);
        setNote({ kind: "ok", text: approve ? `Approved (${r.status.toLowerCase()}). It runs once.` : "Skipped. The agent is told you skipped it." });
      } catch (e) {
        setNote({ kind: "refused", text: refusalWords((e as Error).message) });
      } finally {
        setBusy(false);
        setConfirming(false);
        refresh();
      }
    },
    [sessionId, refresh],
  );

  const run = useCallback(() => {
    const a = currentRef.current;
    if (!a) return;
    if (isIrreversible(a.effectClass)) setConfirming(true);
    else void decide(true);
  }, [decide]);

  const always = useCallback(async () => {
    const a = currentRef.current;
    if (!sessionId || !a || prefix.length === 0) return;
    setBusy(true);
    setNote(null);
    try {
      const r = await window.modbit.allowAlways(sessionId, a.approvalId, a.intentHash, { pattern: prefix, scope, expiresAtMs: 0, coversAlwaysAsk: false });
      setNote({ kind: "ok", text: `Allowed “${prefix.join(" ")}” in ${scope === "REPO" ? "this repository" : "this task"} (rule ${r.rule.ruleId.slice(0, 8)}). It applies when the run mode is Allowlist or higher. This effect was approved once.` });
    } catch (e) {
      setNote({ kind: "refused", text: refusalWords((e as Error).message) });
    } finally {
      setBusy(false);
      refresh();
    }
  }, [sessionId, prefix, scope, refresh]);

  useEffect(() => {
    if (approvals.length === 0) {
      trays.dismiss(TRAY_ID);
      return;
    }
    const n = approvals.length;
    trays.present({
      id: TRAY_ID,
      tone: "attention",
      title: n > 1 ? `${n} approvals need you` : `Approval needed: ${current ? humanTitle(current.toolName) : "a protected action"}`,
      priority: 100,
      dismissible: false,
      body: (
        <ApprovalDeck
          approvals={approvals}
          index={index}
          onIndex={(i) => {
            setSelectedId(approvals[i]?.approvalId ?? null);
            setNote(null);
          }}
          busy={busy}
          confirming={confirming}
          prefix={prefix}
          onPrefix={setPrefix}
          scope={scope}
          onScope={setScope}
          note={note?.kind === "refused" ? note : null}
          onRun={run}
          onConfirm={() => void decide(true)}
          onBack={() => setConfirming(false)}
          onSkip={() => void decide(false)}
          onAlways={() => void always()}
          onChangeMode={changeMode}
          openTaskId={taskId}
          titleOf={titleOfStable}
          onOpenTask={openTask}
        />
      ),
    });
  }, [approvals, index, current, busy, confirming, prefix, scope, note, run, decide, always, changeMode, taskId, titleOfStable, openTask]);

  useEffect(() => () => trays.dismiss(TRAY_ID), []);
  // What the last decision did stays on screen after its card is gone, and is announced.
  useEffect(() => {
    setNote(null);
  }, [taskId]);
  return (
    <>
      {note?.kind === "ok" && (
        <p className="meta appr-last" role="status" data-testid="approval-last">
          {note.text}
        </p>
      )}
      {expired.map((e) => (
        <p key={e.approvalId} className="meta appr-last" role="status" data-testid="approval-expired" data-state="EXPIRED" data-approval-id={e.approvalId} data-intent-hash={e.intentHash}>
          Expired: “{humanTitle(e.toolName)}” (<code>{e.toolName}</code>) was not answered in time, so it can no longer be approved and it did not run. The agent was told; if it still needs the effect it asks again.
        </p>
      ))}
    </>
  );
}
